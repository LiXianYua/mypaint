//! Brush engine core.
//! Corresponds to MyPaintBrush in mypaint-brush.c.

pub mod state;
pub mod settings;

pub use state::BrushState;
pub use settings::BrushSettingData;

mod stroke;
mod json;

use crate::BrushSetting;
use crate::NUM_INPUTS;
use crate::NUM_SETTINGS;
use crate::SETTING_INFO;
use crate::util::rng::RngDouble;
// state and settings are re-exported above

/// Smudge bucket: R, G, B, A, prevR, prevG, prevB, prevA, recentness
const SMUDGE_BUCKET_SIZE: usize = 9;

/// The MyPaint brush engine.
pub struct Brush {
    pub(crate) settings: [BrushSettingData; NUM_SETTINGS],
    pub(crate) state: BrushState,
    /// Optional 256-bucket smudge state. None / empty → use `inline_bucket`.
    pub(crate) smudge_buckets: Option<Vec<[f32; SMUDGE_BUCKET_SIZE]>>,
    /// Fallback bucket used when no buckets are configured. Mirrors how C
    /// uses STATE(SMUDGE_RA..PREV_COL_RECENTNESS) as a default bucket.
    pub(crate) inline_bucket: [f32; SMUDGE_BUCKET_SIZE],
    /// 已写入的 bucket 索引范围（-1 = 从未写入）。对应 C mypaint_brush.c 的
    /// min_bucket_used / max_bucket_used。
    pub(crate) min_bucket_used: i32,
    pub(crate) max_bucket_used: i32,
    pub(crate) rng: RngDouble,
    pub(crate) print_inputs: bool,
    pub(crate) stroke_total_painting_time: f64,
    pub(crate) stroke_current_idling_time: f64,
    pub(crate) reset_requested: bool,
    pub(crate) skip: f32,
    pub(crate) skip_last_x: f32,
    pub(crate) skip_last_y: f32,
    pub(crate) skipped_dtime: f32,
    pub(crate) random_input: f64,
    pub(crate) speed_mapping_gamma: [f32; 2],
    pub(crate) speed_mapping_m: [f32; 2],
    pub(crate) speed_mapping_q: [f32; 2],
    /// Pre-computed setting values for the current stroke step.
    pub(crate) settings_value: [f32; NUM_SETTINGS],
}

impl Brush {
    pub fn new() -> Self {
        Self::new_with_buckets(0)
    }

    pub fn new_with_buckets(num_smudge_buckets: usize) -> Self {
        // 对应 mypaint-brush.c:197-207：初始 min/max = [0, n-1] 让 brush_reset
        // 把整个 bucket 数组清零；reset 后 min/max → -1。
        let (init_min, init_max) = if num_smudge_buckets > 0 {
            (0_i32, (num_smudge_buckets as i32) - 1)
        } else {
            (-1, -1)
        };
        let mut brush = Self {
            settings: std::array::from_fn(|_| BrushSettingData::new()),
            state: BrushState::zeroed(),
            smudge_buckets: if num_smudge_buckets > 0 {
                Some(vec![[0.0; SMUDGE_BUCKET_SIZE]; num_smudge_buckets])
            } else {
                None
            },
            inline_bucket: [0.0; SMUDGE_BUCKET_SIZE],
            min_bucket_used: init_min,
            max_bucket_used: init_max,
            rng: RngDouble::new(1000),
            print_inputs: false,
            stroke_total_painting_time: 0.0,
            stroke_current_idling_time: 0.0,
            reset_requested: false,
            skip: 0.0,
            skip_last_x: 0.0,
            skip_last_y: 0.0,
            skipped_dtime: 0.0,
            random_input: 0.0,
            speed_mapping_gamma: [0.0; 2],
            speed_mapping_m: [0.0; 2],
            speed_mapping_q: [0.0; 2],
            settings_value: [0.0; NUM_SETTINGS],
        };
        brush.new_stroke();
        brush.settings_base_values_have_changed();
        brush.reset_requested = true;
        brush
    }

    pub(crate) fn brush_reset(&mut self) {
        // 对应 mypaint-brush.c:144-167。
        self.skip = 0.0;
        self.skip_last_x = 0.0;
        self.skip_last_y = 0.0;
        self.skipped_dtime = 0.0;
        self.state = BrushState::zeroed();
        self.state.flip = -1.0;
        // 只清零 [min_bucket_used, max_bucket_used] 范围内的 bucket
        if let Some(buckets) = &mut self.smudge_buckets {
            if self.min_bucket_used != -1 {
                let lo = self.min_bucket_used as usize;
                let hi = (self.max_bucket_used as usize).min(buckets.len().saturating_sub(1));
                for b in &mut buckets[lo..=hi] {
                    *b = [0.0; SMUDGE_BUCKET_SIZE];
                }
                self.min_bucket_used = -1;
                self.max_bucket_used = -1;
            }
        }
        self.inline_bucket = [0.0; SMUDGE_BUCKET_SIZE];
    }

    // ============== Smudge bucket public state API ==============
    // 对应 mypaint-brush.c:447-532

