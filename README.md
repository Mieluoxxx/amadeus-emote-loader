# amadeus-emote-loader

**E-mote（EMT/PSB）模型数据层。** 与 M2 Co., Ltd.、Project AZUSA 均无隶属关系；
E-mote 是 M2 的产品名，本项目只处理它的数据文件。

> 边界说明：本仓库**不含渲染**。渲染依赖 M2 的闭源运行时（FreeMote-SDK 的 WebGL 构建，
> CC BY-NC-SA 二进制），本仓库只负责把模型数据整理成它能直接吃的字节。
> 所以这里没有 “SDK”、没有引擎，也没有重写引擎的打算。

## 它是什么

一份 Rust 实现，把 E-mote 的模型数据整理成现成运行时能直接吃的字节：

```
游戏原包 (.psb.m: MZS 外壳 + MT19937 密钥流 + zstd)
        │  shell  ← 未实现，见路线图
        ▼
纯 PSB ──解析──> 模型树 ──兼容规范化──> 运行时能吃的 PSB
        │  psb (读/写)      normalize       │
        │                                   └─ metadata / snaps / 部件查表
        ▼
   交给渲染层（不是本仓库的职责）
```

入口是 `emote-loader bake`：吃一个解包好的模型目录，吐一份**可以直接 `loadData()`** 的 PSB，
外加一份随附数据（消费方契约见下文）。

## 它不是什么

- **不是渲染器**：不含网格变形、物理、蒙皮、stencil 遮罩。
- **不是引擎**：E-mote 引擎是 M2 的闭源产品，本仓库不重写它。
- **不是 E-mote 数据格式规范**：格式知识来自 FreeMote 与社区记录，不是官方文档。

## 快速开始

```bash
cargo build --release
cargo test                                    # 20 条断言

cargo run --release --bin emote-loader -- bake \
  --model /path/to/<模型目录> --out /tmp/model.psb
# → /tmp/model.psb       交给运行时 loadData
# → /tmp/model.meta.json metadata / snaps / 部件查表
```

模型目录的约定与 FreeMote `PsbDecompile` 的解包产物一致：目录名与文件主干同名，内含
`<名>.psb.m.json`、`<名>.psb.m.resx.json`，以及索引里指向的贴图与 `extra/*.bin`。

**谁在用它**：宿主应用在构建期把本仓库编译成随包分发的原生工具（`cargo build --release` →
`dist/bin/`），运行期由主进程按需烘焙、把缓存目录挂成只读路由，渲染端只取用成品。
不要求宿主使用 Rust：产物就是一份 PSB 加一份 JSON。

## 命令行工具

| 工具 | 用途 |
|---|---|
| `emote-loader bake --model <目录> --out <file.psb> [--meta <file.json>] [--snaps <file.json>]` | 烘焙：模型目录 → 成品 PSB（+ 随附数据） |
| `psb-dump <in.psb> <out.json>` | 读 PSB → FreeMote 风格 JSON，附头部信息与耗时 |
| `psb-roundtrip <in.psb> <out.psb>` | 读 → 写 → 再读，校验树与资源字节 |
| `psb-normalize [--detach] [--snaps <file>] <raw.json> <out.json>` | 只做规范化（不碰贴图），用于对照 |
| `psb-parts <model.json> <values.json> <out.json>` | 部件显隐求值：输出结果或错误 key |
| `psb-resources <in.psb> <目录>` | 导出资源与附加资源，供逐字节比对 |

## 库 API

```rust
use amadeus_emote_loader::{json_to_psb, model_metadata, prepare_model, detach_snap_tracks,
                           part_variables, part_table, decode_png, psb_to_json};

let mut model = json_to_psb(&raw_json);        // FreeMote 风格 JSON → PSB 值树
prepare_model(&mut model)?;                    // 兼容规范化（就地）
let snaps = detach_snap_tracks(&mut model);    // 摘除离散轨道，返回待调用方写入的值
let table = part_table(model_metadata(&model).unwrap());   // 部件查表数据
let texture = decode_png(&png_bytes)?;         // PNG → 紧密 RGBA8
```

