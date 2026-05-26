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

mod dab;
mod smudge;
mod state_update;

use crate::brush::Brush;
use crate::surface::Surface;
use crate::util::helpers::rand_gauss;
use crate::BrushSetting;

const ACTUAL_RADIUS_MIN: f32 = 0.2;
const ACTUAL_RADIUS_MAX: f32 = 1000.0;
const GRID_SIZE: f32 = 256.0;

/// 一步（一个 dab 时间窗，或时间窗尾的 catch-up）的 deltas。
///
/// 由 `stroke_to` 在 time-discretization 循环里组装，交给
/// [`Brush::update_states`] 推进 `BrushState`。把以前的 10 个 `step_*`
/// 位置参数收拢成一个 `Copy` 小结构（10×f32 = 40 字节），call site 用
/// 字段名初始化、命名后零成本。
///
/// `viewzoom` / `viewrotation` 不在这里 — 它们是 `stroke_to` 这一整次
/// 调用的常量，不是单步 delta。
/// 一次 `stroke_to` 调用期间的不变量上下文。bundled 给 `paint_dabs_for_timestep`
/// 和 `make_step` 这些内部 helper 用，避免传 10 个独立 f32 参数。
#[derive(Clone, Copy, Debug)]
struct StrokeContext {
    input_x: f32,
    input_y: f32,
    pressure: f32,
    tilt_ascension: f32,
    tilt_declination: f32,
    tilt_declinationx: f32,
    tilt_declinationy: f32,
    viewzoom: f32,
    /// 弧度（未 normalize）。
    viewrotation: f32,
    /// 单位 turn (0..=1)。后续 `* 360.0` 转度。
    barrel_rotation: f32,
}

/// `paint_dabs_for_timestep` 的输出，给 stroke-split 决策用。
#[derive(Clone, Copy, Debug)]
struct PaintDabsResult {
    /// `Some(true)` = 该 timestep 内画了至少一个 dab；`Some(false)` = 没画
    /// （但 op 被处理过）；`None` = 没进过 loop。
    painted: Option<bool>,
    /// 循环里最后一个 dab 的 `step.dpressure`，stroke-split 决策需要。
    last_step_dpressure: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct StrokeStep {
    /// 这一步消耗的 dab 分数（0..=1）。
    ddab: f32,
    /// 位置 / 压力 / 时间增量。
    dx: f32,
    dy: f32,
    dpressure: f32,
    dtime: f32,
    /// 倾角 / 偏角增量。
    declination: f32,
    ascension: f32,
    declinationx: f32,
    declinationy: f32,
    /// 笔杆旋转增量（度，已 mod 360）。
    barrel_rotation: f32,
}

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

// directional_offsets + update_states + print_brush_inputs 拆到 state_update.rs
// smudge bucket fetch + update + apply 拆到 smudge.rs

impl Brush {
    // =========================================================================
    // exp_decay helper — mypaint-brush.c:534-544
    // =========================================================================

    #[inline]
    pub(super) fn exp_decay(t_const: f32, t: f32) -> f32 {
        if t_const <= 0.001 {
            return 0.0;
        }
        (-t / t_const).exp()
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

            if self.skip > 0.001 && !(dtime > max_dtime || self.reset_requested) {
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
        if dtime > max_dtime || self.reset_requested {
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
        let ctx = StrokeContext {
            input_x,
            input_y,
            pressure,
            tilt_ascension,
            tilt_declination,
            tilt_declinationx,
            tilt_declinationy,
            viewzoom,
            viewrotation,
            barrel_rotation,
        };
        let PaintDabsResult {
            painted,
            last_step_dpressure,
        } = self.paint_dabs_for_timestep(surface, &ctx, linear, dtime as f32);

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
