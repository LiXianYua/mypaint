//! stroke_to 核心算法。逐行翻译 mypaint-brush.c。
//!
//! 对应函数:
//! - directional_offsets (L586-664)
//! - update_states_and_setting_values (L708-904)
//! - fetch_smudge_bucket (L906-918)
//! - update_smudge_color (L920-997)
//! - apply_smudge (L999-1035)
//! - prepare_and_draw_dab (L1042-1250)
//! - count_dabs_to (L1253-1287)
//! - mypaint_brush_stroke_to (L1300-1547)

use crate::brush::Brush;
use crate::render::color::*;
use crate::render::DabParams;
use crate::smudge::mix_colors;
use crate::surface::Surface;
use crate::util::helpers::{mod_arith, rand_gauss, smallest_angular_difference, WGM_EPSILON};
use crate::BrushInput;
use crate::BrushSetting;
use crate::NUM_INPUTS;

const ACTUAL_RADIUS_MIN: f32 = 0.2;
const ACTUAL_RADIUS_MAX: f32 = 1000.0;
const GRID_SIZE: f32 = 256.0;

/// Smudge bucket field indices (matching C enum).
const SMUDGE_R: usize = 0;
const SMUDGE_G: usize = 1;
const SMUDGE_B: usize = 2;
const SMUDGE_A: usize = 3;
const PREV_COL_R: usize = 4;
const PREV_COL_G: usize = 5;
const PREV_COL_B: usize = 6;
const PREV_COL_A: usize = 7;
const PREV_COL_RECENTNESS: usize = 8;
const SMUDGE_BUCKET_SIZE: usize = 9;

struct Offsets {
    x: f32,
    y: f32,
}

/// Macro-like helper: setting base value (对应 C 的 BASEVAL 宏)。
#[inline]
fn baseval(brush: &Brush, id: BrushSetting) -> f32 {
    brush.settings[id as usize].base_value()
}

/// Macro-like helper: dynamic setting value (对应 C 的 SETTING 宏，从 settings_value[] 读取)。
#[inline]
fn setting(brush: &Brush, id: BrushSetting) -> f32 {
    brush.settings_value[id as usize]
}

impl Brush {
    // =========================================================================
    // directional_offsets — mypaint-brush.c:586-664
    // 全部使用 SETTING（settings_value），不是 BASEVAL
    // =========================================================================

    fn directional_offsets(&self, base_radius: f32, brush_flip: i32) -> Offsets {
        let offset_mult = setting(self, BrushSetting::OffsetMultiplier).exp();
        if !offset_mult.is_finite() {
            return Offsets { x: 0.0, y: 0.0 };
        }

        let mut dx = setting(self, BrushSetting::OffsetX);
        let mut dy = setting(self, BrushSetting::OffsetY);

        let offset_angle_adj = setting(self, BrushSetting::OffsetAngleAdj);
        let dir_angle_dy = self.state.direction_angle_dy;
        let dir_angle_dx = self.state.direction_angle_dx;
        let angle_deg = ((dir_angle_dy.atan2(dir_angle_dx)).to_degrees() - 90.0) % 360.0;

        // offset to one side of direction
        let offset_angle = setting(self, BrushSetting::OffsetAngle);
        if offset_angle != 0.0 {
            let dir_angle = (angle_deg + offset_angle_adj).to_radians();
            dx += dir_angle.cos() * offset_angle;
            dy += dir_angle.sin() * offset_angle;
        }

        // offset to one side of ascension angle
        let view_rotation = self.state.viewrotation;
        let offset_angle_asc = setting(self, BrushSetting::OffsetAngleAsc);
        if offset_angle_asc != 0.0 {
            let ascension = self.state.ascension;
            let asc_angle = (ascension - view_rotation + offset_angle_adj).to_radians();
            dx += asc_angle.cos() * offset_angle_asc;
            dy += asc_angle.sin() * offset_angle_asc;
        }

        // offset to one side of view orientation
        let view_offset = setting(self, BrushSetting::OffsetAngleView);
        if view_offset != 0.0 {
            let view_angle = (view_rotation + offset_angle_adj).to_radians();
            dx += (-view_angle).cos() * view_offset;
            dy += (-view_angle).sin() * view_offset;
        }

        // offset mirrored to sides of direction
        let offset_dir_mirror = 0.0f32.max(setting(self, BrushSetting::OffsetAngle2));
        if offset_dir_mirror != 0.0 {
            let dir_mirror_angle = (angle_deg + offset_angle_adj * brush_flip as f32).to_radians();
            let offset_factor = offset_dir_mirror * brush_flip as f32;
            dx += dir_mirror_angle.cos() * offset_factor;
            dy += dir_mirror_angle.sin() * offset_factor;
        }

        // offset mirrored to sides of ascension angle
        let offset_asc_mirror = 0.0f32.max(setting(self, BrushSetting::OffsetAngle2Asc));
        if offset_asc_mirror != 0.0 {
            let ascension = self.state.ascension;
            let asc_angle =
                (ascension - view_rotation + offset_angle_adj * brush_flip as f32).to_radians();
            let offset_factor = brush_flip as f32 * offset_asc_mirror;
            dx += asc_angle.cos() * offset_factor;
            dy += asc_angle.sin() * offset_factor;
        }

        // offset mirrored to sides of view orientation
        let offset_view_mirror = 0.0f32.max(setting(self, BrushSetting::OffsetAngle2View));
        if offset_view_mirror != 0.0 {
            let offset_factor = brush_flip as f32 * offset_view_mirror;
            let offset_angle_rad = (view_rotation + offset_angle_adj).to_radians();
            dx += (-offset_angle_rad).cos() * offset_factor;
            dy += (-offset_angle_rad).sin() * offset_factor;
        }

        let lim = 3240.0;
        let base_mul = base_radius * offset_mult;
        Offsets {
            x: (dx * base_mul).clamp(-lim, lim),
            y: (dy * base_mul).clamp(-lim, lim),
        }
    }

