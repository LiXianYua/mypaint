//! Brush JSON loading. Corresponds to mypaint-brush.c:1549-1681.

use crate::brush::{Brush, BrushParseError};
use crate::BrushInput;
use crate::BrushSetting;

impl Brush {
    /// Load brush settings from a JSON string.
    /// Corresponds to `mypaint_brush_from_string`.
    ///
    /// Unknown setting/input names are tolerated (a warning is printed to
    /// stderr and the entry is skipped), matching upstream `libmypaint`'s
    /// "best-effort" behavior. Returns an error only for hard failures:
    /// malformed JSON, missing `version`/`settings`, wrong types, or
    /// unsupported brush version.
    pub fn from_string(&mut self, string: &str) -> Result<(), BrushParseError> {
        let json: serde_json::Value = serde_json::from_str(string)?;

        let version = json
            .get("version")
            .and_then(|v| v.as_i64())
            .ok_or(BrushParseError::MissingField("version"))?;
        if version != 3 {
            return Err(BrushParseError::UnsupportedVersion(version));
        }

        let settings = json
            .get("settings")
            .ok_or(BrushParseError::MissingField("settings"))?;
        let Some(obj) = settings.as_object() else {
            return Err(BrushParseError::WrongFieldType {
                field: "settings",
                expected: "object",
            });
        };

        for (setting_name, setting_obj) in obj {
            if let Some(setting_id) = BrushSetting::from_cname(setting_name) {
                self.update_setting_from_json(setting_id, setting_obj);
            } else {
                eprintln!("Warning: Unknown setting: {setting_name}");
            }
        }
        Ok(())
    }

    fn update_setting_from_json(
        &mut self,
        setting_id: BrushSetting,
        obj: &serde_json::Value,
    ) -> bool {
        let Some(obj) = obj.as_object() else {
            eprintln!("Warning: Wrong type for setting: {}", setting_id.cname());
            return false;
        };

        // Base value
        if let Some(base_value) = obj.get("base_value").and_then(|v| v.as_f64()) {
            self.set_base_value(setting_id, base_value as f32);
        } else {
            eprintln!("Warning: No 'base_value' for: {}", setting_id.cname());
            return false;
        }

        // Inputs
        if let Some(inputs) = obj.get("inputs").and_then(|v| v.as_object()) {
            for (input_name, input_obj) in inputs {
                if let Some(input_id) = BrushInput::from_cname(input_name) {
                    if let Some(arr) = input_obj.as_array() {
                        let n = arr.len();
                        self.set_mapping_n(setting_id, input_id as usize, n);
                        for (i, point) in arr.iter().enumerate() {
                            if let Some(coords) = point.as_array() {
                                if coords.len() >= 2 {
                                    let x = coords[0].as_f64().unwrap_or(0.0) as f32;
                                    let y = coords[1].as_f64().unwrap_or(0.0) as f32;
                                    self.set_mapping_point(setting_id, input_id as usize, i, x, y);
                                }
                            }
                        }
                    }
                } else {
                    eprintln!("Warning: Unknown input: {input_name}");
                }
            }
        }
        true
    }
}
