//! libmypaint — A Rust port of the libmypaint brush engine.
//!
//! This crate provides a brush engine for making brushstrokes,
//! ported from the C library libmypaint (https://github.com/mypaint/libmypaint).

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
pub use brush::{Brush, BrushError, BrushParseError};
pub use surface::Surface;