    // =========================================================================
    // update_states_and_setting_values — mypaint-brush.c:708-904
    // =========================================================================

    fn update_states(
        &mut self,
        step_ddab: f32,
        step_dx: f32,
        step_dy: f32,
        step_dpressure: f32,
        step_declination: f32,
        step_ascension: f32,
        step_dtime: f32,
        step_viewzoom: f32,
        step_viewrotation: f32,
        step_declinationx: f32,
        step_declinationy: f32,
        step_barrel_rotation: f32,
    ) {
        let mut step_dtime = step_dtime;
        if step_dtime < 0.0 {
            eprintln!("Time is running backwards!");
            step_dtime = 0.001;
        } else if step_dtime == 0.0 {
            step_dtime = 0.001;
        }

        self.state.x += step_dx;
        self.state.y += step_dy;
        self.state.pressure += step_dpressure;
        self.state.declination += step_declination;
        self.state.ascension += step_ascension;
        self.state.declinationx += step_declinationx;
        self.state.declinationy += step_declinationy;

        self.state.viewzoom = step_viewzoom;
        let viewrotation = mod_arith(step_viewrotation.to_degrees() + 180.0, 360.0) - 180.0;
        self.state.viewrotation = viewrotation;

        // Gridmap state update — 使用 SETTING (settings_value 来自上一步 update_states)
        // 对应 mypaint-brush.c:732-746
        {
            let x = self.state.actual_x;
            let y = self.state.actual_y;
            let scale = setting(self, BrushSetting::GridmapScale).exp();
            let scale_x = setting(self, BrushSetting::GridmapScaleX);
            let scale_y = setting(self, BrushSetting::GridmapScaleY);
            let scaled_size = scale * GRID_SIZE;
            self.state.gridmap_x =
                mod_arith((x * scale_x).abs(), scaled_size) / scaled_size * GRID_SIZE;
            self.state.gridmap_y =
                mod_arith((y * scale_y).abs(), scaled_size) / scaled_size * GRID_SIZE;
            if x < 0.0 {
                self.state.gridmap_x = GRID_SIZE - self.state.gridmap_x;
            }
            if y < 0.0 {
                self.state.gridmap_y = GRID_SIZE - self.state.gridmap_y;
            }
        }

        let base_radius = baseval(self, BrushSetting::RadiusLogarithmic).exp();
        self.state.barrel_rotation += step_barrel_rotation;

        if self.state.pressure <= 0.0 {
            self.state.pressure = 0.0;
        }
        let pressure = self.state.pressure;

        // start / end stroke
        {
            let lim = 0.0001;
            let threshold = baseval(self, BrushSetting::StrokeThreshold);
            let started = self.state.stroke_started;
            if started == 0.0 && pressure > threshold + lim {
                self.state.stroke_started = 1.0;
                self.state.stroke = 0.0;
            } else if started != 0.0 && pressure <= threshold * 0.9 + lim {
                self.state.stroke_started = 0.0;
            }
        }

        // speed calculation
        let norm_dx = step_dx / step_dtime * self.state.viewzoom;
        let norm_dy = step_dy / step_dtime * self.state.viewzoom;
        let norm_speed = (norm_dx * norm_dx + norm_dy * norm_dy).sqrt();
        let norm_dist = ((step_dx / step_dtime / base_radius).powi(2)
            + (step_dy / step_dtime / base_radius).powi(2))
        .sqrt()
            * step_dtime;

        let mut inputs = [0.0; NUM_INPUTS];

        // Helper macros converted to inline closures
        let set_input = |inputs: &mut [f32; NUM_INPUTS], id: BrushInput, val: f32| {
            inputs[id as usize] = val;
        };

        inputs[BrushInput::Pressure as usize] =
            pressure * baseval(self, BrushSetting::PressureGainLog).exp();

        let m0 = self.speed_mapping_m[0];
        let q0 = self.speed_mapping_q[0];
        let m1 = self.speed_mapping_m[1];
        let q1 = self.speed_mapping_q[1];
        inputs[BrushInput::Speed1 as usize] =
            (self.speed_mapping_gamma[0] + self.state.norm_speed1_slow).ln() * m0 + q0;
        inputs[BrushInput::Speed2 as usize] =
            (self.speed_mapping_gamma[1] + self.state.norm_speed2_slow).ln() * m1 + q1;

        inputs[BrushInput::Random as usize] = self.random_input as f32;
        inputs[BrushInput::Stroke as usize] = self.state.stroke.min(1.0);

        // correct direction for varying view rotation
        let dir_angle = self.state.direction_dy.atan2(self.state.direction_dx);
        inputs[BrushInput::Direction as usize] =
            mod_arith(dir_angle.to_degrees() + viewrotation + 180.0, 180.0);
        let dir_angle_360 = self
            .state
            .direction_angle_dy
            .atan2(self.state.direction_angle_dx);
        inputs[BrushInput::DirectionAngle as usize] =
            (dir_angle_360.to_degrees() + viewrotation + 360.0) % 360.0;
        inputs[BrushInput::TiltDeclination as usize] = self.state.declination;
        inputs[BrushInput::TiltAscension as usize] =
            mod_arith(self.state.ascension + viewrotation + 180.0, 360.0) - 180.0;
        inputs[BrushInput::Viewzoom as usize] = baseval(self, BrushSetting::RadiusLogarithmic)
            - (base_radius / self.state.viewzoom).ln();
        inputs[BrushInput::AttackAngle as usize] = smallest_angular_difference(
            self.state.ascension,
            mod_arith(dir_angle_360.to_degrees() + 90.0, 360.0),
        );
        inputs[BrushInput::BrushRadius as usize] = baseval(self, BrushSetting::RadiusLogarithmic);

        inputs[BrushInput::GridmapX as usize] = self.state.gridmap_x.clamp(0.0, GRID_SIZE);
        inputs[BrushInput::GridmapY as usize] = self.state.gridmap_y.clamp(0.0, GRID_SIZE);

        inputs[BrushInput::TiltDeclinationx as usize] = self.state.declinationx;
        inputs[BrushInput::TiltDeclinationy as usize] = self.state.declinationy;

        inputs[BrushInput::Custom as usize] = self.state.custom_input;
        inputs[BrushInput::BarrelRotation as usize] = mod_arith(self.state.barrel_rotation, 360.0);

        // Calculate all setting values from mappings
        // 对应 mypaint-brush.c:818-820 — 遍历全部 SETTINGS，不是 INPUTS！
        for i in 0..crate::NUM_SETTINGS {
            self.settings_value[i] = self.settings[i].mapping().calculate(&inputs);
        }

        self.state.dabs_per_basic_radius =
            self.settings_value[BrushSetting::DabsPerBasicRadius as usize];
        self.state.dabs_per_actual_radius =
            self.settings_value[BrushSetting::DabsPerActualRadius as usize];
        self.state.dabs_per_second = self.settings_value[BrushSetting::DabsPerSecond as usize];

        // slow position tracking per dab
        {
            let fac = 1.0
                - Self::exp_decay(
                    self.settings_value[BrushSetting::SlowTrackingPerDab as usize],
                    step_ddab,
                );
            self.state.actual_x += (self.state.x - self.state.actual_x) * fac;
            self.state.actual_y += (self.state.y - self.state.actual_y) * fac;
        }

        // slow speed
        {
            let fac1 = 1.0
                - Self::exp_decay(
                    self.settings_value[BrushSetting::Speed1Slowness as usize],
                    step_dtime,
                );
            self.state.norm_speed1_slow += (norm_speed - self.state.norm_speed1_slow) * fac1;
            let fac2 = 1.0
                - Self::exp_decay(
                    self.settings_value[BrushSetting::Speed2Slowness as usize],
                    step_dtime,
                );
            self.state.norm_speed2_slow += (norm_speed - self.state.norm_speed2_slow) * fac2;
        }

        // slow speed vector
        {
            let mut time_constant =
                (self.settings_value[BrushSetting::OffsetBySpeedSlowness as usize] * 0.01).exp()
                    - 1.0;
            if time_constant < 0.002 {
                time_constant = 0.002;
            }
            let fac = 1.0 - Self::exp_decay(time_constant, step_dtime);
            self.state.norm_dx_slow += (norm_dx - self.state.norm_dx_slow) * fac;
            self.state.norm_dy_slow += (norm_dy - self.state.norm_dy_slow) * fac;
        }

        // orientation
        {
            let mut dx = step_dx * self.state.viewzoom;
            let mut dy = step_dy * self.state.viewzoom;
            let step_in_dabtime = (dx * dx + dy * dy).sqrt();
            let fac = 1.0
                - Self::exp_decay(
                    (self.settings_value[BrushSetting::DirectionFilter as usize] * 0.5).exp() - 1.0,
                    step_in_dabtime,
                );

            let dx_old = self.state.direction_dx;
            let dy_old = self.state.direction_dy;

            // 360 Direction
            self.state.direction_angle_dx += (dx - self.state.direction_angle_dx) * fac;
            self.state.direction_angle_dy += (dy - self.state.direction_angle_dy) * fac;

            // use the opposite speed vector if it is closer
            if (dx_old - dx).powi(2) + (dy_old - dy).powi(2)
                > (dx_old - (-dx)).powi(2) + (dy_old - (-dy)).powi(2)
            {
                dx = -dx;
                dy = -dy;
            }
            self.state.direction_dx += (dx - self.state.direction_dx) * fac;
            self.state.direction_dy += (dy - self.state.direction_dy) * fac;
        }

        // custom input
        {
            let fac = 1.0
                - Self::exp_decay(
                    self.settings_value[BrushSetting::CustomInputSlowness as usize],
                    0.1,
                );
            self.state.custom_input += (self.settings_value[BrushSetting::CustomInput as usize]
                - self.state.custom_input)
                * fac;
        }

        // stroke length
        {
            let frequency =
                (-self.settings_value[BrushSetting::StrokeDurationLogarithmic as usize]).exp();
            let stroke = 0.0f32.max(self.state.stroke + norm_dist * frequency);
            let wrap = 1.0 + self.settings_value[BrushSetting::StrokeHoldtime as usize].max(0.0);
            if stroke >= wrap && wrap > 10.9 {
                self.state.stroke = 1.0;
            } else if stroke >= wrap {
                self.state.stroke = stroke % wrap;
            } else {
                self.state.stroke = stroke;
            }
        }

        // calculate final radius
        let radius_log = self.settings_value[BrushSetting::RadiusLogarithmic as usize];
        self.state.actual_radius = radius_log.exp();
        if self.state.actual_radius < ACTUAL_RADIUS_MIN {
            self.state.actual_radius = ACTUAL_RADIUS_MIN;
        }
        if self.state.actual_radius > ACTUAL_RADIUS_MAX {
            self.state.actual_radius = ACTUAL_RADIUS_MAX;
        }

        // aspect ratio
        self.state.actual_elliptical_dab_ratio =
            self.settings_value[BrushSetting::EllipticalDabRatio as usize];
        self.state.actual_elliptical_dab_angle = mod_arith(
            self.settings_value[BrushSetting::EllipticalDabAngle as usize] - viewrotation + 180.0,
            180.0,
        ) - 180.0;
    }

