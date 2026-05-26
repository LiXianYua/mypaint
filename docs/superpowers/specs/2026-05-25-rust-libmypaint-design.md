# Rust libmypaint 设计文档

> 对 https://github.com/mypaint/libmypaint（约 6364 行 C）的 1:1 Rust 复刻。

## 概述

将 libmypaint 画笔引擎以惯式 Rust 完全重写，保持行为级等价（像素级输出一致）。目标是：

- 干净的 Rust API（trait、强类型 enum、颜色 newtype）
- 算法层逐行对照原版 C，确保可追溯
- 可选 FFI 层暴露 `extern "C"` 函数，与原版 ABI 兼容

不做的：GEGL 集成、GLib 兼容层（这两项在 Rust 无意义）。

## Crate 结构

```
rust-libmypaint/
├── Cargo.toml              # 主 crate，features: ["ffi"]
├── build.rs                # 解析 brushsettings.json 生成代码
├── brushsettings.json      # 从上游复制（设置定义唯一来源）
├── src/
│   ├── lib.rs              # 公共入口，re-export
│   ├── brush/              # 画笔引擎
│   │   ├── mod.rs          # Brush 结构体
│   │   ├── state.rs        # BrushState 强类型状态
│   │   ├── stroke.rs       # stroke_to 核心循环
│   │   ├── settings.rs     # 设置读写
│   │   └── json.rs         # brush_from_string / serde 序列化
│   ├── surface/            # 渲染表面
│   │   ├── mod.rs          # Surface trait
│   │   ├── tile.rs         # TileSurface + TileRequest
│   │   ├── fixed.rs        # FixedTiledSurface
│   │   └── operations.rs   # 操作队列（合并 operationqueue + tilemap + fifo）
│   ├── render/             # 渲染与混合
│   │   ├── mod.rs          # draw_dab 入口
│   │   ├── dab.rs          # 笔触形状计算
│   │   ├── blend.rs        # 混合模式（brushmodes.c）
│   │   └── color.rs        # HSV/HSL/RGB 转换 + posterize
│   ├── mapping/            # 映射曲线
│   │   └── mod.rs          # Mapping 结构
│   ├── symmetry/           # 对称绘画
│   │   └── mod.rs          # SymmetryState, SymmetryData, Transform
│   ├── smudge/             # 涂抹混合
│   │   └── mod.rs          # SmudgeBucket, 光谱混合
│   ├── util/               # 工具
│   │   ├── rng.rs          # 随机数（rng-double）
│   │   ├── rect.rs         # 矩形
│   │   ├── matrix.rs       # 矩阵
│   │   └── helpers.rs      # 辅助数学函数
│   └── ffi/                # C FFI（feature-gated）
│       └── mod.rs          # extern "C" 函数
```

## 核心类型

### 颜色类型

```rust
struct Hsva { h: f32, s: f32, v: f32 }
struct Rgba { r: f32, g: f32, b: f32, a: f32 }
struct Pixel16 { r: u16, g: u16, b: u16, a: u16 }  // tile 内部像素格式
```

### Brush 结构体

```rust
pub struct Brush {
    settings: Vec<Mapping>,
    state: BrushState,
    smudge_buckets: Vec<SmudgeBucket>,
    rng: RngDouble,
    stroke_total_painting_time: f64,
    stroke_current_idling_time: f64,
    brush_json: Option<serde_json::Value>,
}
```

- 不用手动 refcount——生命周期由调用者管理
- `BrushState` 是强类型 struct（44 个命名字段），非 float 数组

### Surface trait

替代原版 C vtable 结构体：

```rust
pub trait Surface {
    fn draw_dab(&mut self, params: DabParams) -> bool;
    fn get_color(&mut self, pos: (f32, f32), radius: f32, paint: f32) -> Rgba;
    fn begin_atomic(&mut self);
    fn end_atomic(&mut self) -> Rectangles;
    fn save_png(&mut self, path: &Path, roi: &Rect);
}
```

### DabParams

聚合原版 `draw_dab` 的 15 个独立参数：

```rust
struct DabParams {
    x: f32, y: f32, radius: f32,
    color: Rgba,
    opaque: f32, hardness: f32, softness: f32,
    alpha_eraser: f32, aspect_ratio: f32, angle: f32,
    lock_alpha: f32, colorize: f32, posterize: f32,
    posterize_num: f32, paint: f32,
}
```

## 核心算法结构

`stroke_to` 流程拆为三个方法：

| Rust 方法 | 原版 C 位置 | 职责 |
|-----------|------------|------|
| `stroke_to` | `mypaint_brush.c:1300-1547` | 参数校验 → skip → tracking → dab 循环 → stroke 分离 |
| `update_states` | `mypaint_brush.c:708-904` | 位置/压力/速度/方向滤波 → settings_value 计算 |
| `prepare_and_draw` | `mypaint_brush.c:1042-1250` | 透明度 → 颜色 → smudge → eraser → draw_dab |
| `count_dabs` | `mypaint_brush.c:1253-1287` | 计算 pending dab 数量 |
| `directional_offsets` | `mypaint_brush.c:586-664` | 方向偏移计算 |

关键原则：`stroke_to` / `update_states` / `prepare_and_draw` 的内部逻辑**逐行翻译**原版 C 代码，保持一一对应，方便调试和验证。

## 关键技术决策

### Mapping 曲线

和原版一致：`(x, y)` 控制点数组 + 线性插值。`Mapping::calculate` 对所有 input 求和。

### Tile 渲染线程

用 `std::sync::mpsc` 或 `crossbeam-channel` 做操作队列，不引入 async 框架。`MyPaintTileRequest` 的 `buffer` 用 `&mut [Pixel16]` 替代裸指针。

### brushsettings.json 代码生成

`build.rs` 用 `serde` 解析 JSON，生成：
- `enum BrushSetting`（55 个 variant，带 min/max/def/is_constant）
- `enum BrushInput`（18 个 variant）
- `enum BrushState`（44 个 variant，索引顺序固定不可改）
- 常量数组：setting info 表、input info 表

### FFI 层

- `#[cfg(feature = "ffi")]` 门控，默认不开
- `extern "C"` 函数签名与原版本一一对应
- `MyPaintSurfaceVTable`：C 兼容 vtable struct，让 C 代码可传自定义 surface
- 内部 Brush 用 `Box<Brush>` 转 `*mut c_void` 作为不透明句柄

### 依赖

- `serde` + `serde_json`：brush JSON 解析（运行时 + build.rs 共用）
- `crossbeam-channel`（可选）：tile 操作队列（如果 std::sync::mpsc 够用则不用）
- `png` crate：PNG 写入（替代原版 libpng 依赖）
- `thiserror`（可选）：错误类型定义
- 无 async 框架、无 GLib、无 GObject

## 测试策略

### 行为回放测试

- 直接使用上游 `tests/events/` 的事件录制文件
- 流程：加载 brush JSON → 回放事件 → 捕获 draw_dab 参数 → 与上游参考输出对比
- 用 `rstest` 做参数化测试

### 单元测试

- `mapping`：控制点插值（0/1/多点边界）
- `blend`：混合模式输入/输出验证
- `color`：HSV↔RGB 往返精度
- `smudge`：bucket 初始化、更新、边界
- `FFI`：extern "C" 函数不 segfault
