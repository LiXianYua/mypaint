//! Dab generation + time-discretization loop.
//!
//! 对应 mypaint-brush.c:1042-1287：
//! - prepare_and_draw_dab (L1042-1250)
//! - count_dabs_to (L1253-1287)
//! - 以及把 dab 循环（C 的 `mypaint_brush_stroke_to` 主体）抽成的
//!   `paint_dabs_for_timestep` + `make_step` helper

use super::{
    baseval, PaintDabsResult, StrokeContext, StrokeStep, ACTUAL_RADIUS_MAX, ACTUAL_RADIUS_MIN,
};
use crate::brush::Brush;
use crate::render::color::*;
use crate::render::DabParams;
use crate::surface::Surface;
use crate::util::helpers::{rand_gauss, smallest_angular_difference};
use crate::BrushSetting;

impl Brush {
    // =========================================================================
    // prepare_and_draw_dab — mypaint-brush.c:1042-1250
    // =========================================================================

    pub(super) fn prepare_and_draw_dab(&mut self, surface: &mut dyn Surface, linear: bool) -> bool {
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
        let (offset_x, offset_y) = self.directional_offsets(base_radius, self.state.flip as i32);
        x += offset_x;
        y += offset_y;

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
            let return_early = self.update_smudge_color(
                surface,
                smudge_length,
                smudge_length_log,
                smudge_radius_log,
                smudge_op_lim,
                x.round() as i32,
                y.round() as i32,
                radius,
                legacy_smudge,
                paint_factor,
            );
            if return_early {
                return false;
            }
        }

        let mut eraser_target_alpha = 1.0;
        let smudge_value = self.settings_value[BrushSetting::Smudge as usize];
        if smudge_value > 0.0 {
            eraser_target_alpha = self.apply_smudge(
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

    pub(super) fn count_dabs(&mut self, x: f32, y: f32, dtime: f32) -> f32 {
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

    /// 在给定 timestep 内推进 brush state 并 draw 出所有 dab。
    ///
    /// 对应 mypaint-brush.c:1428-1490 的内部循环 + catch-up step。
    /// 主循环负责：在 `dabs_moved + dabs_todo >= 1.0` 时拆出一个 1-dab 子步，
    /// 推进 state，画 dab。循环结束后做一个 `dabs_todo` 长度的 catch-up
    /// 把剩余时间走完（不画 dab）。最后把 `dabs_moved + dabs_todo` 存到
    /// `state.partial_dabs` 供下次 stroke_to 复用。
    pub(super) fn paint_dabs_for_timestep(
        &mut self,
        surface: &mut dyn Surface,
        ctx: &StrokeContext,
        linear: bool,
        dtime: f32,
    ) -> PaintDabsResult {
        let mut painted: Option<bool> = None;
        let mut dtime_left = dtime;
        // 持久化 step_dpressure（stroke-split 检查需要最后一次的值，
        // 对应 C 的 `step_dpressure` 在循环外声明）
        let mut last_step_dpressure: f32 = 0.0;
        let mut dabs_moved = self.state.partial_dabs;
        let mut dabs_todo = self.count_dabs(ctx.input_x, ctx.input_y, dtime);

        while dabs_moved + dabs_todo >= 1.0 {
            let step = if dabs_moved > 0.001 {
                let ddab = 1.0 - dabs_moved;
                dabs_moved = 0.0;
                let frac = ddab / dabs_todo;
                self.make_step(ctx, frac, ddab, dtime_left)
            } else {
                let frac = 1.0 / dabs_todo;
                self.make_step(ctx, frac, 1.0, dtime_left)
            };

            last_step_dpressure = step.dpressure;
            let step_dtime = step.dtime;
            self.update_states(&step, ctx.viewzoom, ctx.viewrotation);

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
            dabs_todo = self.count_dabs(ctx.input_x, ctx.input_y, dtime_left);
        }

        // Move brush to current time (no more dab will happen)
        // 对应 mypaint-brush.c:1482，使用 dabs_todo (不含 dabs_moved)
        let catch_up = StrokeStep {
            ddab: dabs_todo,
            dx: ctx.input_x - self.state.x,
            dy: ctx.input_y - self.state.y,
            dpressure: ctx.pressure - self.state.pressure,
            dtime: dtime_left,
            declination: ctx.tilt_declination - self.state.declination,
            ascension: smallest_angular_difference(self.state.ascension, ctx.tilt_ascension),
            declinationx: ctx.tilt_declinationx - self.state.declinationx,
            declinationy: ctx.tilt_declinationy - self.state.declinationy,
            barrel_rotation: smallest_angular_difference(
                self.state.barrel_rotation,
                ctx.barrel_rotation * 360.0,
            ),
        };
        self.update_states(&catch_up, ctx.viewzoom, ctx.viewrotation);

        // save the fraction of a dab that is already done
        self.state.partial_dabs = dabs_moved + dabs_todo;

        PaintDabsResult {
            painted,
            last_step_dpressure,
        }
    }

    /// 构造一个 fractional StrokeStep — 两个 dabs loop 分支共用的算式，
    /// 抽出消除 ~30 行复制粘贴。两个分支的差异在外层（`ddab` 怎么算 +
    /// `dabs_moved` 是否清零），传进来的 `(frac, ddab)` 不同。
    #[inline]
    fn make_step(&self, ctx: &StrokeContext, frac: f32, ddab: f32, dtime_left: f32) -> StrokeStep {
        StrokeStep {
            ddab,
            dx: frac * (ctx.input_x - self.state.x),
            dy: frac * (ctx.input_y - self.state.y),
            dpressure: frac * (ctx.pressure - self.state.pressure),
            dtime: frac * dtime_left,
            declination: frac * (ctx.tilt_declination - self.state.declination),
            ascension: frac * smallest_angular_difference(self.state.ascension, ctx.tilt_ascension),
            declinationx: frac * (ctx.tilt_declinationx - self.state.declinationx),
            declinationy: frac * (ctx.tilt_declinationy - self.state.declinationy),
            barrel_rotation: frac
                * smallest_angular_difference(
                    self.state.barrel_rotation,
                    ctx.barrel_rotation * 360.0,
                ),
        }
    }
}
