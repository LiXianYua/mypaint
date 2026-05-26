//! FFI 烟雾测试：模拟 C 客户端用 Rust FFI 的典型流程。

#![cfg(feature = "ffi")]

use libmypaint::ffi::*;
use std::ffi::CString;
use std::os::raw::c_int;

#[test]
fn brush_lifecycle_with_refcounting() {
    unsafe {
        // new → ref → ref → unref → unref → unref（最后一次释放）
        let b = mypaint_brush_new();
        assert!(!b.is_null());

        mypaint_brush_ref(b);
        mypaint_brush_ref(b);
        // 此时 refcount = 3
        mypaint_brush_unref(b);
        mypaint_brush_unref(b);
        // 此时 refcount = 1，b 还没释放
        // 还能用
        mypaint_brush_from_defaults(b);
        mypaint_brush_unref(b);
        // refcount = 0 → 释放（不能再用了）
    }
}

#[test]
fn brush_setting_lookup_returns_negative_one_for_unknown() {
    unsafe {
        let name = CString::new("nonexistent_setting").unwrap();
        let id = mypaint_brush_setting_from_cname(name.as_ptr());
        assert_eq!(id, -1);
    }
}

#[test]
fn fixed_tiled_surface_round_trip() {
    unsafe {
        let surf = mypaint_fixed_tiled_surface_new(128, 128);
        assert!(!surf.is_null());

        assert_eq!(mypaint_fixed_tiled_surface_get_width(surf), 128);
        assert_eq!(mypaint_fixed_tiled_surface_get_height(surf), 128);

        let iface = mypaint_fixed_tiled_surface_interface(surf);
        assert!(!iface.is_null());

        // 通过 vtable 接口画一个 dab
        mypaint_surface_begin_atomic(iface);
        let drew = mypaint_surface_draw_dab(iface,
            64.0, 64.0, 10.0,
            1.0, 0.0, 0.0,  // 红色
            1.0, 0.8, 0.0,  // opaque, hardness, softness
            1.0, 1.0, 90.0, // alpha_eraser, aspect, angle
            0.0, 0.0, 0.0, 0.05, // lock_alpha, colorize, posterize, posterize_num
            0.0); // paint
        assert_eq!(drew, 1, "draw_dab should return TRUE");
        mypaint_surface_end_atomic(iface, std::ptr::null_mut());

        // 采样应该看到红色
        let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        mypaint_surface_get_color(iface, 64.0, 64.0, 3.0,
            &mut r, &mut g, &mut b, &mut a, 0.0);
        eprintln!("FFI get_color: r={r} g={g} b={b} a={a}");
        assert!(a > 0.01, "should have non-zero alpha after draw");
        assert!(r > g && r > b, "red should dominate");

        // 通过 destroy 释放
        fixed_destroy_from_iface(iface);
    }
}

// Helper: 调用 surface 的 destroy vfunc
unsafe fn fixed_destroy_from_iface(iface: *mut MyPaintSurface) {
    // 实际上 mypaint_surface_unref 会调用 destroy 当 refcount 到 0
    mypaint_surface_unref(iface);
}
