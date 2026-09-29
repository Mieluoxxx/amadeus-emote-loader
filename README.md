# E-mote 数据层（暂用名 OpenEmoteSDK）

> **名字待定。** 这个仓库目前只做**数据层**：解外壳、解析、兼容规范化、编码、导出元数据。
> 渲染仍然完全依赖 M2 的闭源运行时（FreeMote-SDK 的 WebGL 构建，CC BY-NC-SA 二进制），
> 所以叫 "OpenEmoteSDK" 现在是**过度承诺**；更准确的说法见下面「它是什么／不是什么」。

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
| 1 | 兼容规范化（选择器索引化、`cw`→`opa`、`arm_type`、眼球镜像…） | ⬜ 未开始 |
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

**待定（需要仓库所有者决定）。** 建议自有代码用 `MIT OR Apache-2.0`：
本仓库依赖 `emote-psb`(MIT)，而消费者之一是 Apache-2.0 的宿主应用，
用 AGPL 会把许可约束传染给下游。在主人确定之前，此文件不声明具体条款。

## 用法

```bash
cargo build --release
cargo test

# 读 PSB → 导出 FreeMote 风格 JSON（含头部信息与耗时）
cargo run --release --bin psb-dump -- <输入.psb> <输出.json>

# 读 → 写 → 再读，校验树与资源字节
cargo run --release --bin psb-roundtrip -- <输入.psb> <输出.psb>
```

`psb-dump` 的输出与 FreeMote `PsbDecompile` 的 JSON 对齐（资源引用写作 `#resource#N` / `#resource@N`），
因此可以直接和磁盘上的解包产物做深比较。
