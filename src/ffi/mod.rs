//! C FFI compatibility layer. Feature-gated by `ffi`.
//! Exposes the same API surface as the original libmypaint.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use libc::{c_int, c_char};
use std::ffi::CStr;
use std::os::raw::c_void;
use crate::Brush;
use crate::BrushSetting;
use crate::BrushInput;
use crate::BrushState as BrushStateEnum;
use crate::surface::Surface;
use crate::render::DabParams;
use crate::util::rect::Rectangles;

// ============================================================================
// MyPaintBrush opaque handle
// ============================================================================

#[repr(C)]
pub struct MyPaintBrush {
    _private: [u8; 0],
}

#[inline]
unsafe fn brush_ref<'a>(p: *mut MyPaintBrush) -> &'a mut Brush {
    &mut *(p as *mut Brush)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_new() -> *mut MyPaintBrush {
    let brush = Box::new(Brush::new());
    Box::into_raw(brush) as *mut MyPaintBrush
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_new_with_buckets(num_smudge_buckets: c_int) -> *mut MyPaintBrush {
    let brush = Box::new(Brush::new_with_buckets(num_smudge_buckets.max(0) as usize));
    Box::into_raw(brush) as *mut MyPaintBrush
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_unref(self_: *mut MyPaintBrush) {
    if !self_.is_null() {
        drop(Box::from_raw(self_ as *mut Brush));
    }
}

/// Refcount is a no-op in Rust port — ownership is via Box.
#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_ref(_self_: *mut MyPaintBrush) {
    // No-op: Rust uses Box ownership instead of refcounting.
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_reset(self_: *mut MyPaintBrush) {
    brush_ref(self_).reset();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_new_stroke(self_: *mut MyPaintBrush) {
    brush_ref(self_).new_stroke();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_base_value(
    self_: *mut MyPaintBrush, id: c_int, value: f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    brush_ref(self_).set_base_value(setting, value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_base_value(
    self_: *mut MyPaintBrush, id: c_int,
) -> f32 {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0.0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    brush_ref(self_).get_base_value(setting)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_is_constant(
    self_: *mut MyPaintBrush, id: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    if brush_ref(self_).is_constant(setting) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_inputs_used_n(
    self_: *mut MyPaintBrush, id: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    brush_ref(self_).inputs_used_n(setting) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_mapping_n(
    self_: *mut MyPaintBrush, id: c_int, input: c_int, n: c_int,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    brush_ref(self_).set_mapping_n(setting, input as usize, n.max(0) as usize);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_mapping_n(
    self_: *mut MyPaintBrush, id: c_int, input: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return 0; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return 0; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    brush_ref(self_).get_mapping_n(setting, input as usize) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_mapping_point(
    self_: *mut MyPaintBrush, id: c_int, input: c_int, index: c_int, x: f32, y: f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    brush_ref(self_).set_mapping_point(setting, input as usize, index.max(0) as usize, x, y);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_mapping_point(
    self_: *mut MyPaintBrush, id: c_int, input: c_int, index: c_int,
    out_x: *mut f32, out_y: *mut f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    let (x, y) = brush_ref(self_).get_mapping_point(setting, input as usize, index.max(0) as usize);
    if !out_x.is_null() { *out_x = x; }
    if !out_y.is_null() { *out_y = y; }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_state(
    self_: *mut MyPaintBrush, i: c_int,
) -> f32 {
    if i < 0 || i as usize >= crate::NUM_STATES { return 0.0; }
    let state: BrushStateEnum = std::mem::transmute(i as usize);
    brush_ref(self_).get_state(state)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_state(
    self_: *mut MyPaintBrush, i: c_int, value: f32,
) {
    if i < 0 || i as usize >= crate::NUM_STATES { return; }
    let state: BrushStateEnum = std::mem::transmute(i as usize);
    brush_ref(self_).set_state(state, value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_from_defaults(self_: *mut MyPaintBrush) {
    brush_ref(self_).from_defaults();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_from_string(
    self_: *mut MyPaintBrush, string: *const c_char,
) -> c_int {
    if string.is_null() { return 0; }
    let s = match CStr::from_ptr(string).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if brush_ref(self_).from_string(s) { 1 } else { 0 }
}

// ============================================================================
// Surface FFI vtable (the C-side passes a MyPaintSurface struct with function pointers)
// ============================================================================

/// C-compatible Surface vtable. Mirrors `struct MyPaintSurface` in mypaint-surface.h.
#[repr(C)]
pub struct MyPaintSurface {
    pub draw_dab: Option<unsafe extern "C" fn(
        surface: *mut MyPaintSurface, x: f32, y: f32, radius: f32,
        color_r: f32, color_g: f32, color_b: f32,
        opaque: f32, hardness: f32, softness: f32,
        alpha_eraser: f32, aspect_ratio: f32, angle: f32,
        lock_alpha: f32, colorize: f32, posterize: f32,
        posterize_num: f32, paint: f32,
    ) -> c_int>,
    pub get_color: Option<unsafe extern "C" fn(
        surface: *mut MyPaintSurface, x: f32, y: f32, radius: f32,
        out_r: *mut f32, out_g: *mut f32, out_b: *mut f32, out_a: *mut f32,
        paint: f32,
    )>,
    pub begin_atomic: Option<unsafe extern "C" fn(surface: *mut MyPaintSurface)>,
    pub end_atomic: Option<unsafe extern "C" fn(surface: *mut MyPaintSurface, roi: *mut c_void)>,
    pub destroy: Option<unsafe extern "C" fn(surface: *mut MyPaintSurface)>,
    pub save_png: Option<unsafe extern "C" fn(
        surface: *mut MyPaintSurface, path: *const c_char,
        x: c_int, y: c_int, width: c_int, height: c_int,
    )>,
    pub refcount: c_int,
}

/// Adapter that wraps a C-side MyPaintSurface so Rust code can call it via the Surface trait.
struct CSurfaceAdapter {
    c_surface: *mut MyPaintSurface,
}

impl Surface for CSurfaceAdapter {
    fn draw_dab(&mut self, p: &DabParams) -> bool {
        unsafe {
            if let Some(f) = (*self.c_surface).draw_dab {
                f(self.c_surface, p.x, p.y, p.radius,
                  p.color_r, p.color_g, p.color_b,
                  p.opaque, p.hardness, p.softness,
                  p.alpha_eraser, p.aspect_ratio, p.angle,
                  p.lock_alpha, p.colorize, p.posterize, p.posterize_num,
                  p.paint) != 0
            } else { false }
        }
    }

    fn get_color(&mut self, x: f32, y: f32, radius: f32, paint: f32) -> (f32, f32, f32, f32) {
        unsafe {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            if let Some(f) = (*self.c_surface).get_color {
                f(self.c_surface, x, y, radius, &mut r, &mut g, &mut b, &mut a, paint);
            }
            (r, g, b, a)
        }
    }

    fn begin_atomic(&mut self) {
        unsafe {
            if let Some(f) = (*self.c_surface).begin_atomic {
                f(self.c_surface);
            }
        }
    }

    fn end_atomic(&mut self) -> Rectangles {
        unsafe {
            if let Some(f) = (*self.c_surface).end_atomic {
                f(self.c_surface, std::ptr::null_mut());
            }
        }
        Rectangles::default()
    }

    fn save_png(&mut self, path: &std::path::Path, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            if let Some(f) = (*self.c_surface).save_png {
                let path_cstr = match std::ffi::CString::new(path.to_string_lossy().as_bytes()) {
                    Ok(c) => c,
                    Err(_) => return,
                };
                f(self.c_surface, path_cstr.as_ptr(), x, y, w, h);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_stroke_to(
    self_: *mut MyPaintBrush, surface: *mut MyPaintSurface,
    x: f32, y: f32, pressure: f32,
    xtilt: f32, ytilt: f32,
    dtime: f64, viewzoom: f32, viewrotation: f32,
    barrel_rotation: f32, linear: c_int,
) -> c_int {
    let mut adapter = CSurfaceAdapter { c_surface: surface };
    let result = brush_ref(self_).stroke_to(
        &mut adapter, x, y, pressure, xtilt, ytilt,
        dtime, viewzoom, viewrotation, barrel_rotation, linear != 0);
    if result { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_total_stroke_painting_time(
    _self_: *mut MyPaintBrush,
) -> f64 {
    // Not exposed via public API yet; return 0
    0.0
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_print_inputs(
    _self_: *mut MyPaintBrush, _enabled: c_int,
) {
    // No-op: diagnostic only
}

// ============================================================================
// Setting/Input name lookup
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_setting_from_cname(name: *const c_char) -> c_int {
    if name.is_null() { return crate::NUM_SETTINGS as c_int; }
    let s = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return crate::NUM_SETTINGS as c_int,
    };
    match BrushSetting::from_cname(s) {
        Some(setting) => setting as c_int,
        None => crate::NUM_SETTINGS as c_int,
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_input_from_cname(name: *const c_char) -> c_int {
    if name.is_null() { return crate::NUM_INPUTS as c_int; }
    let s = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return crate::NUM_INPUTS as c_int,
    };
    match BrushInput::from_cname(s) {
        Some(input) => input as c_int,
        None => crate::NUM_INPUTS as c_int,
    }
}
