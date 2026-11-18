# 运行时矢量资源支持评估

日期：2026-11-18。本文件与 `out/` 下的实测数据、对比图、测评代码共同构成可复核材料。
复现方式见文末。

## 1. 现状盘点（资源统计）

统计口径：`find assets -type f`，全仓库（含 examples 引用扫描）。

| 类别 | 数量 | 说明 |
|---|---|---|
| 矢量（.svg） | 7 | 全部位于 `assets/branding/`，共 92,755 字节 |
| 位图 | 232 PNG / 8 JPG / 1 JPEG / 20 KTX2 / 2 HDR | 无 `@2x`/`@3x` 等多尺寸切图命名 |
| 矢量对应的 PNG 导出 | 5 | `icon`、`bevy_bird_dark`、`bevy_logo_dark`、`bevy_logo_light`、`bevy_logo_dark_big` 为同名 SVG 的手工导出版 |

关键事实：

- 运行时代码对品牌资源的加载共 98 处，**全部加载 PNG 导出版**（`icon.png` 70 处、`bevy_bird_dark.png` 12 处等）；唯一的 `.svg` 引用是 `bevy_solari` 文档注释里的 URL，不在运行时加载。
- 即当前模式为"SVG 母版 + 手工导出 PNG 双份维护"，正是矢量运行时支持要消除的对象。
- `embedded_asset!` 在 63 个文件中使用，但**不内联任何 .svg**；`include_str!` 引用的 SVG 仅为 `bevy_curve` 的文档插图，不进运行时包。当前不存在矢量资源被重复内联的问题。

## 2. 技术方案（设计结论）

- **解析器**：`resvg`/`usvg`/`tiny-skia`，纯 Rust，完全离线，不依赖任何在线转换服务。
- **加载器**：为 `.svg` 扩展名注册独立 `AssetLoader`，不影响现有位图加载路径；loader settings 携带目标显示尺寸与抗锯齿质量档位，按显示尺寸光栅化。
- **抗锯齿质量可选**：`Fast` = 按目标尺寸直接光栅化（tiny-skia 内置 AA）；`High` = 2 倍超采样光栅化后 bicubic 降采样。实测两者成本差 8–15 倍（见第 3 节），质量档位有真实意义。
- **颜色与透明度**：tiny-skia 输出预乘 RGBA，导入 `bevy_image::Image`（`Rgba8UnormSrgb`，非预乘）前需做 unpremultiply；颜色空间按 sRGB 处理。
- **错误处理**：解析/读取失败时 loader 返回 `Err`，由 `AssetServer` 记录并上报加载失败事件，不静默显示空白。实测见第 3 节。
- **缓存**：构建期复用 `AssetProcessor`，按资源粒度缓存到 `imported_assets`；运行期按 `(asset, 尺寸, 质量)` 键缓存光栅结果。
- **热更新**：复用 `bevy_asset` 的文件监听，SVG 变更触发重载重光栅化，无需重启客户端。
- **目录结构**：不变，SVG 与 PNG 共存于 `assets/`。
- **打包**：矢量资源作为普通资源经 asset processor 拷贝一份进包；不使用 `embedded_asset!`，避免二进制内联。代价项：静态链接 resvg 全链路使 release 二进制增大约 3.8 MB（未 strip 的实测上限）。

## 3. 实测数据

测评代码：`artifacts/vector-assessment/`（独立 crate，不进入 Bevy workspace）。
原始数据：`out/report.md`、`out/report.json`。

### 冷启动开销

- 全部 7 个 SVG 解析合计 **2.7 ms**（冷）。
- 最大文件（`bevy_logo_light_dark_and_dimmed.svg`，20 KB）在全新进程中端到端（读盘+解析+1x 光栅化）10 次测量：**1.11–1.31 ms**。
- 结论：对冷启动的影响可忽略。

### 光栅化耗时与质量成本（节选自 report.md）

| asset | parse (us) | fast 1x/2x/3x (us) | high 2x/3x (us) |
|---|---|---|---|
| bevy_logo_light_dark_and_dimmed.svg | 523 | 723/1373/2746 | 11114/22451 |
| icon.svg | 208 | 133/219/320 | 4123/8848 |

### 画质对比（同一份矢量，三种缩放）

以高质量（超采样）渲染为参考，边缘区域平均绝对误差（edge MAE，0–255）：

| asset | 位图方案（1x 光栅后放大）@2x | 位图方案 @3x | 矢量 fast @2x |
|---|---|---|---|
| bevy_logo_dark.svg | 44.0 | 50.7 | 5.8 |
| bevy_logo_light.svg | 37.7 | 46.9 | 5.6 |
| icon.svg | 26.5 | 32.3 | 3.7 |
| bevy_solari.svg | 20.5 | 24.8 | 2.7 |

对比图（3 倍缩放）：`out/icon_vector_3x.png`（矢量按尺寸光栅化，边缘锐利）vs
`out/icon_bitmap_upscaled_3x.png`（位图放大，边缘明显发虚），其余 6 个资源同目录成对存在。

### 错误处理断言

- 缺文件：`--expect-error ../../assets/branding/does_not_exist.svg` → `io_error: No such file or directory`，exit 0。
- 损坏文件（截断的 SVG）：→ `parse_error: SVG data parsing failed cause unexpected end of stream`，exit 0。
- 两种场景均产生明确错误而非静默空白。

## 4. 收益评估与投入建议

- 本仓库矢量资源仅 7 个（全部为品牌 logo），绝对收益规模小；但存在"SVG 母版 + 手工 PNG 导出"双份维护模式（5 个导出 PNG、98 处运行时加载），运行时矢量支持可消除该重复。
- 对矢量 UI 资源多、需多尺寸适配的下游项目，收益随资源数量线性增长（省切图、包体积由 3 套位图降为 1 份矢量）。
- 引擎侧成本：一个可选 feature 的独立 loader crate + 约 3.8 MB 二进制体积（未 strip）。
- 实测风险项全部可控：冷启动 < 3 ms、解析离线、错误可断言、缓存与热更新复用现有资产管线。
- **建议**：以可选 crate/feature 形式投入，默认构建不引入；本目录材料可作为评审依据。

## 5. 复现方式

```sh
cd artifacts/vector-assessment
cargo run --release                          # 全量测评，重新生成 out/report.md 与对比图
cargo run --release -- --cold <svg路径>      # 单文件冷启动测量（全新进程）
cargo run --release -- --expect-error <路径> # 错误处理断言，期望报错时 exit 0
```