| 导出 | 说明 |
|---|---|
| `prepare_model(&mut PsbValue)` | 兼容规范化的 5 条规则，顺序敏感，就地修改 |
| `detach_snap_tracks(&mut PsbValue)` | 摘掉表情里「单关键帧、第 0 帧」的选择器轨道；返回 `(时间线, [(轨道, 取值)…])` |
| `part_variables(&PsbValue, &BTreeMap<String, f64>)` | 选择器取值 → 要写入的变量（语义单点定义） |
| `part_table(&PsbValue)` | 把上面的映射展开成消费方查表数据 |
| `decode_png(&[u8])` | PNG → `Texture { width, height, rgba }`（8 位 RGBA） |
| `model_metadata(&PsbValue)` | 取 `metadata` 子树（变量表 / 眼球控制 / 选择器表） |
| `psb_to_json` / `json_to_psb` / `sorted_object` | 与 FreeMote JSON 互转、稳定排序 |
| `NormalizeError::key()` / `PartError::key()` | 返回 `error.selectorRange` 之类的 key，便于宿主做 i18n |

## 消费方契约

`bake` 产出两份文件，渲染端只需照做三件事。

### 1. 载入

```js
player.loadData(new Uint8Array(await (await fetch(`${model}.psb`)).arrayBuffer()))
const { metadata, snaps, parts } = await (await fetch(`${model}.meta.json`)).json()
for (const variable of metadata.variableMetaInfoList) {
  if (!variable.id.startsWith('fade_')) player.setVariable(variable.id, variable.defaultValue, 0)
}
```

### 2. 每帧写部件显隐

```js
// parts 的形状：[{ label, defaultValue, options: [{ lower, upper, writes: { 变量: 取值 } }] }]
const values = Object.fromEntries(metadata.customSelectorList.filter((c) => c.enabled)
  .map((c) => [c.label, player.getVariable(c.label)]))
for (const [label, value] of Object.entries(partVariablesFromTable(parts, values))) player.setVariable(label, value, 0)
for (const control of metadata.eyeControl) {           // 眼球参数要镜像到左右两只
  const value = player.getVariable(control.label)
  player.setVariable(control.varL, value); player.setVariable(control.varR, value)
}
```

查表求值的规则（与 `part_variables` 等价，任何一条不满足都要报错而不是猜）：

- 传入的标签必须在 `parts` 里，否则 `error.partControlUnsupported`；
- 取值必须**恰好命中一个**选项区间（`lower ≤ value ≤ upper`），否则 `error.partControlRange`；
- 缺省取值用该选择器的 `defaultValue`；
- 合并 `writes` 之后，还要把选择器自身的取值也写进去。

### 3. 切换表情后补写被抓掉的离散轨道

```js
// snaps 的形状：{ "表情_笑": [["face_mouth", 3], ["face_cheek", 0]] }
const timer = setTimeout(() => {
  for (const [name, value] of snaps[label] ?? []) player.setVariable(name, value, 0)
}, 175)   // 约为头身线性过渡（350ms）的一半
```

**为什么必须补写**：选择器是离散量，时间线里一帧就跳，而头身等连续量要线性走约 350ms。
不摘就会「脸先啪地变、头身还在移动」；摘了就必须由调用方在过渡中点补写。连续切换要取消
上一次未生效的写入。

### 宿主必须知道的运行时约束

- **必须设 `var Module = { TOTAL_MEMORY: 512 * 1024 * 1024 }`**。运行时默认堆只有 96 MiB，
  读 100 MB 级的模型会直接 `abort("Cannot enlarge memory arrays")`。这不是可选优化。
- 成品 PSB 体积 99.6% 是贴图原始 RGBA（4096px 图集），预期 130–220 MB 一份。
- 运行时**接受** PSB 的变长整数（`0x04|N`），实测与写死 int32/int64 渲染出的像素完全相同。

## 兼容规范化的 5 条规则

上游数据是 E-mote 3.03，运行时时序是 3.9，逐条补偿（顺序不可调换）：

| # | 规则 | 为什么 |
|---|---|---|
| 1 | 选择器取值 → 选项序号，然后删掉 `lower`/`upper` | 3.9 不认区间，只按选项序号取值 |
| 2 | 丢掉被选择器接管的遗留合并口控 | 否则它会覆盖 `face_mouth` |
| 3 | 中性 RGB 的打包 `cw` 权重 → 旧版 `opa`，mask 的隐藏位换成 `0x400` | 3.9 不认打包色权；非中性 RGB 直接报 `error.cwColor`，不猜 |
| 4 | `spec = "ems"`、`source.*.texture.type = "RGBA8"` | 运行时只认这两个 |
| 5 | 顶层合成入口改成 `all_parts / 全体構造`，写进 `metadata.base` | 顶层合成入口在 3.9 上渲染成黑色剪影 |

两条照搬 JS 语义、容易踩的细节：

- **选项表必须与 `optionList` 一一对应**：缺 `lower`/`upper` 的选项要占位，
  否则序号整体错位（上游是在完整选项表上 `findIndex`）。
- **真值判定里对象与列表恒为真**，空对象也是真；否则 `all_parts/全体構造` 会被误判成缺失。

