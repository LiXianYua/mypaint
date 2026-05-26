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
        let mut brush = Self {
            settings: std::array::from_fn(|_| BrushSettingData::new()),
            state: BrushState::zeroed(),
            smudge_buckets: if num_smudge_buckets > 0 {
                Some(vec![[0.0; SMUDGE_BUCKET_SIZE]; num_smudge_buckets])
            } else {
                None
            },
            inline_bucket: [0.0; SMUDGE_BUCKET_SIZE],
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
        self.skip = 0.0;
        self.skip_last_x = 0.0;
        self.skip_last_y = 0.0;
        self.skipped_dtime = 0.0;
        self.state = BrushState::zeroed();
        self.state.flip = -1.0;
        if let Some(buckets) = &mut self.smudge_buckets {
            for b in buckets.iter_mut() {
                *b = [0.0; SMUDGE_BUCKET_SIZE];
            }
        }
        self.inline_bucket = [0.0; SMUDGE_BUCKET_SIZE];
    }

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
