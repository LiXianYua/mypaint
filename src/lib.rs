//! libmypaint — A Rust port of the libmypaint brush engine.
//!
//! This crate provides a brush engine for making brushstrokes,
//! ported from the C library libmypaint (https://github.com/mypaint/libmypaint).

// 大量 f32 字面量是从 mypaint C 上游 helpers.c / brushsettings.json 等
// 处直译过来的 — 写法保留 f64-级精度数字方便逐行对照 C 源（实际编译
// 时被截断到 f32 精度，行为不变）。Rust 自动 lint 不喜欢这种风格，
// 但手改 60+ 处字面量脱离上游对照度，得不偿失，crate 级关闭这个 lint。
#![allow(clippy::excessive_precision)]

include!(concat!(env!("OUT_DIR"), "/generated_settings.rs"));

pub mod brush;
pub mod mapping;
pub mod render;
pub mod smudge;
pub mod surface;
pub mod symmetry;
pub mod util;

#[cfg(feature = "ffi")]
pub mod ffi;

// Re-exports
pub use brush::{Brush, BrushError, BrushParseError, SmudgeBucket, StrokeInputs};
pub use surface::Surface;