    // =========================================================================
    // fetch_smudge_bucket — mypaint-brush.c:906-918
    // 无桶配置时回退到 inline_bucket（等价于 C 的 STATE(SMUDGE_RA) 默认行为）
    // =========================================================================

    fn fetch_smudge_bucket_mut(&mut self) -> &mut [f32; SMUDGE_BUCKET_SIZE] {
        let has_buckets = self.smudge_buckets.as_ref().is_some_and(|b| !b.is_empty());
        if !has_buckets {
            return &mut self.inline_bucket;
        }
        let buckets_len = self.smudge_buckets.as_ref().unwrap().len();
        let bucket_index = self.settings_value[BrushSetting::SmudgeBucket as usize]
            .round()
            .clamp(0.0, buckets_len as f32 - 1.0) as usize;
        // min/max 跟踪，对应 mypaint-brush.c:911-916
        let bi = bucket_index as i32;
        if self.min_bucket_used == -1 || self.min_bucket_used > bi {
            self.min_bucket_used = bi;
        }
        if self.max_bucket_used < bi {
            self.max_bucket_used = bi;
        }
        &mut self.smudge_buckets.as_mut().unwrap()[bucket_index]
    }

    fn fetch_smudge_bucket_ref(&self) -> &[f32; SMUDGE_BUCKET_SIZE] {
        let has_buckets = self.smudge_buckets.as_ref().is_some_and(|b| !b.is_empty());
        if !has_buckets {
            return &self.inline_bucket;
        }
        let buckets = self.smudge_buckets.as_ref().unwrap();
        let bucket_index = self.settings_value[BrushSetting::SmudgeBucket as usize]
            .round()
            .clamp(0.0, buckets.len() as f32 - 1.0) as usize;
        &buckets[bucket_index]
    }

