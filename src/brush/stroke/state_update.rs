//! Per-step `Brush` state advance + setting interpolation.
//!
//! 对应 mypaint-brush.c:586-904：
//! - directional_offsets (L586-664)
//! - update_states_and_setting_values (L708-904)
//! - print_inputs (L667-699，调试输出)

use super::{baseval, setting, StrokeStep, ACTUAL_RADIUS_MAX, ACTUAL_RADIUS_MIN, GRID_SIZE};
use crate::brush::Brush;
use crate::util::helpers::{mod_arith, smallest_angular_difference};
use crate::{BrushInput, BrushSetting, NUM_INPUTS};

impl Brush {
    // =========================================================================
    // directional_offsets — mypaint-brush.c:586-664
    // 全部使用 SETTING（settings_value），不是 BASEVAL
    // =========================================================================

    pub(super) fn directional_offsets(&self, base_radius: f32, brush_flip: i32) -> (f32, f32) {
        let offset_mult = setting(self, BrushSetting::OffsetMultiplier).exp();
        if !offset_mult.is_finite() {
            return (0.0, 0.0);
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
        (
            (dx * base_mul).clamp(-lim, lim),
            (dy * base_mul).clamp(-lim, lim),
        )
    }

    // =========================================================================
    // update_states_and_setting_values — mypaint-brush.c:708-904
    // =========================================================================

    pub(super) fn update_states(&mut self, step: &StrokeStep, viewzoom: f32, viewrotation: f32) {
        // Destructure step into the local names used throughout this 200+ line
        // method body — keeps the C-translation transliteration legible.
        let StrokeStep {
            ddab: step_ddab,
            dx: step_dx,
            dy: step_dy,
            dpressure: step_dpressure,
            dtime: step_dtime_in,
            declination: step_declination,
            ascension: step_ascension,
            declinationx: step_declinationx,
            declinationy: step_declinationy,
            barrel_rotation: step_barrel_rotation,
        } = *step;
        let mut step_dtime = step_dtime_in;
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

        self.state.viewzoom = viewzoom;
        // Normalize and shadow `viewrotation` to a degrees-in-(-180, 180] form;
        // body below uses this normalized value, not the raw radians.
        let viewrotation = mod_arith(viewrotation.to_degrees() + 180.0, 360.0) - 180.0;
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
        let _set_input = |inputs: &mut [f32; NUM_INPUTS], id: BrushInput, val: f32| {
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

        // 对应 mypaint-brush.c:814-816 — 在 settings 计算前打印调试输出
        if self.print_inputs {
            print_brush_inputs(
                &inputs,
                self.state.viewrotation,
                self.state.actual_elliptical_dab_angle,
            );
        }

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
                - super::exp_decay(
                    self.settings_value[BrushSetting::SlowTrackingPerDab as usize],
                    step_ddab,
                );
            self.state.actual_x += (self.state.x - self.state.actual_x) * fac;
            self.state.actual_y += (self.state.y - self.state.actual_y) * fac;
        }

        // slow speed
        {
            let fac1 = 1.0
                - super::exp_decay(
                    self.settings_value[BrushSetting::Speed1Slowness as usize],
                    step_dtime,
                );
            self.state.norm_speed1_slow += (norm_speed - self.state.norm_speed1_slow) * fac1;
            let fac2 = 1.0
                - super::exp_decay(
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
            let fac = 1.0 - super::exp_decay(time_constant, step_dtime);
            self.state.norm_dx_slow += (norm_dx - self.state.norm_dx_slow) * fac;
            self.state.norm_dy_slow += (norm_dy - self.state.norm_dy_slow) * fac;
        }

        // orientation
        {
            let mut dx = step_dx * self.state.viewzoom;
            let mut dy = step_dy * self.state.viewzoom;
            let step_in_dabtime = (dx * dx + dy * dy).sqrt();
            let fac = 1.0
                - super::exp_decay(
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
                - super::exp_decay(
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
        self.state.actual_radius = radius_log.exp().clamp(ACTUAL_RADIUS_MIN, ACTUAL_RADIUS_MAX);

        // aspect ratio
        self.state.actual_elliptical_dab_ratio =
            self.settings_value[BrushSetting::EllipticalDabRatio as usize];
        self.state.actual_elliptical_dab_angle = mod_arith(
            self.settings_value[BrushSetting::EllipticalDabAngle as usize] - viewrotation + 180.0,
            180.0,
        ) - 180.0;
    }
}

/// 调试用：在 stderr 上打印 brush 输入。对应 mypaint-brush.c:667-699 的 print_inputs。
/// 仅当 brush.print_inputs == true 时调用。
fn print_brush_inputs(inputs: &[f32; NUM_INPUTS], viewrotation: f32, actual_dab_angle: f32) {
    eprint!(
        "press={:6.3}, speed1={:7.4}\tspeed2={:7.4}",
        inputs[BrushInput::Pressure as usize],
        inputs[BrushInput::Speed1 as usize],
        inputs[BrushInput::Speed2 as usize],
    );
    eprint!(
        "\tstroke={:6.3}\tcustom={:6.3}",
        inputs[BrushInput::Stroke as usize],
        inputs[BrushInput::Custom as usize],
    );
    eprint!(
        "\tviewzoom={:6.3}\tviewrotation={:6.3}",
        inputs[BrushInput::Viewzoom as usize],
        viewrotation,
    );
    eprint!(
        "\tasc={:6.3}\tdir={:6.3}\tdec={:6.3}\tdabang={:6.3}",
        inputs[BrushInput::TiltAscension as usize],
        inputs[BrushInput::Direction as usize],
        inputs[BrushInput::TiltDeclination as usize],
        actual_dab_angle,
    );
    eprint!(
        "\txtilt={:6.3}\tytilt={:6.3}attack={:6.3}",
        inputs[BrushInput::TiltDeclinationx as usize],
        inputs[BrushInput::TiltDeclinationy as usize],
        inputs[BrushInput::AttackAngle as usize],
    );
    eprintln!();
}
