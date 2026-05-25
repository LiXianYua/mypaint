use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct BrushSettingsJson {
    inputs: Vec<InputDef>,
    settings: Vec<SettingDef>,
    states: Vec<String>,
}

#[derive(Deserialize)]
struct InputDef {
    id: String,
    hard_minimum: Option<f32>,
    soft_minimum: Option<f32>,
    normal: f32,
    soft_maximum: Option<f32>,
    hard_maximum: Option<f32>,
    displayed_name: String,
    tooltip: String,
}

#[derive(Deserialize)]
struct SettingDef {
    internal_name: String,
    displayed_name: String,
    constant: bool,
    minimum: f32,
    default: f32,
    maximum: f32,
    tooltip: String,
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let json_path = Path::new(&manifest_dir).join("brushsettings.json");
    println!("cargo:rerun-if-changed={}", json_path.display());

    let json_str = fs::read_to_string(&json_path).expect("read brushsettings.json");
    let data: BrushSettingsJson =
        serde_json::from_str(&json_str).expect("parse brushsettings.json");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_settings.rs");

    let mut code = String::new();

    // === BrushInput enum ===
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str("#[repr(usize)]\n");
    code.push_str("pub enum BrushInput {\n");
    for input in &data.inputs {
        let name = to_rust_ident(&input.id);
        code.push_str(&format!("    {name},\n"));
    }
    code.push_str("}\n\n");
    code.push_str(&format!(
        "pub const NUM_INPUTS: usize = {};\n\n",
        data.inputs.len()
    ));

    // Input info struct
    code.push_str("#[derive(Debug, Clone, Copy)]\n");
    code.push_str("pub struct BrushInputInfo {\n");
    code.push_str("    pub cname: &'static str,\n");
    code.push_str("    pub name: &'static str,\n");
    code.push_str("    pub tooltip: &'static str,\n");
    code.push_str("    pub hard_min: Option<f32>,\n");
    code.push_str("    pub soft_min: Option<f32>,\n");
    code.push_str("    pub normal: f32,\n");
    code.push_str("    pub soft_max: Option<f32>,\n");
    code.push_str("    pub hard_max: Option<f32>,\n");
    code.push_str("}\n\n");

    code.push_str("pub const INPUT_INFO: &[BrushInputInfo] = &[\n");
    for input in &data.inputs {
        let name = escape_str(&input.displayed_name);
        let tooltip = escape_str(&input.tooltip);
        let cname = &input.id;
        let hmin = match input.hard_minimum {
            Some(v) => format!("Some({v}f32)"),
            None => "None".to_string(),
        };
        let smin = match input.soft_minimum {
            Some(v) => format!("Some({v}f32)"),
            None => "None".to_string(),
        };
        let smax = match input.soft_maximum {
            Some(v) => format!("Some({v}f32)"),
            None => "None".to_string(),
        };
        let hmax = match input.hard_maximum {
            Some(v) => format!("Some({v}f32)"),
            None => "None".to_string(),
        };
        code.push_str(&format!(
            "    BrushInputInfo {{ cname: \"{cname}\", name: \"{name}\", tooltip: \"{tooltip}\", \
             hard_min: {hmin}, soft_min: {smin}, normal: {}f32, soft_max: {smax}, hard_max: {hmax} }},\n",
            input.normal
        ));
    }
    code.push_str("];\n\n");

    // === BrushSetting enum ===
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str("#[repr(usize)]\n");
    code.push_str("pub enum BrushSetting {\n");
    for setting in &data.settings {
        let name = to_rust_ident(&setting.internal_name);
        code.push_str(&format!("    {name},\n"));
    }
    code.push_str("}\n\n");
    code.push_str(&format!(
        "pub const NUM_SETTINGS: usize = {};\n\n",
        data.settings.len()
    ));

    // Setting info struct
    code.push_str("#[derive(Debug, Clone, Copy)]\n");
    code.push_str("pub struct BrushSettingInfo {\n");
    code.push_str("    pub cname: &'static str,\n");
    code.push_str("    pub name: &'static str,\n");
    code.push_str("    pub tooltip: &'static str,\n");
    code.push_str("    pub constant: bool,\n");
    code.push_str("    pub min: f32,\n");
    code.push_str("    pub def: f32,\n");
    code.push_str("    pub max: f32,\n");
    code.push_str("}\n\n");

    code.push_str("pub const SETTING_INFO: &[BrushSettingInfo] = &[\n");
    for setting in &data.settings {
        let name = escape_str(&setting.displayed_name);
        let tooltip = escape_str(&setting.tooltip);
        let cname = &setting.internal_name;
        code.push_str(&format!(
            "    BrushSettingInfo {{ cname: \"{cname}\", name: \"{name}\", tooltip: \"{tooltip}\", \
             constant: {}, min: {}f32, def: {}f32, max: {}f32 }},\n",
            setting.constant, setting.minimum, setting.default, setting.maximum
        ));
    }
    code.push_str("];\n\n");

    // === BrushState enum ===
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str("#[repr(usize)]\n");
    code.push_str("pub enum BrushState {\n");
    for state in &data.states {
        let name = to_rust_ident(state);
        code.push_str(&format!("    {name},\n"));
    }
    code.push_str("}\n\n");
    code.push_str(&format!(
        "pub const NUM_STATES: usize = {};\n\n",
        data.states.len()
    ));

    // Helper: from_cname for settings
    code.push_str("impl BrushSetting {\n");
    code.push_str("    pub fn from_cname(name: &str) -> Option<Self> {\n");
    code.push_str("        match name {\n");
    for setting in &data.settings {
        let name = to_rust_ident(&setting.internal_name);
        code.push_str(&format!("            \"{}\" => Some(BrushSetting::{name}),\n", setting.internal_name));
    }
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("    pub fn cname(&self) -> &'static str {\n");
    code.push_str("        match self {\n");
    for setting in &data.settings {
        let name = to_rust_ident(&setting.internal_name);
        code.push_str(&format!("            BrushSetting::{name} => \"{}\",\n", setting.internal_name));
    }
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // Helper: from_cname for inputs
    code.push_str("impl BrushInput {\n");
    code.push_str("    pub fn from_cname(name: &str) -> Option<Self> {\n");
    code.push_str("        match name {\n");
    for input in &data.inputs {
        let name = to_rust_ident(&input.id);
        code.push_str(&format!("            \"{}\" => Some(BrushInput::{name}),\n", input.id));
    }
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("    pub fn cname(&self) -> &'static str {\n");
    code.push_str("        match self {\n");
    for input in &data.inputs {
        let name = to_rust_ident(&input.id);
        code.push_str(&format!("            BrushInput::{name} => \"{}\",\n", input.id));
    }
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    fs::write(&dest_path, code).expect("write generated file");
}

fn to_rust_ident(s: &str) -> String {
    let mut result = String::new();
    let mut upper_next = false;
    for c in s.chars() {
        match c {
            '_' => upper_next = true,
            c if upper_next => {
                result.push(c.to_ascii_uppercase());
                upper_next = false;
            }
            c => result.push(c),
        }
    }
    if let Some(first) = result.chars().next() {
        result.replace_range(..1, &first.to_uppercase().to_string());
    }
    result
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
