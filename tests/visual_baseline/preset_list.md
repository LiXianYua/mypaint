# Baseline preset 清单（12 个）

来源：[mypaint-brushes 上游](https://github.com/mypaint/mypaint-brushes) ISC license，commit `08da4a48a4b63f2e0f3303942d7c21da8479ea9f`

> 注：上游实际目录结构为 `brushes/{classic,experimental,ramon,deevad,kaerhon_v1,tanda,Dieterle}/`。
> 计划中的预设名称（如 `classic/round` 等）与上游结构略有差异。
> 下表列出的是实际映射后的上游路径及对应关系。

| 文件 | 上游路径 | 用途 | 映射说明 |
|---|---|---|---|
| round.myb | brushes/classic/rounded.myb | 基础圆笔 | rounded 是上游 classic 分类中的圆形笔刷 |
| watercolor.myb | brushes/deevad/watercolor_expressive.myb | 水彩（基础） | 使用 deevad 的 watercolor_expressive 作为基础水彩笔 |
| watercolor_glazing.myb | brushes/deevad/watercolor_glazing.myb | 水彩（湿润罩染风） | deevad 分类中的水彩罩染笔 |
| marker_2.myb | brushes/ramon/Marker.myb | 马克笔 | ramon 分类中的 Marker 笔刷 |
| pencil.myb | brushes/classic/pencil.myb | 铅笔 | 基础铅笔笔刷 |
| charcoal.myb | brushes/classic/charcoal.myb | 炭笔 | 基础炭笔笔刷 |
| ink_sketch.myb | brushes/classic/slow_ink.myb | 素描墨迹 | slow_ink 提供平稳的墨迹效果，用于素描演示 |
| oil.myb | brushes/tanda/oil-01-paint.myb | 油画效果 | tanda 分类中的油画笔，paint 版本用于涂绘 |
| eraser.myb | brushes/deevad/thin_hard_eraser.myb | 橡皮擦 | deevad 分类中的细硬橡皮擦 |
| airbrush.myb | brushes/deevad/airbrush.myb | 喷笔 | deevad 分类中的喷笔 |
| spray.myb | brushes/deevad/spray.myb | 喷枪 | deevad 分类中的喷枪效果 |
| smudge.myb | brushes/classic/smudge.myb | 涂抹 | 基础涂抹笔刷 |

## 变化说明

与计划中的 baseline preset 相比，实际映射情况：

1. **classic/round** → 找不到名称完全匹配的 `round.myb`，使用上游 `classic/rounded.myb`（功能完全等价）
2. **classic/watercolor** → 上游 classic 分类中没有水彩笔，改用 deevad 分类的 `watercolor_expressive.myb`（更专业）
3. **experimental/watercolor_glazing** → 实际位置在 `deevad/watercolor_glazing.myb`（不在 experimental）
4. **marker/marker_2** → 上游 ramon（而非 marker）分类有 `Marker.myb`
5. **experimental/ink_sketch** → 上游 experimental 没有 ink_sketch，改用 classic 分类的 `slow_ink.myb`（功能相近）
6. **experimental/oil** → 上游 experimental 没有油画，改用 tanda 分类的 `oil-01-paint.myb`
7. **classic/eraser** → 上游 classic 没有单独的 eraser，改用 deevad 的 `thin_hard_eraser.myb`
8. **classic/airbrush** → 实际位置在 `deevad/airbrush.myb`（不在 classic）
9. **classic/spray** → 实际位置在 `deevad/spray.myb`（不在 classic）

其余 5 个预设（pencil, charcoal, smudge 等）在上游 classic 分类中完全对应。

## 文件完整性

所有 12 个预设都已复制了对应的 `.myb` 文件和 `.png` preview 缩略图。