    // =========================================================================
    // exp_decay helper — mypaint-brush.c:534-544
    // =========================================================================

    #[inline]
    fn exp_decay(t_const: f32, t: f32) -> f32 {
        if t_const <= 0.001 {
            return 0.0;
        }
        (-t / t_const).exp()
    }

    // =========================================================================
    // update_smudge_color — mypaint-brush.c:920-997
    // =========================================================================

    // (Implemented as free functions below to avoid borrow conflicts)

    // =========================================================================
    // apply_smudge — mypaint-brush.c:999-1035
    // =========================================================================

    // (Implemented as free functions below)

    // =========================================================================
    // prepare_and_draw_dab — mypaint-brush.c:1042-1250
    // =========================================================================

    fn prepare_and_draw_dab(&mut self, surface: &mut dyn Surface, linear: bool) -> bool {
        let opaque_fac = self.settings_value[BrushSetting::OpaqueMultiply as usize];
        let mut opaque = 0.0f32.max(self.settings_value[BrushSetting::Opaque as usize]);
        opaque = (opaque * opaque_fac).clamp(0.0, 1.0);

        let opaque_linearize = baseval(self, BrushSetting::OpaqueLinearize);
        if opaque_linearize != 0.0 {
            let dabs_per_pixel =
                (self.state.dabs_per_actual_radius + self.state.dabs_per_basic_radius) * 2.0;
            let dabs_per_pixel = 1.0f32.max(dabs_per_pixel);
            let dabs_per_pixel = 1.0 + opaque_linearize * (dabs_per_pixel - 1.0);

            let alpha = opaque;
            let beta = 1.0 - alpha;
            let beta_dab = beta.powf(1.0 / dabs_per_pixel);
            let alpha_dab = 1.0 - beta_dab;
            opaque = alpha_dab;
        }

        let mut x = self.state.actual_x;
        let mut y = self.state.actual_y;

        let base_radius = baseval(self, BrushSetting::RadiusLogarithmic).exp();

        // Directional offsets
        let offs = self.directional_offsets(base_radius, self.state.flip as i32);
        x += offs.x;
        y += offs.y;

        let view_zoom = self.state.viewzoom;
        let offset_by_speed = self.settings_value[BrushSetting::OffsetBySpeed as usize];
        if offset_by_speed != 0.0 {
            x += self.state.norm_dx_slow * offset_by_speed * 0.1 / view_zoom;
            y += self.state.norm_dy_slow * offset_by_speed * 0.1 / view_zoom;
        }

        let offset_by_random = self.settings_value[BrushSetting::OffsetByRandom as usize];
        if offset_by_random != 0.0 {
            let amp = 0.0f32.max(offset_by_random);
            x += rand_gauss(&mut self.rng) * amp * base_radius;
            y += rand_gauss(&mut self.rng) * amp * base_radius;
        }

        let mut radius = self.state.actual_radius;
        let radius_by_random = self.settings_value[BrushSetting::RadiusByRandom as usize];
        if radius_by_random != 0.0 {
            let noise = rand_gauss(&mut self.rng) * radius_by_random;
            let radius_log = self.settings_value[BrushSetting::RadiusLogarithmic as usize] + noise;
            radius = radius_log.exp().clamp(ACTUAL_RADIUS_MIN, ACTUAL_RADIUS_MAX);
            let alpha_correction = (self.state.actual_radius / radius).powi(2);
            if alpha_correction <= 1.0 {
                opaque *= alpha_correction;
            }
        }

        let paint_factor = self.settings_value[BrushSetting::PaintMode as usize];
        let paint_setting_constant = self.settings[BrushSetting::PaintMode as usize]
            .mapping()
            .is_constant();
        let legacy_smudge = paint_factor <= 0.0 && paint_setting_constant;

        // color part — convert HSV to RGB
        let mut color_h = baseval(self, BrushSetting::ColorH);
        let mut color_s = baseval(self, BrushSetting::ColorS);
        let mut color_v = baseval(self, BrushSetting::ColorV);
        hsv_to_rgb(&mut color_h, &mut color_s, &mut color_v);

        // update smudge color
        let smudge_length = self.settings_value[BrushSetting::SmudgeLength as usize];
        if smudge_length < 1.0
            && (self.settings_value[BrushSetting::Smudge as usize] != 0.0
                || !self.settings[BrushSetting::Smudge as usize]
                    .mapping()
                    .is_constant())
        {
            let smudge_length_log = self.settings_value[BrushSetting::SmudgeLengthLog as usize];
            let smudge_radius_log = self.settings_value[BrushSetting::SmudgeRadiusLog as usize];
            let smudge_op_lim = self.settings_value[BrushSetting::SmudgeTransparency as usize];
            let return_early = {
                let bucket = self.fetch_smudge_bucket_mut();
                update_smudge_color_fn(
                    surface,
                    bucket,
                    smudge_length,
                    smudge_length_log,
                    smudge_radius_log,
                    smudge_op_lim,
                    x.round() as i32,
                    y.round() as i32,
                    radius,
                    legacy_smudge,
                    paint_factor,
                )
            };
            if return_early {
                return false;
            }
        }

        let mut eraser_target_alpha = 1.0;
        let smudge_value = self.settings_value[BrushSetting::Smudge as usize];
        if smudge_value > 0.0 {
            let bucket_copy = *self.fetch_smudge_bucket_ref();
            eraser_target_alpha = apply_smudge_fn(
                &bucket_copy,
                smudge_value,
                legacy_smudge,
                paint_factor,
                &mut color_h,
                &mut color_s,
                &mut color_v,
            );
        }

        // eraser
        let eraser = self.settings_value[BrushSetting::Eraser as usize];
        if eraser != 0.0 {
            eraser_target_alpha *= 1.0 - eraser;
        }

        // HSV/HSL color dynamics
        let using_hsv_dynamics = self.settings_value[BrushSetting::ChangeColorH as usize] != 0.0
            || self.settings_value[BrushSetting::ChangeColorHsvS as usize] != 0.0
            || self.settings_value[BrushSetting::ChangeColorV as usize] != 0.0;
        let using_hsl_dynamics = self.settings_value[BrushSetting::ChangeColorL as usize] != 0.0
            || self.settings_value[BrushSetting::ChangeColorHslS as usize] != 0.0;
        let using_color_dynamics = using_hsv_dynamics || using_hsl_dynamics;

        // delinearize
        if linear && using_color_dynamics {
            color_h = color_h.powf(1.0 / 2.2);
            color_s = color_s.powf(1.0 / 2.2);
            color_v = color_v.powf(1.0 / 2.2);
        }

        // HSV color change
        if using_hsv_dynamics {
            rgb_to_hsv(&mut color_h, &mut color_s, &mut color_v);
            color_h += self.settings_value[BrushSetting::ChangeColorH as usize];
            color_s +=
                color_s * color_v * self.settings_value[BrushSetting::ChangeColorHsvS as usize];
            color_v += self.settings_value[BrushSetting::ChangeColorV as usize];
            hsv_to_rgb(&mut color_h, &mut color_s, &mut color_v);
        }

        // HSL color change
        if using_hsl_dynamics {
            rgb_to_hsl(&mut color_h, &mut color_s, &mut color_v);
            color_v += self.settings_value[BrushSetting::ChangeColorL as usize];
            color_s += color_s
                * (1.0 - color_v).abs().min(color_v.abs())
                * 2.0
                * self.settings_value[BrushSetting::ChangeColorHslS as usize];
            hsl_to_rgb(&mut color_h, &mut color_s, &mut color_v);
        }

        // linearize
        if linear && using_color_dynamics {
            color_h = color_h.powf(2.2);
            color_s = color_s.powf(2.2);
            color_v = color_v.powf(2.2);
        }

        let mut hardness = self.settings_value[BrushSetting::Hardness as usize].clamp(0.0, 1.0);
        let softness = self.settings_value[BrushSetting::Softness as usize].clamp(0.0, 1.0);

        // anti-aliasing
        let current_fadeout_in_pixels = radius * (1.0 - hardness);
        let min_fadeout_in_pixels = self.settings_value[BrushSetting::AntiAliasing as usize];
        if current_fadeout_in_pixels < min_fadeout_in_pixels {
            let current_optical_radius = radius - (1.0 - hardness) * radius / 2.0;
            let hardness_new = (current_optical_radius - (min_fadeout_in_pixels / 2.0))
                / (current_optical_radius + (min_fadeout_in_pixels / 2.0));
            let radius_new = min_fadeout_in_pixels / (1.0 - hardness_new);
            hardness = hardness_new;
            radius = radius_new;
        }

        // snap to pixel
        let snap_to_pixel = self.settings_value[BrushSetting::SnapToPixel as usize];
        if snap_to_pixel > 0.0 {
            let snapped_x = x.floor() + 0.5;
            let snapped_y = y.floor() + 0.5;
            x = x + (snapped_x - x) * snap_to_pixel;
            y = y + (snapped_y - y) * snap_to_pixel;

            let mut snapped_radius = (radius * 2.0).round() / 2.0;
            if snapped_radius < 0.5 {
                snapped_radius = 0.5;
            }
            if snap_to_pixel > 0.9999 {
                snapped_radius -= 0.0001;
            }
            radius = radius + (snapped_radius - radius) * snap_to_pixel;
        }

        let dab_ratio = self.state.actual_elliptical_dab_ratio;
        let dab_angle = self.state.actual_elliptical_dab_angle;
        let lock_alpha = self.settings_value[BrushSetting::LockAlpha as usize];
        let colorize = self.settings_value[BrushSetting::Colorize as usize];
        let posterize = self.settings_value[BrushSetting::Posterize as usize];
        let posterize_num = self.settings_value[BrushSetting::PosterizeNum as usize];

        surface.draw_dab(&DabParams {
            x,
            y,
            radius,
            color_r: color_h,
            color_g: color_s,
            color_b: color_v,
            opaque,
            hardness,
            softness,
            alpha_eraser: eraser_target_alpha,
            aspect_ratio: dab_ratio,
            angle: dab_angle,
            lock_alpha,
            colorize,
            posterize,
            posterize_num,
            paint: paint_factor,
        })
    }

