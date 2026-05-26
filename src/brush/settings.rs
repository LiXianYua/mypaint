//! Per-setting data: a mapping curve plus base value.

use crate::mapping::Mapping;
use crate::NUM_INPUTS;

pub struct BrushSettingData {
    mapping: Mapping,
}

impl Default for BrushSettingData {
    fn default() -> Self {
        Self::new()
    }
}

impl BrushSettingData {
    pub fn new() -> Self {
        Self {
            mapping: Mapping::new(NUM_INPUTS),
        }
    }

    pub fn base_value(&self) -> f32 {
        self.mapping.get_base_value()
    }

    pub fn set_base_value(&mut self, value: f32) {
        self.mapping.set_base_value(value);
    }

    pub fn mapping(&self) -> &Mapping {
        &self.mapping
    }

    pub fn mapping_mut(&mut self) -> &mut Mapping {
        &mut self.mapping
    }

    pub fn is_constant(&self) -> bool {
        self.mapping.is_constant()
    }

    pub fn inputs_used_n(&self) -> usize {
        self.mapping.inputs_used_n()
    }
}
