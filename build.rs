fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("generated_settings.rs");
    std::fs::write(dest, "pub const NUM_INPUTS: usize = 18;\npub const NUM_SETTINGS: usize = 55;\npub const NUM_STATES: usize = 44;\n").unwrap();
    println!("cargo:rerun-if-changed=brushsettings.json");
}