    // =========================================================================
    // count_dabs_to — mypaint-brush.c:1253-1287
    // =========================================================================

    fn count_dabs(&mut self, x: f32, y: f32, dtime: f32) -> f32 {
        let base_radius_log = baseval(self, BrushSetting::RadiusLogarithmic);
        let base_radius = base_radius_log
            .exp()
            .clamp(ACTUAL_RADIUS_MIN, ACTUAL_RADIUS_MAX);

        if self.state.actual_radius == 0.0 {
            self.state.actual_radius = base_radius;
        }

        let dx = x - self.state.x;
        let dy = y - self.state.y;

        let dist = if self.state.actual_elliptical_dab_ratio > 1.0 {
            let angle_rad = self.state.actual_elliptical_dab_angle.to_radians();
            let cs = angle_rad.cos();
            let sn = angle_rad.sin();
            let yyr = (dy * cs - dx * sn) * self.state.actual_elliptical_dab_ratio;
            let xxr = dy * sn + dx * cs;
            (yyr * yyr + xxr * xxr).sqrt()
        } else {
            (dx * dx + dy * dy).sqrt()
        };

        let res1 = dist / self.state.actual_radius * self.state.dabs_per_actual_radius;
        let res2 = dist / base_radius * self.state.dabs_per_basic_radius;
        let res3 = dtime * self.state.dabs_per_second;
        let res4 = res1 + res2 + res3;
        if res4.is_nan() || res4 < 0.0 {
            0.0
        } else {
            res4
        }
    }

