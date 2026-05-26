//! C FFI compatibility layer. Feature-gated by `ffi`.
//! Exposes the same API surface as the original libmypaint.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use libc::{c_char, c_int};
use std::ffi::CStr;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

// ============================================================================
// MyPaintRectangle / MyPaintRectangles — C ABI 兼容类型
// 对应 mypaint-rectangle.h:26-36
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MyPaintRectangle {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

/// C-side rectangle batch: `num_rectangles` is input/output (capacity → actual count
/// written by `end_atomic`). `rectangles` must point to that many slots.
#[repr(C)]
pub struct MyPaintRectangles {
    pub num_rectangles: c_int,
    pub rectangles: *mut MyPaintRectangle,
}

/// 对应 mypaint_rectangle_expand_to_include_point。
#[no_mangle]
pub unsafe extern "C" fn mypaint_rectangle_expand_to_include_point(
    r: *mut MyPaintRectangle,
    x: c_int,
    y: c_int,
) {
    if r.is_null() {
        return;
    }
    let rr = &mut *r;
    let mut rect = crate::util::rect::Rect::new(rr.x, rr.y, rr.width, rr.height);
    rect.expand_to_include_point(x, y);
    rr.x = rect.x;
    rr.y = rect.y;
    rr.width = rect.width;
    rr.height = rect.height;
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_rectangle_expand_to_include_rect(
    r: *mut MyPaintRectangle,
    other: *const MyPaintRectangle,
) {
    if r.is_null() || other.is_null() {
        return;
    }
    let rr = &mut *r;
    let oo = &*other;
    let mut rect = crate::util::rect::Rect::new(rr.x, rr.y, rr.width, rr.height);
    let other_rect = crate::util::rect::Rect::new(oo.x, oo.y, oo.width, oo.height);
    rect.expand_to_include_rect(&other_rect);
    rr.x = rect.x;
    rr.y = rect.y;
    rr.width = rect.width;
    rr.height = rect.height;
}

/// 对应 mypaint_rectangle_copy。返回的指针由调用者用 free() 释放。
#[no_mangle]
pub unsafe extern "C" fn mypaint_rectangle_copy(
    src: *const MyPaintRectangle,
) -> *mut MyPaintRectangle {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let copy = Box::new(*src);
    Box::into_raw(copy)
}
use crate::render::DabParams;
use crate::surface::Surface;
use crate::util::rect::Rectangles;
use crate::Brush;
use crate::BrushInput;
use crate::BrushSetting;
use crate::BrushState as BrushStateEnum;

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
pub unsafe extern "C" fn mypaint_brush_new_with_buckets(
    num_smudge_buckets: c_int,
) -> *mut MyPaintBrush {
    let b = Box::new(MyPaintBrush {
        refcount: AtomicUsize::new(1),
        inner: Brush::new_with_buckets(num_smudge_buckets.max(0) as usize),
    });
    Box::into_raw(b)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_ref(self_: *mut MyPaintBrush) {
    if self_.is_null() {
        return;
    }
    (*self_).refcount.fetch_add(1, Ordering::AcqRel);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_unref(self_: *mut MyPaintBrush) {
    if self_.is_null() {
        return;
    }
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
    self_: *mut MyPaintBrush,
    id: c_int,
    value: f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).set_base_value(setting, value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_base_value(self_: *mut MyPaintBrush, id: c_int) -> f32 {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0.0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).get_base_value(setting)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_is_constant(self_: *mut MyPaintBrush, id: c_int) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    if handle(self_).is_constant(setting) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_inputs_used_n(
    self_: *mut MyPaintBrush,
    id: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).inputs_used_n(setting) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_mapping_n(
    self_: *mut MyPaintBrush,
    id: c_int,
    input: c_int,
    n: c_int,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return;
    }
    if input < 0 || input as usize >= crate::NUM_INPUTS {
        return;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).set_mapping_n(setting, input as usize, n.max(0) as usize);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_mapping_n(
    self_: *mut MyPaintBrush,
    id: c_int,
    input: c_int,
) -> c_int {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return 0;
    }
    if input < 0 || input as usize >= crate::NUM_INPUTS {
        return 0;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).get_mapping_n(setting, input as usize) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_mapping_point(
    self_: *mut MyPaintBrush,
    id: c_int,
    input: c_int,
    index: c_int,
    x: f32,
    y: f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return;
    }
    if input < 0 || input as usize >= crate::NUM_INPUTS {
        return;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    handle(self_).set_mapping_point(setting, input as usize, index.max(0) as usize, x, y);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_mapping_point(
    self_: *mut MyPaintBrush,
    id: c_int,
    input: c_int,
    index: c_int,
    out_x: *mut f32,
    out_y: *mut f32,
) {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return;
    }
    if input < 0 || input as usize >= crate::NUM_INPUTS {
        return;
    }
    let setting: BrushSetting = std::mem::transmute(id as usize);
    let (x, y) = handle(self_).get_mapping_point(setting, input as usize, index.max(0) as usize);
    if !out_x.is_null() {
        *out_x = x;
    }
    if !out_y.is_null() {
        *out_y = y;
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_state(self_: *mut MyPaintBrush, i: c_int) -> f32 {
    if i < 0 || i as usize >= crate::NUM_STATES {
        return 0.0;
    }
    let state: BrushStateEnum = std::mem::transmute(i as usize);
    handle(self_).get_state(state)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_state(self_: *mut MyPaintBrush, i: c_int, value: f32) {
    if i < 0 || i as usize >= crate::NUM_STATES {
        return;
    }
    let state: BrushStateEnum = std::mem::transmute(i as usize);
    handle(self_).set_state(state, value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_from_defaults(self_: *mut MyPaintBrush) {
    handle(self_).from_defaults();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_from_string(
    self_: *mut MyPaintBrush,
    string: *const c_char,
) -> c_int {
    if string.is_null() {
        return 0;
    }
    let s = match CStr::from_ptr(string).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if handle(self_).from_string(s) {
        1
    } else {
        0
    }
}

// ============================================================================
// Surface FFI vtable (the C-side passes a MyPaintSurface struct with function pointers)
// ============================================================================

/// C-compatible Surface vtable. Mirrors `struct MyPaintSurface` in mypaint-surface.h.
#[repr(C)]
pub struct MyPaintSurface {
    pub draw_dab: Option<
        unsafe extern "C" fn(
            surface: *mut MyPaintSurface,
            x: f32,
            y: f32,
            radius: f32,
            color_r: f32,
            color_g: f32,
            color_b: f32,
            opaque: f32,
            hardness: f32,
            softness: f32,
            alpha_eraser: f32,
            aspect_ratio: f32,
            angle: f32,
            lock_alpha: f32,
            colorize: f32,
            posterize: f32,
            posterize_num: f32,
            paint: f32,
        ) -> c_int,
    >,
    pub get_color: Option<
        unsafe extern "C" fn(
            surface: *mut MyPaintSurface,
            x: f32,
            y: f32,
            radius: f32,
            out_r: *mut f32,
            out_g: *mut f32,
            out_b: *mut f32,
            out_a: *mut f32,
            paint: f32,
        ),
    >,
    pub begin_atomic: Option<unsafe extern "C" fn(surface: *mut MyPaintSurface)>,
    pub end_atomic:
        Option<unsafe extern "C" fn(surface: *mut MyPaintSurface, roi: *mut MyPaintRectangles)>,
    pub destroy: Option<unsafe extern "C" fn(surface: *mut MyPaintSurface)>,
    pub save_png: Option<
        unsafe extern "C" fn(
            surface: *mut MyPaintSurface,
            path: *const c_char,
            x: c_int,
            y: c_int,
            width: c_int,
            height: c_int,
        ),
    >,
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
                f(
                    self.c_surface,
                    p.x,
                    p.y,
                    p.radius,
                    p.color_r,
                    p.color_g,
                    p.color_b,
                    p.opaque,
                    p.hardness,
                    p.softness,
                    p.alpha_eraser,
                    p.aspect_ratio,
                    p.angle,
                    p.lock_alpha,
                    p.colorize,
                    p.posterize,
                    p.posterize_num,
                    p.paint,
                ) != 0
            } else {
                false
            }
        }
    }

    fn get_color(&mut self, x: f32, y: f32, radius: f32, paint: f32) -> (f32, f32, f32, f32) {
        unsafe {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            if let Some(f) = (*self.c_surface).get_color {
                f(
                    self.c_surface,
                    x,
                    y,
                    radius,
                    &mut r,
                    &mut g,
                    &mut b,
                    &mut a,
                    paint,
                );
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
                // 我们不向 C surface 索取 ROI（无 buffer 分配），直接传 null
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
    self_: *mut MyPaintBrush,
    surface: *mut MyPaintSurface,
    x: f32,
    y: f32,
    pressure: f32,
    xtilt: f32,
    ytilt: f32,
    dtime: f64,
    viewzoom: f32,
    viewrotation: f32,
    barrel_rotation: f32,
    linear: c_int,
) -> c_int {
    let mut adapter = CSurfaceAdapter { c_surface: surface };
    let result = handle(self_).stroke_to(
        &mut adapter,
        x,
        y,
        pressure,
        xtilt,
        ytilt,
        dtime,
        viewzoom,
        viewrotation,
        barrel_rotation,
        linear != 0,
    );
    if result {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_total_stroke_painting_time(
    self_: *mut MyPaintBrush,
) -> f64 {
    handle(self_).total_stroke_painting_time()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_print_inputs(
    _self_: *mut MyPaintBrush,
    _enabled: c_int,
) {
    // No-op: diagnostic only. Original print_inputs prints to stderr; we skip.
}

/// Empty stub matching the C `mypaint_init()` symbol (which is empty in upstream too).
#[no_mangle]
pub unsafe extern "C" fn mypaint_init() {
    // No-op
}

// ============================================================================
// Smudge bucket state APIs — 对应 mypaint-brush.c:455-532
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_set_smudge_bucket_state(
    self_: *mut MyPaintBrush,
    bucket_index: c_int,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
    prev_r: f32,
    prev_g: f32,
    prev_b: f32,
    prev_a: f32,
    prev_color_recentness: f32,
) -> c_int {
    if self_.is_null() || bucket_index < 0 {
        return 0;
    }
    let ok = handle(self_).set_smudge_bucket_state(
        bucket_index as usize,
        r,
        g,
        b,
        a,
        prev_r,
        prev_g,
        prev_b,
        prev_a,
        prev_color_recentness,
    );
    if ok {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_smudge_bucket_state(
    self_: *const MyPaintBrush,
    bucket_index: c_int,
    r: *mut f32,
    g: *mut f32,
    b: *mut f32,
    a: *mut f32,
    prev_r: *mut f32,
    prev_g: *mut f32,
    prev_b: *mut f32,
    prev_a: *mut f32,
    prev_color_recentness: *mut f32,
) -> c_int {
    if self_.is_null() || bucket_index < 0 {
        return 0;
    }
    let brush = &(*self_).inner;
    match brush.get_smudge_bucket_state(bucket_index as usize) {
        Some((rv, gv, bv, av, prv, pgv, pbv, pav, pcrv)) => {
            if !r.is_null() {
                *r = rv;
            }
            if !g.is_null() {
                *g = gv;
            }
            if !b.is_null() {
                *b = bv;
            }
            if !a.is_null() {
                *a = av;
            }
            if !prev_r.is_null() {
                *prev_r = prv;
            }
            if !prev_g.is_null() {
                *prev_g = pgv;
            }
            if !prev_b.is_null() {
                *prev_b = pbv;
            }
            if !prev_a.is_null() {
                *prev_a = pav;
            }
            if !prev_color_recentness.is_null() {
                *prev_color_recentness = pcrv;
            }
            1
        }
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_min_smudge_bucket_used(
    self_: *const MyPaintBrush,
) -> c_int {
    if self_.is_null() {
        return -1;
    }
    (*self_).inner.min_smudge_bucket_used() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_get_max_smudge_bucket_used(
    self_: *const MyPaintBrush,
) -> c_int {
    if self_.is_null() {
        return -1;
    }
    (*self_).inner.max_smudge_bucket_used() as c_int
}

// ============================================================================
// Setting/Input name lookup
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_setting_from_cname(name: *const c_char) -> c_int {
    if name.is_null() {
        return -1_i32 as c_int;
    }
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
    if name.is_null() {
        return -1_i32 as c_int;
    }
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
    x: f32,
    y: f32,
    radius: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
    opaque: f32,
    hardness: f32,
    softness: f32,
    alpha_eraser: f32,
    aspect_ratio: f32,
    angle: f32,
    lock_alpha: f32,
    colorize: f32,
    posterize: f32,
    posterize_num: f32,
    paint: f32,
) -> c_int {
    if self_.is_null() {
        return 0;
    }
    let Some(f) = (*self_).draw_dab else { return 0 };
    f(
        self_,
        x,
        y,
        radius,
        color_r,
        color_g,
        color_b,
        opaque,
        hardness,
        softness,
        alpha_eraser,
        aspect_ratio,
        angle,
        lock_alpha,
        colorize,
        posterize,
        posterize_num,
        paint,
    )
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_get_color(
    self_: *mut MyPaintSurface,
    x: f32,
    y: f32,
    radius: f32,
    out_r: *mut f32,
    out_g: *mut f32,
    out_b: *mut f32,
    out_a: *mut f32,
    paint: f32,
) {
    if self_.is_null() {
        return;
    }
    let Some(f) = (*self_).get_color else { return };
    f(self_, x, y, radius, out_r, out_g, out_b, out_a, paint);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_get_alpha(
    self_: *mut MyPaintSurface,
    x: f32,
    y: f32,
    radius: f32,
) -> f32 {
    let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    mypaint_surface_get_color(self_, x, y, radius, &mut r, &mut g, &mut b, &mut a, 1.0);
    a
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_begin_atomic(self_: *mut MyPaintSurface) {
    if self_.is_null() {
        return;
    }
    if let Some(f) = (*self_).begin_atomic {
        f(self_);
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_end_atomic(
    self_: *mut MyPaintSurface,
    roi: *mut MyPaintRectangles,
) {
    if self_.is_null() {
        return;
    }
    if let Some(f) = (*self_).end_atomic {
        f(self_, roi);
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_save_png(
    self_: *mut MyPaintSurface,
    path: *const c_char,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
) {
    if self_.is_null() {
        return;
    }
    if let Some(f) = (*self_).save_png {
        f(self_, path, x, y, width, height);
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_ref(self_: *mut MyPaintSurface) {
    if self_.is_null() {
        return;
    }
    (*self_).refcount += 1;
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_unref(self_: *mut MyPaintSurface) {
    if self_.is_null() {
        return;
    }
    (*self_).refcount -= 1;
    if (*self_).refcount <= 0 {
        if let Some(destroy) = (*self_).destroy {
            destroy(self_);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_surface_init(self_: *mut MyPaintSurface) {
    if self_.is_null() {
        return;
    }
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
    width: c_int,
    height: c_int,
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
    if self_.is_null() {
        return 0;
    }
    (*self_).inner.width() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_fixed_tiled_surface_get_height(
    self_: *mut CFixedTiledSurface,
) -> c_int {
    if self_.is_null() {
        return 0;
    }
    (*self_).inner.height() as c_int
}

/// 返回作为通用 `MyPaintSurface*` 的指针（vtable 在结构头部）。
#[no_mangle]
pub unsafe extern "C" fn mypaint_fixed_tiled_surface_interface(
    self_: *mut CFixedTiledSurface,
) -> *mut MyPaintSurface {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    &mut (*self_).surface
}

// vtable 函数实现 — 把 surface 指针 cast 回 CFixedTiledSurface 后调用 Rust 方法

unsafe extern "C" fn fixed_draw_dab(
    surface: *mut MyPaintSurface,
    x: f32,
    y: f32,
    radius: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
    opaque: f32,
    hardness: f32,
    softness: f32,
    alpha_eraser: f32,
    aspect_ratio: f32,
    angle: f32,
    lock_alpha: f32,
    colorize: f32,
    posterize: f32,
    posterize_num: f32,
    paint: f32,
) -> c_int {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() {
        return 0;
    }
    let params = DabParams {
        x,
        y,
        radius,
        color_r,
        color_g,
        color_b,
        opaque,
        hardness,
        softness,
        alpha_eraser,
        aspect_ratio,
        angle,
        lock_alpha,
        colorize,
        posterize,
        posterize_num,
        paint,
    };
    use crate::surface::Surface as _;
    if (*s).inner.draw_dab(&params) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn fixed_get_color(
    surface: *mut MyPaintSurface,
    x: f32,
    y: f32,
    radius: f32,
    out_r: *mut f32,
    out_g: *mut f32,
    out_b: *mut f32,
    out_a: *mut f32,
    paint: f32,
) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() {
        return;
    }
    use crate::surface::Surface as _;
    let (r, g, b, a) = (*s).inner.get_color(x, y, radius, paint);
    if !out_r.is_null() {
        *out_r = r;
    }
    if !out_g.is_null() {
        *out_g = g;
    }
    if !out_b.is_null() {
        *out_b = b;
    }
    if !out_a.is_null() {
        *out_a = a;
    }
}

unsafe extern "C" fn fixed_begin_atomic(surface: *mut MyPaintSurface) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() {
        return;
    }
    use crate::surface::Surface as _;
    (*s).inner.begin_atomic();
}

unsafe extern "C" fn fixed_end_atomic(surface: *mut MyPaintSurface, roi: *mut MyPaintRectangles) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() {
        return;
    }
    use crate::surface::Surface as _;
    let rects = (*s).inner.end_atomic();

    if roi.is_null() {
        return;
    }
    let r = &mut *roi;
    if r.rectangles.is_null() || r.num_rectangles <= 0 {
        return;
    }
    let cap = r.num_rectangles as usize;

    // 对应 mypaint-tiled-surface.c:123-148 — 先清空可能要被写入的槽
    let slots = std::slice::from_raw_parts_mut(r.rectangles, cap);
    for slot in slots.iter_mut() {
        *slot = MyPaintRectangle::default();
    }

    // 把 dirty bboxes 合并到 cap 个 slot 里
    let num_dirty = rects.rects.len();
    if num_dirty == 0 {
        r.num_rectangles = 0;
        return;
    }
    let bboxes_per_output = (num_dirty as f32 / cap as f32).max(1.0);
    for (i, bbox) in rects.rects.iter().enumerate() {
        let out_index = if num_dirty > cap {
            ((i as f32 / bboxes_per_output).round() as usize).min(cap - 1)
        } else {
            i
        };
        // expand 目标槽以包含此 bbox
        let slot = &mut slots[out_index];
        let mut rect = crate::util::rect::Rect::new(slot.x, slot.y, slot.width, slot.height);
        let bb = crate::util::rect::Rect::new(bbox.x, bbox.y, bbox.width, bbox.height);
        rect.expand_to_include_rect(&bb);
        slot.x = rect.x;
        slot.y = rect.y;
        slot.width = rect.width;
        slot.height = rect.height;
    }
    r.num_rectangles = num_dirty.min(cap) as c_int;
}

unsafe extern "C" fn fixed_destroy(surface: *mut MyPaintSurface) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() {
        return;
    }
    drop(Box::from_raw(s));
}

unsafe extern "C" fn fixed_save_png(
    surface: *mut MyPaintSurface,
    path: *const c_char,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
) {
    let s = surface as *mut CFixedTiledSurface;
    if s.is_null() || path.is_null() {
        return;
    }
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return,
    };
    use crate::surface::Surface as _;
    (*s).inner
        .save_png(std::path::Path::new(path_str), x, y, width, height);
}

// ============================================================================
// MyPaintTiledSurface FFI — 对应 mypaint-tiled-surface.h:73-77
// 这些函数都基于 CFixedTiledSurface（最常见的 tiled surface 实现）
// ============================================================================

/// 对应 mypaint_tile_request_init。注意 C 版用 MyPaintTileRequest 结构体；
/// 这里因为 Rust TileRequest 不暴露 buffer 字段（由 backend trait 提供），
/// 我们用一个 C ABI 兼容的简化版。
#[repr(C)]
pub struct MyPaintTileRequest {
    pub tx: c_int,
    pub ty: c_int,
    pub readonly: c_int,
    pub buffer: *mut u16,
    pub context: *mut c_void,
    pub thread_id: c_int,
    pub mipmap_level: c_int,
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_tile_request_init(
    data: *mut MyPaintTileRequest,
    level: c_int,
    tx: c_int,
    ty: c_int,
    readonly: c_int,
) {
    if data.is_null() {
        return;
    }
    let d = &mut *data;
    d.tx = tx;
    d.ty = ty;
    d.readonly = readonly;
    d.buffer = std::ptr::null_mut();
    d.context = std::ptr::null_mut();
    d.thread_id = -1;
    d.mipmap_level = level;
}

/// 对应 mypaint_tiled_surface_set_symmetry_state。
#[no_mangle]
pub unsafe extern "C" fn mypaint_tiled_surface_set_symmetry_state(
    self_: *mut CFixedTiledSurface,
    active: c_int,
    center_x: f32,
    center_y: f32,
    symmetry_angle: f32,
    symmetry_type: c_int,
    rot_symmetry_lines: c_int,
) {
    if self_.is_null() {
        return;
    }
    use crate::symmetry::SymmetryType;
    let sym_type = match symmetry_type {
        0 => SymmetryType::Vertical,
        1 => SymmetryType::Horizontal,
        2 => SymmetryType::VertHorz,
        3 => SymmetryType::Rotational,
        4 => SymmetryType::Snowflake,
        _ => return,
    };
    (*self_).inner.symmetry_data.set_pending(
        active != 0,
        center_x,
        center_y,
        symmetry_angle,
        sym_type,
        rot_symmetry_lines,
    );
}

/// 对应 mypaint_tiled_surface_get_alpha。
#[no_mangle]
pub unsafe extern "C" fn mypaint_tiled_surface_get_alpha(
    self_: *mut CFixedTiledSurface,
    x: f32,
    y: f32,
    radius: f32,
) -> f32 {
    if self_.is_null() {
        return 0.0;
    }
    use crate::surface::Surface as _;
    (*self_).inner.get_alpha(x, y, radius)
}

/// begin/end_atomic 同样基于 vtable。这里提供按 *CFixedTiledSurface 的直接调用。
#[no_mangle]
pub unsafe extern "C" fn mypaint_tiled_surface_begin_atomic(self_: *mut CFixedTiledSurface) {
    if self_.is_null() {
        return;
    }
    use crate::surface::Surface as _;
    (*self_).inner.begin_atomic();
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_tiled_surface_end_atomic(
    self_: *mut CFixedTiledSurface,
    roi: *mut MyPaintRectangles,
) {
    if self_.is_null() {
        return;
    }
    fixed_end_atomic(&mut (*self_).surface, roi);
}

// ============================================================================
// MyPaintBrushSettingInfo / InputInfo FFI — 对应 mypaint-brush-settings.h:37-67
// ============================================================================

#[repr(C)]
pub struct MyPaintBrushSettingInfo {
    pub cname: *const c_char,
    pub name: *const c_char,
    pub constant: c_int,
    pub min: f32,
    pub def: f32,
    pub max: f32,
    pub tooltip: *const c_char,
}

#[repr(C)]
pub struct MyPaintBrushInputInfo {
    pub cname: *const c_char,
    pub hard_min: f32,
    pub soft_min: f32,
    pub normal: f32,
    pub soft_max: f32,
    pub hard_max: f32,
    pub name: *const c_char,
    pub tooltip: *const c_char,
}

// 静态预生成的 CString 表，让 FFI 返回的 *const c_char 始终有效。
use std::sync::OnceLock;
static SETTING_CNAMES: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
static SETTING_NAMES: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
static SETTING_TOOLTIPS: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
static SETTING_INFOS: OnceLock<Vec<MyPaintBrushSettingInfo>> = OnceLock::new();

static INPUT_CNAMES: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
static INPUT_NAMES: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
static INPUT_TOOLTIPS: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
static INPUT_INFOS: OnceLock<Vec<MyPaintBrushInputInfo>> = OnceLock::new();

// 静态生命周期内的 CString ptr 跨线程读取安全
unsafe impl Send for MyPaintBrushSettingInfo {}
unsafe impl Sync for MyPaintBrushSettingInfo {}
unsafe impl Send for MyPaintBrushInputInfo {}
unsafe impl Sync for MyPaintBrushInputInfo {}

fn build_setting_infos() -> &'static [MyPaintBrushSettingInfo] {
    SETTING_INFOS.get_or_init(|| {
        let cnames = SETTING_CNAMES.get_or_init(|| {
            crate::SETTING_INFO
                .iter()
                .map(|s| std::ffi::CString::new(s.cname).unwrap())
                .collect()
        });
        let names = SETTING_NAMES.get_or_init(|| {
            crate::SETTING_INFO
                .iter()
                .map(|s| std::ffi::CString::new(s.name).unwrap())
                .collect()
        });
        let tips = SETTING_TOOLTIPS.get_or_init(|| {
            crate::SETTING_INFO
                .iter()
                .map(|s| std::ffi::CString::new(s.tooltip).unwrap())
                .collect()
        });
        crate::SETTING_INFO
            .iter()
            .enumerate()
            .map(|(i, s)| MyPaintBrushSettingInfo {
                cname: cnames[i].as_ptr(),
                name: names[i].as_ptr(),
                constant: if s.constant { 1 } else { 0 },
                min: s.min,
                def: s.def,
                max: s.max,
                tooltip: tips[i].as_ptr(),
            })
            .collect()
    })
}

fn build_input_infos() -> &'static [MyPaintBrushInputInfo] {
    INPUT_INFOS.get_or_init(|| {
        let cnames = INPUT_CNAMES.get_or_init(|| {
            crate::INPUT_INFO
                .iter()
                .map(|i| std::ffi::CString::new(i.cname).unwrap())
                .collect()
        });
        let names = INPUT_NAMES.get_or_init(|| {
            crate::INPUT_INFO
                .iter()
                .map(|i| std::ffi::CString::new(i.name).unwrap())
                .collect()
        });
        let tips = INPUT_TOOLTIPS.get_or_init(|| {
            crate::INPUT_INFO
                .iter()
                .map(|i| std::ffi::CString::new(i.tooltip).unwrap())
                .collect()
        });
        crate::INPUT_INFO
            .iter()
            .enumerate()
            .map(|(i, info)| MyPaintBrushInputInfo {
                cname: cnames[i].as_ptr(),
                hard_min: info.hard_min.unwrap_or(f32::NEG_INFINITY),
                soft_min: info.soft_min.unwrap_or(f32::NEG_INFINITY),
                normal: info.normal,
                soft_max: info.soft_max.unwrap_or(f32::INFINITY),
                hard_max: info.hard_max.unwrap_or(f32::INFINITY),
                name: names[i].as_ptr(),
                tooltip: tips[i].as_ptr(),
            })
            .collect()
    })
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_setting_info(id: c_int) -> *const MyPaintBrushSettingInfo {
    if id < 0 || id as usize >= crate::NUM_SETTINGS {
        return std::ptr::null();
    }
    &build_setting_infos()[id as usize]
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_setting_info_get_name(
    self_: *const MyPaintBrushSettingInfo,
) -> *const c_char {
    if self_.is_null() {
        return std::ptr::null();
    }
    (*self_).name
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_setting_info_get_tooltip(
    self_: *const MyPaintBrushSettingInfo,
) -> *const c_char {
    if self_.is_null() {
        return std::ptr::null();
    }
    (*self_).tooltip
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_input_info(id: c_int) -> *const MyPaintBrushInputInfo {
    if id < 0 || id as usize >= crate::NUM_INPUTS {
        return std::ptr::null();
    }
    &build_input_infos()[id as usize]
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_input_info_get_name(
    self_: *const MyPaintBrushInputInfo,
) -> *const c_char {
    if self_.is_null() {
        return std::ptr::null();
    }
    (*self_).name
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_brush_input_info_get_tooltip(
    self_: *const MyPaintBrushInputInfo,
) -> *const c_char {
    if self_.is_null() {
        return std::ptr::null();
    }
    (*self_).tooltip
}

// ============================================================================
// MyPaintMapping FFI — 对应 mypaint-mapping.h
// ============================================================================

use crate::mapping::Mapping;

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_new(inputs: c_int) -> *mut Mapping {
    let n = inputs.max(0) as usize;
    Box::into_raw(Box::new(Mapping::new(n)))
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_free(self_: *mut Mapping) {
    if !self_.is_null() {
        drop(Box::from_raw(self_));
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_get_base_value(self_: *mut Mapping) -> f32 {
    if self_.is_null() {
        return 0.0;
    }
    (*self_).get_base_value()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_set_base_value(self_: *mut Mapping, value: f32) {
    if self_.is_null() {
        return;
    }
    (*self_).set_base_value(value);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_set_n(self_: *mut Mapping, input: c_int, n: c_int) {
    if self_.is_null() || input < 0 || n < 0 {
        return;
    }
    (*self_).set_n(input as usize, n as usize);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_get_n(self_: *mut Mapping, input: c_int) -> c_int {
    if self_.is_null() || input < 0 {
        return 0;
    }
    (*self_).get_n(input as usize) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_set_point(
    self_: *mut Mapping,
    input: c_int,
    index: c_int,
    x: f32,
    y: f32,
) {
    if self_.is_null() || input < 0 || index < 0 {
        return;
    }
    (*self_).set_point(input as usize, index as usize, x, y);
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_get_point(
    self_: *mut Mapping,
    input: c_int,
    index: c_int,
    out_x: *mut f32,
    out_y: *mut f32,
) {
    if self_.is_null() || input < 0 || index < 0 {
        return;
    }
    let (x, y) = (*self_).get_point(input as usize, index as usize);
    if !out_x.is_null() {
        *out_x = x;
    }
    if !out_y.is_null() {
        *out_y = y;
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_is_constant(self_: *mut Mapping) -> c_int {
    if self_.is_null() {
        return 1;
    }
    if (*self_).is_constant() {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_get_inputs_used_n(self_: *mut Mapping) -> c_int {
    if self_.is_null() {
        return 0;
    }
    (*self_).inputs_used_n() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_calculate(self_: *mut Mapping, data: *const f32) -> f32 {
    if self_.is_null() || data.is_null() {
        return 0.0;
    }
    let n = crate::NUM_INPUTS;
    let slice = std::slice::from_raw_parts(data, n);
    (*self_).calculate(slice)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_mapping_calculate_single_input(
    self_: *mut Mapping,
    input: f32,
) -> f32 {
    if self_.is_null() {
        return 0.0;
    }
    (*self_).calculate_single(input)
}

// ============================================================================
// MyPaintMatrix / Transform FFI — 对应 mypaint-matrix.h
// ============================================================================

use crate::util::matrix::Transform;

#[repr(C)]
pub struct MyPaintTransform {
    pub xx: f32,
    pub xy: f32,
    pub x0: f32,
    pub yx: f32,
    pub yy: f32,
    pub y0: f32,
}

impl From<Transform> for MyPaintTransform {
    fn from(t: Transform) -> Self {
        Self {
            xx: t.xx,
            xy: t.xy,
            x0: t.x0,
            yx: t.yx,
            yy: t.yy,
            y0: t.y0,
        }
    }
}
impl From<MyPaintTransform> for Transform {
    fn from(t: MyPaintTransform) -> Self {
        Self {
            xx: t.xx,
            xy: t.xy,
            x0: t.x0,
            yx: t.yx,
            yy: t.yy,
            y0: t.y0,
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_transform_unit() -> MyPaintTransform {
    Transform::identity().into()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_transform_rotate_cw(
    t: *const MyPaintTransform,
    angle: f32,
) -> MyPaintTransform {
    if t.is_null() {
        return Transform::identity().into();
    }
    let base: Transform = MyPaintTransform {
        xx: (*t).xx,
        xy: (*t).xy,
        x0: (*t).x0,
        yx: (*t).yx,
        yy: (*t).yy,
        y0: (*t).y0,
    }
    .into();
    let c = angle.cos();
    let s = angle.sin();
    let rot = Transform {
        xx: c,
        xy: -s,
        yx: s,
        yy: c,
        x0: 0.0,
        y0: 0.0,
    };
    base.multiply(&rot).into()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_transform_rotate_ccw(
    t: *const MyPaintTransform,
    angle: f32,
) -> MyPaintTransform {
    mypaint_transform_rotate_cw(t, -angle)
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_transform_reflect(
    t: *const MyPaintTransform,
    angle: f32,
) -> MyPaintTransform {
    if t.is_null() {
        return Transform::identity().into();
    }
    let base: Transform = MyPaintTransform {
        xx: (*t).xx,
        xy: (*t).xy,
        x0: (*t).x0,
        yx: (*t).yx,
        yy: (*t).yy,
        y0: (*t).y0,
    }
    .into();
    // 沿过原点、与 x 轴成 angle 的轴反射
    let c = (2.0 * angle).cos();
    let s = (2.0 * angle).sin();
    let refl = Transform {
        xx: c,
        xy: s,
        yx: s,
        yy: -c,
        x0: 0.0,
        y0: 0.0,
    };
    base.multiply(&refl).into()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_transform_translate(
    t: *const MyPaintTransform,
    dx: f32,
    dy: f32,
) -> MyPaintTransform {
    if t.is_null() {
        return Transform::identity().into();
    }
    let mut base: Transform = MyPaintTransform {
        xx: (*t).xx,
        xy: (*t).xy,
        x0: (*t).x0,
        yx: (*t).yx,
        yy: (*t).yy,
        y0: (*t).y0,
    }
    .into();
    base.x0 += dx;
    base.y0 += dy;
    base.into()
}

#[no_mangle]
pub unsafe extern "C" fn mypaint_transform_point(
    t: *const MyPaintTransform,
    x: f32,
    y: f32,
    out_x: *mut f32,
    out_y: *mut f32,
) {
    if t.is_null() {
        return;
    }
    let base: Transform = MyPaintTransform {
        xx: (*t).xx,
        xy: (*t).xy,
        x0: (*t).x0,
        yx: (*t).yx,
        yy: (*t).yy,
        y0: (*t).y0,
    }
    .into();
    let (nx, ny) = base.transform_point(x, y);
    if !out_x.is_null() {
        *out_x = nx;
    }
    if !out_y.is_null() {
        *out_y = ny;
    }
}
