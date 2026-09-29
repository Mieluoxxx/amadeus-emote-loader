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
        │  shell  ← 待实现
        ▼
纯 PSB ──解析──> 模型树 ──兼容规范化──> 运行时能吃的 PSB + 元数据
        │  psb (读/写)      normalize       │
        │                                   └─ metadata / snaps / partVariables
        ▼
   交给渲染层（不是本仓库的职责）
```

## 它不是什么

- **不是渲染器**：不含网格变形、物理、蒙皮、stencil 遮罩。
- **不是引擎**：E-mote 引擎是 M2 的闭源产品，本仓库不重写它。
- **不是 E-mote 编辑器数据格式规范**：格式知识来自 FreeMote 与社区记录，不是官方文档。

## 当前进度

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 | 读 PSB → 模型树；与 FreeMote 解包出的 JSON 逐字段对照 | ✅ 四个模型结构一致 |
| 0 | 写 PSB（`emote-psb` 的 `PsbWriter`）→ 回读比对 | ✅ 树与资源字节完全相同 |
| 0 | 端到端像素验收（132 MB 真实贴图模型，JS 编码 vs Rust 回写） | ✅ 像素指纹完全相同 |
| 1 | 兼容规范化（选择器索引化、`cw`→`opa`、`arm_type`、眼球镜像…） | ✅ 已移植 `prepareModel` 的 5 条规则，四个模型与 JS 参照实现逐字段一致；8 条单测 |
| 1 | 离散轨道摘除 + `snaps` 导出（`detachSnapTracks`） | ⬜ 未开始 |
| 1 | `bake` 子命令：模型目录 → 现成 PSB | ⬜ 未开始 |
| 2 | MZS 外壳（`mzs\0` + zstd + MT19937 密钥流） | ⬜ 未开始 |
| 3 | 编到 wasm32 + 贴图路径 | ⬜ 未开始 |

## 已确认的两条硬事实

1. **运行时接受 PSB 的变长整数**（`0x04|N`）。lab 的 JS 编码器一直写死 int32/int64，实测两者渲染出的像素**完全相同**，所以编码选择上可以更省。
   注意：模型体积 99.6% 是贴图原始 RGBA，树的这点差异在真实数据上只有 −0.37%。
2. **运行时的默认堆只有 96 MiB**，读 100 MB 级的模型会直接 `abort("Cannot enlarge memory arrays")`。
   宿主页面必须设 `var Module = { TOTAL_MEMORY: 512 * 1024 * 1024 }`，这是载荷性设置，不是可选优化。

## 依赖

| 依赖 | 用途 | 许可 |
|---|---|---|
| [`emote-psb`](https://github.com/storycraft/emote-psb-rs) | PSB/MDF 的读写与值类型 | MIT |
| `serde_json` | 元数据与调试输出 | MIT/Apache-2.0 |

本仓库**不含**任何 M2 或 FreeMote 的二进制；渲染层由消费者自己引入。

## 许可

自有代码用 **`MIT OR Apache-2.0`**（`Cargo.toml` 已声明）。本仓库依赖 `emote-psb`(MIT)，
消费方之一是 Apache-2.0 的宿主应用，所以没选 AGPL。

**待办：`LICENSE-MIT` / `LICENSE-APACHE` 正文还没补**（要仓库所有者确认条款后再落），
确认前不要对外发布这个 crate。

## 用法

```bash
cargo build --release
cargo test

# 读 PSB → 导出 FreeMote 风格 JSON（含头部信息与耗时）
cargo run --release --bin psb-dump -- <输入.psb> <输出.json>

# 读 → 写 → 再读，校验树与资源字节
cargo run --release --bin psb-roundtrip -- <输入.psb> <输出.psb>

# 原始解包 JSON → 兼容规范化 → JSON（与实验室的 JS 参照实现对照）
cargo run --release --bin psb-normalize -- <raw.json> <out.json>
```

`psb-dump` 的输出与 FreeMote `PsbDecompile` 的 JSON 对齐（资源引用写作 `#resource#N` / `#resource@N`），
因此可以直接和磁盘上的解包产物做深比较。