    // =========================================================================
    // mypaint_brush_stroke_to — mypaint-brush.c:1300-1547
    // =========================================================================

    /// Main stroke entry point.
    /// Corresponds to `mypaint_brush_stroke_to` (L1300-1547).
    /// Returns true if the stroke is finished or should be split.
    pub fn stroke_to(
        &mut self,
        surface: &mut dyn Surface,
        x: f32,
        y: f32,
        pressure: f32,
        xtilt: f32,
        ytilt: f32,
        dtime: f64,
        viewzoom: f32,
        viewrotation: f32,
        barrel_rotation: f32,
        linear: bool,
    ) -> bool {
        let max_dtime = 5.0;
        let mut dtime = dtime;

        let mut tilt_ascension = 0.0;
        let mut tilt_declination = 90.0;
        let mut tilt_declinationx = 90.0;
        let mut tilt_declinationy = 90.0;
        if xtilt != 0.0 || ytilt != 0.0 {
            let xtilt = xtilt.clamp(-1.0, 1.0);
            let ytilt = ytilt.clamp(-1.0, 1.0);
            assert!(xtilt.is_finite() && ytilt.is_finite());
            tilt_ascension = (-xtilt).atan2(ytilt).to_degrees();
            let rad = (xtilt * xtilt + ytilt * ytilt).sqrt();
            tilt_declination = 90.0 - (rad * 60.0);
            tilt_declinationx = xtilt * 60.0;
            tilt_declinationy = ytilt * 60.0;
            assert!(tilt_ascension.is_finite());
            assert!(tilt_declination.is_finite());
            assert!(tilt_declinationx.is_finite());
            assert!(tilt_declinationy.is_finite());
        }

        let pressure = if pressure <= 0.0 { 0.0 } else { pressure };
        if !x.is_finite() || !y.is_finite() || x > 1e10 || y > 1e10 || x < -1e10 || y < -1e10 {
            eprintln!("Warning: ignoring brush::stroke_to with insane inputs (x = {x}, y = {y})");
            // Reset to safe values
            return true;
        }
        assert!(x < 1e8 && y < 1e8 && x > -1e8 && y > -1e8);

        if dtime < 0.0 {
            eprintln!("Time jumped backwards by dtime={dtime} seconds!");
        }
        if dtime <= 0.0 {
            dtime = 0.0001;
        }

        // Workaround for tablets that don't report motion without pressure
        if dtime > 0.1 && pressure != 0.0 && self.state.pressure == 0.0 {
            self.stroke_to(
                surface,
                x,
                y,
                0.0,
                90.0,
                0.0,
                dtime - 0.0001,
                viewzoom,
                viewrotation,
                0.0,
                linear,
            );
            dtime = 0.0001;
        }

        // skip some length of input (for stable tracking noise)
        if self.skip > 0.001 {
            let dist = ((self.skip_last_x - x).powi(2) + (self.skip_last_y - y).powi(2)).sqrt();
            self.skip_last_x = x;
            self.skip_last_y = y;
            self.skipped_dtime += dtime as f32;
            self.skip -= dist;
            dtime = self.skipped_dtime as f64;

            if self.skip > 0.001 && !(dtime > max_dtime as f64 || self.reset_requested) {
                return false;
            }
            self.skip = 0.0;
            self.skip_last_x = 0.0;
            self.skip_last_y = 0.0;
            self.skipped_dtime = 0.0;
        }

        // Calculate the actual "virtual" cursor position.
        // 对应 mypaint-brush.c:1372-1396 — noise 后 slow_tracking。
        // 仅修改本地变量，state.x/state.y 由 update_states 经 step_dx 累加更新。
        let mut effective_x = x;
        let mut effective_y = y;

        // tracking noise
        if baseval(self, BrushSetting::TrackingNoise) != 0.0 {
            let base_radius = baseval(self, BrushSetting::RadiusLogarithmic).exp();
            let noise = base_radius * baseval(self, BrushSetting::TrackingNoise);
            if noise > 0.001 {
                self.skip = 0.5 * noise;
                self.skip_last_x = x;
                self.skip_last_y = y;
                effective_x += rand_gauss(&mut self.rng) * noise;
                effective_y += rand_gauss(&mut self.rng) * noise;
            }
        }

        // slow_tracking fac — 仅修改局部 filtered_x/y，不写 state.x/state.y
        let fac = 1.0
            - Self::exp_decay(
                baseval(self, BrushSetting::SlowTracking),
                100.0 * dtime as f32,
            );
        let input_x = self.state.x + (effective_x - self.state.x) * fac;
        let input_y = self.state.y + (effective_y - self.state.y) * fac;

        // reset or time jump
        if dtime > (max_dtime as f64) || self.reset_requested {
            self.reset_requested = false;
            self.brush_reset();
            self.random_input = self.rng.next();
            self.state.x = input_x;
            self.state.y = input_y;
            self.state.pressure = pressure;
            self.state.actual_x = self.state.x;
            self.state.actual_y = self.state.y;
            self.state.stroke = 1.0; // start as if stroke was long finished
            return true;
        }

        // draw many dabs
        let mut painted: Option<bool> = None;
        let mut dtime_left = dtime as f32;

        // 持久化 step_dpressure（stroke-split 检查需要最后一次的值，
        // 对应 C 的 `step_dpressure` 在循环外声明）
        let mut last_step_dpressure: f32 = 0.0;

        let mut dabs_moved = self.state.partial_dabs;
        let mut dabs_todo = self.count_dabs(input_x, input_y, dtime as f32);

        while dabs_moved + dabs_todo >= 1.0 {
            let (
                step_ddab,
                step_dx,
                step_dy,
                step_dpressure,
                step_dtime,
                step_declination,
                step_ascension,
                step_declinationx,
                step_declinationy,
                step_barrel_rotation,
            ) = {
                if dabs_moved > 0.001 {
                    let step_ddab = 1.0 - dabs_moved;
                    dabs_moved = 0.0;
                    let frac = step_ddab / dabs_todo;
                    (
                        step_ddab,
                        frac * (input_x - self.state.x),
                        frac * (input_y - self.state.y),
                        frac * (pressure - self.state.pressure),
                        frac * dtime_left,
                        frac * (tilt_declination - self.state.declination),
                        frac * smallest_angular_difference(self.state.ascension, tilt_ascension),
                        frac * (tilt_declinationx - self.state.declinationx),
                        frac * (tilt_declinationy - self.state.declinationy),
                        frac * smallest_angular_difference(
                            self.state.barrel_rotation,
                            barrel_rotation * 360.0,
                        ),
                    )
                } else {
                    let frac = 1.0 / dabs_todo;
                    (
                        1.0,
                        frac * (input_x - self.state.x),
                        frac * (input_y - self.state.y),
                        frac * (pressure - self.state.pressure),
                        frac * dtime_left,
                        frac * (tilt_declination - self.state.declination),
                        frac * smallest_angular_difference(self.state.ascension, tilt_ascension),
                        frac * (tilt_declinationx - self.state.declinationx),
                        frac * (tilt_declinationy - self.state.declinationy),
                        frac * smallest_angular_difference(
                            self.state.barrel_rotation,
                            barrel_rotation * 360.0,
                        ),
                    )
                }
            };

            last_step_dpressure = step_dpressure;
            self.update_states(
                step_ddab,
                step_dx,
                step_dy,
                step_dpressure,
                step_declination,
                step_ascension,
                step_dtime,
                viewzoom,
                viewrotation,
                step_declinationx,
                step_declinationy,
                step_barrel_rotation,
            );

            // Flip between 1 and -1
            self.state.flip *= -1.0;

            let painted_now = self.prepare_and_draw_dab(surface, linear);
            if painted_now {
                painted = Some(true);
            } else if painted.is_none() {
                painted = Some(false);
            }

            self.random_input = self.rng.next();
            dtime_left -= step_dtime;
            dabs_todo = self.count_dabs(input_x, input_y, dtime_left);
        }

        // Move brush to current time (no more dab will happen)
        // 对应 mypaint-brush.c:1482，使用 dabs_todo (不含 dabs_moved)
        {
            let step_ddab = dabs_todo;
            let step_dx = input_x - self.state.x;
            let step_dy = input_y - self.state.y;
            let step_dpressure = pressure - self.state.pressure;
            let step_declination = tilt_declination - self.state.declination;
            let step_declinationx = tilt_declinationx - self.state.declinationx;
            let step_declinationy = tilt_declinationy - self.state.declinationy;
            let step_ascension = smallest_angular_difference(self.state.ascension, tilt_ascension);
            let step_dtime = dtime_left;
            let step_barrel_rotation =
                smallest_angular_difference(self.state.barrel_rotation, barrel_rotation * 360.0);

            self.update_states(
                step_ddab,
                step_dx,
                step_dy,
                step_dpressure,
                step_declination,
                step_ascension,
                step_dtime,
                viewzoom,
                viewrotation,
                step_declinationx,
                step_declinationy,
                step_barrel_rotation,
            );
        }

        // save the fraction of a dab that is already done
        self.state.partial_dabs = dabs_moved + dabs_todo;

        // stroke separation logic
        let painted = match painted {
            None => {
                if self.stroke_current_idling_time > 0.0 || self.stroke_total_painting_time == 0.0 {
                    Some(false)
                } else {
                    Some(true)
                }
            }
            some => some,
        };

        if let Some(true) = painted {
            self.stroke_total_painting_time += dtime;
            self.stroke_current_idling_time = 0.0;
            if self.stroke_total_painting_time > 4.0 + 3.0 * pressure as f64 {
                // Only split if pressure isn't being released.
                // 对应 mypaint-brush.c:1525：`if (step_dpressure >= 0)` —
                // 使用循环中最后一次保存的 step_dpressure。
                if last_step_dpressure >= 0.0 {
                    return true;
                }
            }
        } else if let Some(false) = painted {
            self.stroke_current_idling_time += dtime;
            if self.stroke_total_painting_time == 0.0 {
                if self.stroke_current_idling_time > 1.0 {
                    return true;
                }
            } else {
                if self.stroke_total_painting_time + self.stroke_current_idling_time
                    > 0.9 + 5.0 * pressure as f64
                {
                    return true;
                }
            }
        }
        false
    }
}