## 实测结论

| 结论 | 数据 |
|---|---|
| 贴图路径上，浏览器链路**才是有损的** | canvas 预乘往返：alpha≥128 差 1 级、16 差 8 级、2 差 127 级，alpha=0 的像素 RGB 被抹成 0；本仓库直接解 PNG 时 **alpha 通道逐字节相同** |
| 端到端像素对照（同模型，JS 烘焙 vs 本仓库烘焙） | 抽样 9890 点仅 **1 点**不同（最大通道增量 3）；总亮度差 +0.008%；体积 −0.35% |
| 烘焙耗时 | 常服 C：132 MB / **367 ms**（贴图解码约 100 ms）；白衣 C：211 MB / 479 ms |
| 树的编码收益有限 | 变长整数让「树」小 37%，但贴图占体积 99.6%，整份只小 0.35% |
| 离散轨道规模 | 每个模型 44–56 条（表情时间线各 2–3 条） |

## 验收

本仓库的单测覆盖规则与边界（20 条）。**跨仓库的端到端验收在实验室仓库**
`amadeus-next-emote`，那里保留着当年那版 JS 参照实现作为对照物：

```bash
# 需要本地解包素材（git-ignored）
node tools/check-parser.mjs
```

四层对照，四个模型全过才算通过：

1. **结构往返**：读回的树 = 磁盘 JSON；写回后树与资源字节原样；
2. **规范化**：本仓库产物 = JS `prepareModel` 产物；
3. **烘焙**：成品 PSB 的树与 `snaps` = JS 参照；
4. **部件显隐**：`partVariables` 以及**消费方只用查表数据**求值，都要与 JS 逐样例一致。

渲染层面的验收看实验室的 `tools/psb-probe.html`（吃任意 PSB，输出元数据、像素统计与指纹）。

## 依赖

| 依赖 | 用途 | 许可 |
|---|---|---|
| [`emote-psb`](https://github.com/storycraft/emote-psb-rs) | PSB/MDF 的读写与值类型 | MIT |
| [`png`](https://github.com/image-rs/image-png) | 贴图解码（PNG → RGBA8） | MIT/Apache-2.0 |
| `serde_json` | 元数据与调试输出 | MIT/Apache-2.0 |
| `smol_str` | 键类型（必须与 `emote-psb` 内部版本一致） | MIT/Apache-2.0 |

本仓库**不含**任何 M2 或 FreeMote 的二进制；渲染层由消费者自己引入。

## 许可

本仓库自有代码用 **Apache-2.0**，正文见 [`LICENSE`](https://github.com/Mieluoxxx/amadeus-emote-loader/blob/main/LICENSE)（未改动的标准文本；
Apache-2.0 的正文里不含版权人字段，所以这里**没有**填任何署名 —— 如需显式署名，
另加 `NOTICE` 文件即可）。

依赖各自的许可见上表；本仓库不分发它们的二进制。消费方之一是与本仓库同为
Apache-2.0 的宿主应用，整条链路不再有非商业或 copyleft 的代码层。

## 路线图

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 | PSB 解析 / 写回 / 与 FreeMote JSON 对照 / 端到端像素验收 | ✅ |
| 1 | 兼容规范化、离散轨道摘除、贴图解码、`bake`、部件查表 | ✅ |
| 2 | MZS 外壳（`mzs\0` + zstd + MT19937 密钥流），直接吃游戏原包 | ⬜ **缺可验证的样本与密钥**，拿到样本再动（先有验收再写代码） |
| 3 | 编到 wasm32 + 浏览器侧绑定 | ⬜ 暂无消费方（产品走原生工具、实验室走 JS 参照实现） |

MZS 那一层已经查清：头部 `mzs\0` + 4 字节解压后长度，正文是 **zstd 压缩**（不是 MT19937）；
`-k/-l` 管的是另一层 —— MT19937 生成的重复密钥流逐字节 XOR（默认长度 131，跳过 8 字节头）。
另有一条已知坑：MT19937 的种子派生各实现不一致，必须拿已知密钥+样本与 FreeMote 输出逐字节对齐。

## 目录

```
src/lib.rs                 值转换（PSB ↔ FreeMote JSON）
src/normalize.rs           兼容规范化 + 离散轨道摘除
src/parts.rs               部件显隐（直接求值 + 查表导出）
src/texture.rs             PNG → RGBA8
src/main.rs                psb-dump
src/bin/                   emote-loader(bake) / psb-roundtrip / psb-normalize / psb-parts / psb-resources
tests/                     normalize / parts / texture / value_roundtrip
```
