# libmypaint (Rust)

[libmypaint](https://github.com/mypaint/libmypaint) 画笔引擎的 Rust 1:1 复刻。

逐行翻译自 C 源码（约 6364 行 → 4900+ 行 Rust），通过 dab 参数 trace 对照
在 62k+ 笔触上验证与 C 上游行为等价：4/5 个测试笔刷的 dab 数量精确一致，
所有结构化参数（aspect/angle/opaque/lock_alpha/colorize/posterize/paint/
alpha_eraser/color）bit-exact 完全相同，剩余字段仅有亚像素浮点累积误差
（<1e-3 像素）。

## Features

- 完整 brush 引擎：55 个 setting、18 个 input、44 个 state
- Tile-based surface（64×64 tile + `TileBackend` trait）
- RLE-encoded dab mask + 8 个 blend mode（Normal/Eraser/LockAlpha/Color/
  Posterize/Paint variants）
- 光谱颜色混合（10-bin spectral primaries + WGM）
- Knuth lagged-Fibonacci RNG（与 C 上游 `rng-double.c` 同算法）
- 5 种对称模式（Vertical/Horizontal/VertHorz/Rotational/Snowflake）
- Smudge buckets（256 + inline 回退 + min/max 跟踪）
- 完整 C ABI（`ffi` feature） — 可作为 `libmypaint.so` 的 drop-in 替换

## Quick Start

```rust
use mypaint::{Brush, BrushSetting, StrokeInputs, Surface};
use mypaint::surface::fixed::FixedTiledSurface;

let mut brush = Brush::new();
brush.from_defaults();
brush.set_base_value(BrushSetting::RadiusLogarithmic, 2.0);
brush.set_base_value(BrushSetting::ColorH, 0.0);  // 红色
brush.set_base_value(BrushSetting::ColorS, 1.0);
brush.set_base_value(BrushSetting::ColorV, 1.0);

let mut surface = FixedTiledSurface::new(256, 256);

surface.begin_atomic();
// 初始 reset stroke
brush.stroke_to(&mut *surface, &StrokeInputs {
    x: 100.0, y: 100.0, dtime: 0.01,
    ..Default::default()
});
// 画 stroke
for i in 0..30 {
    let x = 100.0 + i as f32 * 1.5;
    brush.stroke_to(&mut *surface, &StrokeInputs {
        x, y: 100.0, pressure: 1.0, dtime: 0.01,
        ..Default::default()
    });
}
let roi = surface.end_atomic();

surface.save_png(std::path::Path::new("stroke.png"), 0, 0, 256, 256);
```

详见 [`examples/basic_paint.rs`](examples/basic_paint.rs)。

## C FFI

启用 `ffi` feature 后，crate 暴露与 libmypaint 完全兼容的 C API：

```toml
[dependencies]
mypaint = { version = "0.1", features = ["ffi"] }
```

```bash
cargo build --release --features ffi
# 生成 target/release/liblibmypaint.so，可作为 libmypaint.so 替换
```

C 客户端用法与上游 libmypaint 完全相同：

```c
MyPaintBrush *b = mypaint_brush_new();
mypaint_brush_from_string(b, brush_json);

MyPaintFixedTiledSurface *s = mypaint_fixed_tiled_surface_new(512, 512);
MyPaintSurface *iface = mypaint_fixed_tiled_surface_interface(s);

mypaint_surface_begin_atomic(iface);
mypaint_brush_stroke_to(b, iface, x, y, pressure, 0, 0, dt, 1.0, 0, 0, 0);
MyPaintRectangle rects[1];
MyPaintRectangles roi = { 1, rects };
mypaint_surface_end_atomic(iface, &roi);

mypaint_brush_unref(b);
mypaint_surface_unref(iface);
```

## 验证

```bash
# 单元测试 + brush load + replay + tile_render + ffi (43 tests)
cargo test --release --features ffi

# C↔Rust dab 参数对照（需要先编译上游 c_trace）
cargo run --release --example rust_trace -- \
    tests/brushes/charcoal.myb tests/events/painting30sec.dat \
    > rust_trace.txt
diff rust_trace.txt c_trace.txt
```

## 实现说明

- **`src/brush/`** — Brush + state + settings + stroke_to (~1100 行核心算法)
- **`src/mapping/`** — Mapping 曲线插值
- **`src/render/`** — `color` (HSV/HSL/RGB), `blend` (RLE blends), `dab` (mask shape),
  `mask` (RLE encoding)
- **`src/smudge/`** — 光谱混合 + spectral primaries
- **`src/surface/`** — Surface trait, `TiledSurface` + `TileBackend`,
  `FixedTiledSurface`, `OperationQueue`
- **`src/symmetry/`** — 5 种对称变换矩阵生成
- **`src/util/`** — Knuth RNG, helpers, rect, matrix
- **`src/ffi/`** — feature-gated C ABI 兼容层
- **`build.rs`** — 从 `brushsettings.json` 生成 `BrushSetting`/`BrushInput`/
  `BrushState` enum + info 表

## 与 C 上游的有意差异

| 项目 | C 上游 | 本实现 | 原因 |
|------|--------|--------|------|
| `FixedTiledSurface` 初始填充 | `0xFFFF` (memset 255) | `0u16` (透明黑) | C 上游用了非法像素值（超过 SCALE=32768），blend 公式会溢出导致"空心"渲染。MyPaint 应用层不踩这个 bug 是因为载入时 layer 像素会 alpha-blend 覆盖整个 tile。|

`main` 分支在早期阶段提供过 `FixedTiledSurface::new_c_compat()`
用于 100% bit-exact 复刻 C 上游（含其 bug）；`refactor/idiomatic-rust`
分支已脱离"代码对代码 1:1 复刻"目标，仅保留功能对应 + FFI ABI 兼容，
该 API 已删除。

## 致谢

C 上游 libmypaint 由 Martin Renold、Jon Nordby 及 MyPaint 团队开发。本 crate
遵循同样的 ISC 许可证。

## License

ISC，与上游 libmypaint 一致。