// =========================================================================
// Free functions for smudge (to avoid borrow checker conflicts)
// =========================================================================

/// update_smudge_color — mypaint-brush.c:920-997.
fn update_smudge_color_fn(
    surface: &mut dyn Surface,
    bucket: &mut [f32; SMUDGE_BUCKET_SIZE],
    smudge_length: f32,
    smudge_length_log: f32,
    smudge_radius_log: f32,
    smudge_op_lim: f32,
    px: i32,
    py: i32,
    radius: f32,
    legacy_smudge: bool,
    paint_factor: f32,
) -> bool {
    // 对应 mypaint-brush.c:927-995。
    // update_factor 可能在 recentness==0 时被改为 0（首次初始化：直接用采样色）。
    let mut update_factor = 0.01f32.max(smudge_length);

    let recentness = bucket[PREV_COL_RECENTNESS] * update_factor;
    bucket[PREV_COL_RECENTNESS] = recentness;

    let margin = 0.0000000000000001;
    if recentness < 1.0f32.min((0.5 * update_factor).powf(smudge_length_log) + margin) {
        if recentness == 0.0 {
            // First initialization — sampled color used directly, no blend.
            // 对应 mypaint-brush.c:942-945
            update_factor = 0.0;
        }
        bucket[PREV_COL_RECENTNESS] = 1.0;

        let smudge_radius =
            (radius * smudge_radius_log.exp()).clamp(ACTUAL_RADIUS_MIN, ACTUAL_RADIUS_MAX);

        let (r, g, b, a) = surface.get_color(
            px as f32,
            py as f32,
            smudge_radius,
            if legacy_smudge { -1.0 } else { paint_factor },
        );

        if (smudge_op_lim > 0.0 && a < smudge_op_lim) || (smudge_op_lim < 0.0 && a > -smudge_op_lim)
        {
            return true;
        }
        bucket[PREV_COL_R] = r;
        bucket[PREV_COL_G] = g;
        bucket[PREV_COL_B] = b;
        bucket[PREV_COL_A] = a;
    }

    if legacy_smudge {
        let fac_old = update_factor;
        let fac_new = (1.0 - update_factor) * bucket[PREV_COL_A];
        bucket[SMUDGE_R] = fac_old * bucket[SMUDGE_R] + fac_new * bucket[PREV_COL_R];
        bucket[SMUDGE_G] = fac_old * bucket[SMUDGE_G] + fac_new * bucket[PREV_COL_G];
        bucket[SMUDGE_B] = fac_old * bucket[SMUDGE_B] + fac_new * bucket[PREV_COL_B];
        bucket[SMUDGE_A] = (fac_old * bucket[SMUDGE_A] + fac_new).clamp(0.0, 1.0);
    } else if bucket[PREV_COL_A] > WGM_EPSILON * 10.0 {
        let prev_smudge_color = [
            bucket[SMUDGE_R],
            bucket[SMUDGE_G],
            bucket[SMUDGE_B],
            bucket[SMUDGE_A],
        ];
        let sampled_color = [
            bucket[PREV_COL_R],
            bucket[PREV_COL_G],
            bucket[PREV_COL_B],
            bucket[PREV_COL_A],
        ];
        let mixed = mix_colors(
            &prev_smudge_color,
            &sampled_color,
            update_factor,
            paint_factor,
        );
        bucket[SMUDGE_R] = mixed[0];
        bucket[SMUDGE_G] = mixed[1];
        bucket[SMUDGE_B] = mixed[2];
        bucket[SMUDGE_A] = mixed[3];
    } else {
        bucket[SMUDGE_A] = (bucket[SMUDGE_A] + bucket[PREV_COL_A]) / 2.0;
    }
    false
}