    /// 设置某个 smudge bucket 的全部状态（RGBA + prevRGBA + recentness）。
    /// 返回 true 成功，false 失败（索引越界）。
    pub fn set_smudge_bucket_state(
        &mut self, bucket_index: usize,
        r: f32, g: f32, b: f32, a: f32,
        prev_r: f32, prev_g: f32, prev_b: f32, prev_a: f32,
        prev_color_recentness: f32,
    ) -> bool {
        let Some(buckets) = self.smudge_buckets.as_mut() else { return false };
        if bucket_index >= buckets.len() { return false; }
        let b_slot = &mut buckets[bucket_index];
        b_slot[0] = r; b_slot[1] = g; b_slot[2] = b; b_slot[3] = a;
        b_slot[4] = prev_r; b_slot[5] = prev_g; b_slot[6] = prev_b; b_slot[7] = prev_a;
        b_slot[8] = prev_color_recentness;
        true
    }

    /// 读取某个 smudge bucket 的状态。返回 Some 表示成功。
    pub fn get_smudge_bucket_state(&self, bucket_index: usize)
        -> Option<(f32, f32, f32, f32, f32, f32, f32, f32, f32)>
    {
        let buckets = self.smudge_buckets.as_ref()?;
        if bucket_index >= buckets.len() { return None; }
        let b = &buckets[bucket_index];
        Some((b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8]))
    }

    /// 已写入的最小 bucket 索引（-1 = 从未使用）。
    pub fn min_smudge_bucket_used(&self) -> i32 { self.min_bucket_used }
    /// 已写入的最大 bucket 索引（-1 = 从未使用）。
    pub fn max_smudge_bucket_used(&self) -> i32 { self.max_bucket_used }

    fn settings_base_values_have_changed(&mut self) {
        for i in 0..2 {
            let gamma = (if i == 0 {
                self.settings[BrushSetting::Speed1Gamma as usize].base_value()
            } else {
                self.settings[BrushSetting::Speed2Gamma as usize].base_value()
            }).exp();
            let fix1_x = 45.0;
            let fix1_y = 0.5;
            let fix2_x = 45.0;
            let fix2_dy = 0.015;
            let c1 = (fix1_x + gamma).ln();
            let m = fix2_dy * (fix2_x + gamma);
            let q = fix1_y - m * c1;
            self.speed_mapping_gamma[i] = gamma;
            self.speed_mapping_m[i] = m;
            self.speed_mapping_q[i] = q;
        }
    }

    pub fn reset(&mut self) {
        self.reset_requested = true;
    }

    pub fn new_stroke(&mut self) {
        self.stroke_current_idling_time = 0.0;
        self.stroke_total_painting_time = 0.0;
    }

    /// Total accumulated painting time since the last `new_stroke()`.
    /// 对应 `mypaint_brush_get_total_stroke_painting_time`。
    pub fn total_stroke_painting_time(&self) -> f64 {
        self.stroke_total_painting_time
    }

    pub fn set_base_value(&mut self, id: BrushSetting, value: f32) {
        self.settings[id as usize].set_base_value(value);
        self.settings_base_values_have_changed();
    }

    pub fn get_base_value(&self, id: BrushSetting) -> f32 {
        self.settings[id as usize].base_value()
    }

    pub fn is_constant(&self, id: BrushSetting) -> bool {
        self.settings[id as usize].is_constant()
    }

    pub fn inputs_used_n(&self, id: BrushSetting) -> usize {
        self.settings[id as usize].inputs_used_n()
    }

    pub fn set_mapping_n(&mut self, id: BrushSetting, input: usize, n: usize) {
        self.settings[id as usize].mapping_mut().set_n(input, n);
    }

    pub fn get_mapping_n(&self, id: BrushSetting, input: usize) -> usize {
        self.settings[id as usize].mapping().get_n(input)
    }

    pub fn set_mapping_point(&mut self, id: BrushSetting, input: usize, index: usize, x: f32, y: f32) {
        self.settings[id as usize].mapping_mut().set_point(input, index, x, y);
    }

    pub fn get_mapping_point(&self, id: BrushSetting, input: usize, index: usize) -> (f32, f32) {
        self.settings[id as usize].mapping().get_point(input, index)
    }

    pub fn get_state(&self, state: crate::BrushState) -> f32 {
        self.state.get(state)
    }

    pub fn set_state(&mut self, state: crate::BrushState, value: f32) {
        self.state.set(state, value)
    }

    pub fn from_defaults(&mut self) {
        for s in 0..NUM_SETTINGS {
            for i in 0..NUM_INPUTS {
                self.settings[s].mapping_mut().set_n(i, 0);
            }
            let def = SETTING_INFO[s].def;
            self.settings[s].set_base_value(def);
        }
        // Default: opaque_multiply mapped to pressure
        self.set_mapping_n(BrushSetting::OpaqueMultiply, 0, 2);
        self.set_mapping_point(BrushSetting::OpaqueMultiply, 0, 0, 0.0, 0.0);
        self.set_mapping_point(BrushSetting::OpaqueMultiply, 0, 1, 1.0, 1.0);
    }
}
