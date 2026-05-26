# 4 个 baseline stroke 形状

每个 stroke 是 PointerSample 序列 JSON，供 rust_libmypaint CPU 渲染 CLI + MyPaint 桌面手工对照参考使用。

| stroke | canvas | sample 数 | 设计意图 |
|---|---|---|---|
| straight_line | 800×200 | 60 | 测 spacing / dab 间距 / opaque_linearize（看是否离散） |
| s_curve | 800×200 | 70 | 测 direction-aware 旋转 + speed1/speed2 滤波 |
| circle | 400×400 | 36 | 测 stroke direction 滤波 + smudge 桶在回头点行为 |
| cross | 800×300 | 80 | 测交叉处 X 形 artifact（水彩 / 马克笔重叠混色） |

## JSON schema

每个 stroke 文件顶层结构：

```json
{
  "name": "stroke_name",
  "canvas_w": 800,
  "canvas_h": 200,
  "samples": [
    { "x": 50.0, "y": 100.0, "pressure": 0.0, "dtime": 0.000 },
    ...
  ]
}
```

各字段语义：

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | string | stroke 名称，与文件名一致 |
| `canvas_w` | integer | 画布宽度（像素） |
| `canvas_h` | integer | 画布高度（像素） |
| `samples[].x` | f32 | 落点 x 坐标（像素，左=0） |
| `samples[].y` | f32 | 落点 y 坐标（像素，上=0） |
| `samples[].pressure` | f32 | 笔压，范围 [0.0, 1.0] |
| `samples[].dtime` | f32 | 距上一个 sample 的时间间隔（**秒**，不是毫秒） |

## dtime 约定

- **第一个 sample 的 dtime 必须为 0.0**（标记 stroke 起始）
- 其余 sample 默认 dtime = 0.016（即 60 fps 匀速采样）
- dtime 单位是**秒**；0.016 s ≈ 16 ms

## pressure 曲线设计

| stroke | pressure 曲线 |
|---|---|
| straight_line | 线性斜坡：0 → 1.0（前半段），1.0 → 0.5（后半段） |
| s_curve | 三角形：起点 0.0，中点峰值 1.0，末点 0.5 |
| circle | 恒定 0.7（起点 pressure=0，后续全部 0.7） |
| cross | 两段各自三角形：起点 0.0 → 中点 1.0 → 末点 0.5 |

## stroke 形状

- **straight_line**：水平直线，x=50→750，y 恒定 100
- **s_curve**：正弦曲线，x=50→750，y = 100 + 50 × sin(8π × (x−50) / 700)，约 4 个完整波形
- **circle**：圆心 (200, 200)，半径 80，等角度 36 个 sample（每 10°），顺时针
- **cross**：两条对角线交叉（见下方说明）

## cross.json 多 stroke 说明

cross.json 包含两条独立 stroke（第一条对角线 + 第二条对角线）：

- **stroke 1**：(100, 50) → (700, 250)，40 samples
- **stroke 2**：(100, 250) → (700, 50)，40 samples

渲染端按 **dtime=0 重置 stroke 状态**识别新 stroke 起始。
即 `samples[40]`（stroke 2 的第一个 sample）的 dtime=0，表示笔离开画布后重新落笔。

两条线在画布中央交叉，形成 X 形，用于测试重叠区域的混色 / artifact 行为。