/// apply_smudge — mypaint-brush.c:999-1035.
#[inline]
fn apply_smudge_fn(
    bucket: &[f32; SMUDGE_BUCKET_SIZE],
    smudge_value: f32,
    legacy_smudge: bool,
    paint_factor: f32,
    color_r: &mut f32,
    color_g: &mut f32,
    color_b: &mut f32,
) -> f32 {
    let smudge_factor = 1.0f32.min(smudge_value);
    let eraser_target_alpha =
        (1.0 - smudge_factor + smudge_factor * bucket[SMUDGE_A]).clamp(0.0, 1.0);

    if eraser_target_alpha > 0.0 {
        if legacy_smudge {
            let col_factor = 1.0 - smudge_factor;
            *color_r =
                (smudge_factor * bucket[SMUDGE_R] + col_factor * *color_r) / eraser_target_alpha;
            *color_g =
                (smudge_factor * bucket[SMUDGE_G] + col_factor * *color_g) / eraser_target_alpha;
            *color_b =
                (smudge_factor * bucket[SMUDGE_B] + col_factor * *color_b) / eraser_target_alpha;
        } else {
            let smudge_color = [
                bucket[SMUDGE_R],
                bucket[SMUDGE_G],
                bucket[SMUDGE_B],
                bucket[SMUDGE_A],
            ];
            let brush_color = [*color_r, *color_g, *color_b, 1.0];
            let mixed = mix_colors(&smudge_color, &brush_color, smudge_factor, paint_factor);
            *color_r = mixed[0];
            *color_g = mixed[1];
            *color_b = mixed[2];
        }
    } else {
        *color_r = 1.0;
        *color_g = 0.0;
        *color_b = 0.0;
    }
    eraser_target_alpha
}
