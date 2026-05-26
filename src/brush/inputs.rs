//! Public input parameter struct for [`crate::Brush::stroke_to`].

/// 单次 [`crate::Brush::stroke_to`] 调用的输入参数。
///
/// 对应 C `mypaint_brush_stroke_to` 的 11 个位置参数（除了 `surface`），
/// 改 struct 包装让 Rust caller 用字段名初始化更安全 + 配合 `Default`
/// 可省略缺省值字段。
///
/// FFI 层 `mypaint_brush_stroke_to` 仍接收 11 个 C 参数，shim 内部把
/// 它们装进 `StrokeInputs` 再调本 crate 的 [`crate::Brush::stroke_to`]。
///
/// # Default
///
/// 默认值适合"静态笔，无 tilt，100% zoom 视图"。仅 `viewzoom` 非 0
/// （= 1.0，避免除零）。其他字段都是 0 / false。
///
/// ```
/// use mypaint::StrokeInputs;
/// let inputs = StrokeInputs {
///     x: 100.0,
///     y: 50.0,
///     pressure: 0.8,
///     dtime: 0.016,
///     ..Default::default()
/// };
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeInputs {
    /// X 坐标（surface 空间，像素单位）。
    pub x: f32,
    /// Y 坐标（surface 空间，像素单位）。
    pub y: f32,
    /// 压力，0.0..=1.0。
    pub pressure: f32,
    /// X 方向倾斜，-1.0..=1.0（数位板 tilt）。
    pub xtilt: f32,
    /// Y 方向倾斜，-1.0..=1.0。
    pub ytilt: f32,
    /// 距离上次 `stroke_to` 的时间（秒）。
    pub dtime: f64,
    /// 视图缩放因子。常用 1.0（100%）。
    pub viewzoom: f32,
    /// 视图旋转角（弧度，未 normalize）。
    pub viewrotation: f32,
    /// 笔杆旋转（单位 turn，0..=1，后续 `* 360.0` 转度）。
    pub barrel_rotation: f32,
    /// 当前 surface 是否处于线性颜色空间。影响 color dynamics 的
    /// (de)linearize 步骤。
    pub linear: bool,
}

impl Default for StrokeInputs {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            pressure: 0.0,
            xtilt: 0.0,
            ytilt: 0.0,
            dtime: 0.0,
            viewzoom: 1.0, // 100% zoom — 避免下游除零
            viewrotation: 0.0,
            barrel_rotation: 0.0,
            linear: false,
        }
    }
}
