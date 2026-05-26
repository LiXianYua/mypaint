//! C FFI compatibility layer. Feature-gated by `ffi`.
//! Exposes the same API surface as the original libmypaint.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use libc::{c_int, c_char};
use std::ffi::CStr;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::Brush;
use crate::BrushSetting;
use crate::BrushInput;
use crate::BrushState as BrushStateEnum;
use crate::surface::Surface;
use crate::render::DabParams;
use crate::util::rect::Rectangles;

// ============================================================================
// MyPaintBrush opaque handle with refcounting
// ============================================================================

/// FFI wrapper that adds C-style refcounting on top of `Brush`.
/// C 调用者通过 `mypaint_brush_ref`/`unref` 管理生命周期，必须配对。
#[repr(C)]
pub struct MyPaintBrush {
    refcount: AtomicUsize,
    inner: Brush,
}

#[inline]
unsafe fn handle<'a>(p: *mut MyPaintBrush) -> &'a mut Brush {
    &mut (*p).inner
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_new() -> *mut MyPaintBrush {
    let b = Box::new(MyPaintBrush {
        refcount: AtomicUsize::new(1),
        inner: Brush::new(),
    });
    Box::into_raw(b)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_new_with_buckets(num_smudge_buckets: c_int) -> *mut MyPaintBrush {
    let b = Box::new(MyPaintBrush {
        refcount: AtomicUsize::new(1),
        inner: Brush::new_with_buckets(num_smudge_buckets.max(0) as usize),
    });
    Box::into_raw(b)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_ref(self_: *mut MyPaintBrush) {
    if self_.is_null() { return; }
    (*self_).refcount.fetch_add(1, Ordering::AcqRel);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_unref(self_: *mut MyPaintBrush) {
    if self_.is_null() { return; }
    let old = (*self_).refcount.fetch_sub(1, Ordering::AcqRel);
    if old == 1 {
        // 最后一个引用 — drop
        drop(Box::from_raw(self_));
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_reset(self_: *mut MyPaintBrush) {
    handle(self_).reset();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_new_stroke(self_: *mut MyPaintBrush) {
    handle(self_).new_stroke();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_base_value(
    self_: *mut MyPaintBrush, id: c_int, value: f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).set_base_value(setting, value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_base_value(
    self_: *mut MyPaintBrush, id: c_int,
) -> f32 {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0.0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).get_base_value(setting)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_is_constant(
    self_: *mut MyPaintBrush, id: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    if handle(self_).is_constant(setting) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_inputs_used_n(
    self_: *mut MyPaintBrush, id: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).inputs_used_n(setting) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_mapping_n(
    self_: *mut MyPaintBrush, id: c_int, input: c_int, n: c_int,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).set_mapping_n(setting, input as usize, n.max(0) as usize);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_mapping_n(
    self_: *mut MyPaintBrush, id: c_int, input: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return 0; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return 0; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).get_mapping_n(setting, input as usize) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_mapping_point(
    self_: *mut MyPaintBrush, id: c_int, input: c_int, index: c_int, x: f32, y: f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).set_mapping_point(setting, input as usize, index.max(0) as usize, x, y);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_mapping_point(
    self_: *mut MyPaintBrush, id: c_int, input: c_int, index: c_int,
    out_x: *mut f32, out_y: *mut f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS { return; }
    if input < 0 || input as usize >= crate::NUM_INPUTS { return; }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    let (x, y) = handle(self_).get_mapping_point(setting, input as usize, index.max(0) as usize);
    if !out_x.is_null() { *out_x = x; }
    if !out_y.is_null() { *out_y = y; }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_state(
    self_: *mut MyPaintBrush, i: c_int,
) -> f32 {
    if i < 0 || i as usize >= crate::NUM_STATES { return 0.0; }
    let state: BrushStateEnum = std::mem::transmute(i as usize);
    handle(self_).get_state(state)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_state(
    self_: *mut MyPaintBrush, i: c_int, value: f32,
) {
    if i < 0 || i as usize >= crate::NUM_STATES { return; }
    let state: BrushStateEnum = std::mem::transmute(i as usize);
    handle(self_).set_state(state, value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_from_defaults(self_: *mut MyPaintBrush) {
    handle(self_).from_defaults();
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
    if handle(self_).from_string(s) { 1 } else { 0 }
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
    let result = handle(self_).stroke_to(
        &mut adapter, x, y, pressure, xtilt, ytilt,
        dtime, viewzoom, viewrotation, barrel_rotation, linear != 0);
    if result { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_total_stroke_painting_time(
    self_: *mut MyPaintBrush,
) -> f64 {
    handle(self_).total_stroke_painting_time()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_print_inputs(
    _self_: *mut MyPaintBrush, _enabled: c_int,
) {
    // No-op: diagnostic only. Original print_inputs prints to stderr; we skip.
}

/// Empty stub matching the C `mypaint_init()` symbol (which is empty in upstream too).
#[no_mangle]
pub unsafe extern "C" fn mypaint_init() {
    // No-op
}

// ============================================================================
// Setting/Input name lookup
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_setting_from_cname(name: *const c_char) -> c_int {
    if name.is_null() { return -1_i32 as c_int; }
    let s = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return -1_i32 as c_int,
    };
    match BrushSetting::from_cname(s) {
        Some(setting) => setting as c_int,
        None => -1_i32 as c_int,
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_input_from_cname(name: *const c_char) -> c_int {
    if name.is_null() { return -1_i32 as c_int; }
    let s = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return -1_i32 as c_int,
    };
    match BrushInput::from_cname(s) {
        Some(input) => input as c_int,
        None => -1_i32 as c_int,
    }
}

// ============================================================================
// MyPaintSurface 包装函数（mypaint-surface.c）+ FixedTiledSurface 暴露
// 让 C 客户端能直接用 Rust 的 FixedTiledSurface
// ============================================================================

use crate::surface::fixed::FixedTiledSurface;

/// Wrapper: 调用 vtable.draw_dab。对应 mypaint_surface_draw_dab。
#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_draw_dab(
    self_: *mut MyPaintSurface,
    x: f32, y: f32, radius: f32,
    color_r: f32, color_g: f32, color_b: f32,
    opaque: f32, hardness: f32, softness: f32,
    alpha_eraser: f32, aspect_ratio: f32, angle: f32,
    lock_alpha: f32, colorize: f32, posterize: f32,
    posterize_num: f32, paint: f32,
) -> c_int {
    if self_.is_null() { return 0; }
    let Some(f) = (*self_).draw_dab else { return 0 };
    f(self_, x, y, radius, color_r, color_g, color_b,
      opaque, hardness, softness, alpha_eraser, aspect_ratio, angle,
      lock_alpha, colorize, posterize, posterize_num, paint)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_get_color(
    self_: *mut MyPaintSurface, x: f32, y: f32, radius: f32,
    out_r: *mut f32, out_g: *mut f32, out_b: *mut f32, out_a: *mut f32,
    paint: f32,
) {
    if self_.is_null() { return; }
    let Some(f) = (*self_).get_color else { return };
    f(self_, x, y, radius, out_r, out_g, out_b, out_a, paint);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_get_alpha(
    self_: *mut MyPaintSurface, x: f32, y: f32, radius: f32,
) -> f32 {
    let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    mypaint_surface_get_color(self_, x, y, radius, &mut r, &mut g, &mut b, &mut a, 1.0);
    a
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_begin_atomic(self_: *mut MyPaintSurface) {
    if self_.is_null() { return; }
    if let Some(f) = (*self_).begin_atomic { f(self_); }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_end_atomic(self_: *mut MyPaintSurface, roi: *mut c_void) {
    if self_.is_null() { return; }
    if let Some(f) = (*self_).end_atomic { f(self_, roi); }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_save_png(
    self_: *mut MyPaintSurface, path: *const c_char,
    x: c_int, y: c_int, width: c_int, height: c_int,
) {
    if self_.is_null() { return; }
    if let Some(f) = (*self_).save_png { f(self_, path, x, y, width, height); }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_ref(self_: *mut MyPaintSurface) {
    if self_.is_null() { return; }
    (*self_).refcount += 1;
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_unref(self_: *mut MyPaintSurface) {
    if self_.is_null() { return; }
    (*self_).refcount -= 1;
    if (*self_).refcount <= 0 {
        if let Some(destroy) = (*self_).destroy {
            destroy(self_);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_init(self_: *mut MyPaintSurface) {
    if self_.is_null() { return; }
    (*self_).refcount = 1;
}

// ============================================================================
// Rust FixedTiledSurface → C MyPaintSurface 暴露
// ============================================================================

/// FFI wrapper: `MyPaintSurface` vtable + Rust FixedTiledSurface storage。
/// C 客户端把它当作不透明的 MyPaintSurface 句柄使用。
#[repr(C)]
pub struct CFixedTiledSurface {
    /// vtable 必须在头部，与 C 端 cast 兼容
    surface: MyPaintSurface,
    inner: FixedTiledSurface,
}

/// 创建 Rust FixedTiledSurface 给 C 用。返回值可以传给所有 mypaint_surface_* 函数。
#[no_mangle]
pub unsafe extern "C" fn mypaint_fixed_tiled_surface_new(
    width: c_int, height: c_int,
) -> *mut CFixedTiledSurface {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let boxed = Box::new(CFixedTiledSurface {
        surface: MyPaintSurface {
            draw_dab: Some(fixed_draw_dab),
            get_color: Some(fixed_get_color),
            begin_atomic: Some(fixed_begin_atomic),
            end_atomic: Some(fixed_end_atomic),
            destroy: Some(fixed_destroy),
            save_png: Some(fixed_save_png),
            refcount: 1,
        },
        inner: FixedTiledSurface::new(w, h),
    });
    Box::into_raw(boxed)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_fixed_tiled_surface_get_width(
    self_: *mut CFixedTiledSurface,
) -> c_int {
    if self_.is_null() { return 0; }
    (*self_).inner.width() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_fixed_tiled_surface_get_height(
    self_: *mut CFixedTiledSurface,
) -> c_int {
    if self_.is_null() { return 0; }
    (*self_).inner.height() as c_int
}

/// 返回作为通用 `MyPaintSurface*` 的指针（vtable 在结构头部）。
#[no_mangle]
pub unsafe extern "C" fn mypaint_fixed_tiled_surface_interface(
    self_: *mut CFixedTiledSurface,
) -> *mut MyPaintSurface {
    if self_.is_null() { return std::ptr::null_mut(); }
    &mut (*self_).surface
}

// vtable 函数实现 — 把 surface 指针 cast 回 CFixedTiledSurface 后调用 Rust 方法

unsafe extern "C" fn fixed_draw_dab(
    surface: *mut MyPaintSurface,
    x: f32, y: f32, radius: f32,
    color_r: f32, color_g: f32, color_b: f32,
    opaque: f32, hardness: f32, softness: f32,
    alpha_eraser: f32, aspect_ratio: f32, angle: f32,
    lock_alpha: f32, colorize: f32, posterize: f32,
    posterize_num: f32, paint: f32,
) -> c_int {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() { return 0; }
    let params = DabParams {
        x, y, radius, color_r, color_g, color_b,
        opaque, hardness, softness, alpha_eraser,
        aspect_ratio, angle, lock_alpha, colorize,
        posterize, posterize_num, paint,
    };
    use crate::surface::Surface as _;
    if (*s).inner.draw_dab(&params) { 1 } else { 0 }
}

unsafe extern "C" fn fixed_get_color(
    surface: *mut MyPaintSurface, x: f32, y: f32, radius: f32,
    out_r: *mut f32, out_g: *mut f32, out_b: *mut f32, out_a: *mut f32,
    paint: f32,
) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() { return; }
    use crate::surface::Surface as _;
    let (r, g, b, a) = (*s).inner.get_color(x, y, radius, paint);
    if !out_r.is_null() { *out_r = r; }
    if !out_g.is_null() { *out_g = g; }
    if !out_b.is_null() { *out_b = b; }
    if !out_a.is_null() { *out_a = a; }
}

unsafe extern "C" fn fixed_begin_atomic(surface: *mut MyPaintSurface) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() { return; }
    use crate::surface::Surface as _;
    (*s).inner.begin_atomic();
}

unsafe extern "C" fn fixed_end_atomic(surface: *mut MyPaintSurface, _roi: *mut c_void) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() { return; }
    use crate::surface::Surface as _;
    let _ = (*s).inner.end_atomic();
    // TODO: roi 写回（如果 C 客户端提供了 MyPaintRectangles）
}

unsafe extern "C" fn fixed_destroy(surface: *mut MyPaintSurface) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() { return; }
    drop(Box::from_raw(s));
}

unsafe extern "C" fn fixed_save_png(
    surface: *mut MyPaintSurface, path: *const c_char,
    x: c_int, y: c_int, width: c_int, height: c_int,
) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() || path.is_null() { return; }
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return,
    };
    use crate::surface::Surface as _;
    (*s).inner.save_png(std::path::Path::new(path_str), x, y, width, height);
}
