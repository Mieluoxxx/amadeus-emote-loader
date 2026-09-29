//! 兼容规范化：把 E-mote 3.03 的数据整理成 FreeMote WebGL 3.9 运行时能吃的形状。
//!
//! 这里是 `amadeus-next-emote/psb.mjs` 里 `prepareModel()` 的逐条移植，规则顺序必须一致：
//!
//! 1. 选择器按选项序号工作（3.9 不认 `lower`/`upper` 区间）；
//! 2. 被选择器接管的那条遗留合并口控要丢掉，否则会覆盖 `face_mouth`；
//! 3. 中性 RGB 的打包 `cw` 权重换算成旧版 `opa`；
//! 4. 运行时只认 `ems` 规格与 `RGBA8` 贴图；
//! 5. 顶层合成入口在 3.9 上呈黑色剪影，改用模型自己的 `all_parts / 全体構造`。

use std::collections::HashMap;

use emote_psb::value::{number::PsbNumber, PsbValue};

/// 与实验室 `i18n.mjs` 的 `error.*` 词条一一对应，便于两边对同一份数据报同一个错。
#[derive(Debug, Clone, PartialEq)]
pub enum NormalizeError {
    /// 选择器取值落在所有选项区间之外。
    SelectorRange { label: String, value: f64 },
    /// `cw` 的色彩权重不是中性 RGB，需要完整的颜色混合实现才能近似。
    CwColor,
    /// 缺少 `all_parts / 全体構造` 入口。
    MissingEntry,
}

impl NormalizeError {
    /// 实验室 i18n 里的错误 key。
    pub const fn key(&self) -> &'static str {
        match self {
            Self::SelectorRange { .. } => "error.selectorRange",
            Self::CwColor => "error.cwColor",
            Self::MissingEntry => "error.missingEntry",
        }
    }
}

impl std::fmt::Display for NormalizeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelectorRange { label, value } => write!(formatter, "{}：{}={value} 超出选择器区间", self.key(), label),
            Self::CwColor => write!(formatter, "{}：暂不支持非中性 cw 颜色权重", self.key()),
            Self::MissingEntry => write!(formatter, "{}：缺少 all_parts/全体構造 入口", self.key()),
        }
    }
}

impl std::error::Error for NormalizeError {}

fn as_object(value: &PsbValue) -> Option<&HashMap<smol_alias::Key, PsbValue>> {
    match value {
        PsbValue::Object(entries) => Some(entries),
        _ => None,
    }
}

/// `HashMap<SmolStr, …>` 的键类型来自 emote-psb 的私有重导出，这里用别名占位。
mod smol_alias {
    pub type Key = smol_str::SmolStr;
}

fn object_mut(value: &mut PsbValue) -> Option<&mut HashMap<smol_alias::Key, PsbValue>> {
    match value {
        PsbValue::Object(entries) => Some(entries),
        _ => None,
    }
}

fn list_mut(value: &mut PsbValue) -> Option<&mut Vec<PsbValue>> {
    match value {
        PsbValue::List(items) => Some(items),
        _ => None,
    }
}

fn member<'a>(value: &'a PsbValue, key: &str) -> Option<&'a PsbValue> {
    as_object(value)?.get(key)
}

fn member_mut<'a>(value: &'a mut PsbValue, key: &str) -> Option<&'a mut PsbValue> {
    object_mut(value)?.get_mut(key)
}

fn number_of(value: &PsbValue) -> Option<f64> {
    match value {
        PsbValue::Number(PsbNumber::Integer(number)) => Some(*number as f64),
        PsbValue::Number(PsbNumber::Double(number)) => Some(*number),
        PsbValue::Number(PsbNumber::Float(number)) => Some(*number as f64),
        _ => None,
    }
}

/// JS 的真值语义：`false` / `0` / `""` / `null` / 缺失 都是假，**对象与列表恒为真**（哪怕空）。
fn truthy(value: Option<&PsbValue>) -> bool {
    match value {
        None | Some(PsbValue::Null) => false,
        Some(PsbValue::Bool(flag)) => *flag,
        Some(PsbValue::Number(PsbNumber::Integer(number))) => *number != 0,
        Some(PsbValue::Number(PsbNumber::Double(number))) => *number != 0.0,
        Some(PsbValue::Number(PsbNumber::Float(number))) => *number != 0.0,
        Some(PsbValue::String(text)) => !text.is_empty(),
        _ => true,
    }
}

fn object_entries(value: &PsbValue) -> &[PsbValue] {
    match value {
        PsbValue::List(items) => items,
        _ => &[],
    }
}

/// 一个待转换的选择器：标签 + 各选项的区间。
struct RangedSelector {
    label: String,
    /// 与 `optionList` 一一对应；缺 `lower`/`upper` 的选项占位为 `None`。
    /// 必须保持一一对应：JS 的 `findIndex` 是在完整选项表上找序号，漏掉一项就会整体错位。
    ranges: Vec<Option<(f64, f64)>>,
}

/// `enabled` 为真、且选项带 `lower` 的选择器。
fn ranged_selectors(metadata: &PsbValue) -> Vec<RangedSelector> {
    let Some(controls) = member(metadata, "selectorControl").map(object_entries) else {
        return Vec::new();
    };
    let mut selectors = Vec::new();
    for control in controls {
        if !truthy(member(control, "enabled")) {
            continue;
        }
        let Some(label) = member(control, "label").and_then(|value| match value {
            PsbValue::String(text) => Some(text.to_string()),
            _ => None,
        }) else {
            continue;
        };
        let options = member(control, "optionList").map(object_entries).unwrap_or_default();
        if !options.iter().any(|option| member(option, "lower").is_some()) {
            continue;
        }
        let ranges = options
            .iter()
            .map(|option| {
                let lower = member(option, "lower").and_then(number_of)?;
                let upper = member(option, "upper").and_then(number_of)?;
                Some((lower, upper))
            })
            .collect();
        selectors.push(RangedSelector { label, ranges });
    }
    selectors
}

/// 步骤 1：丢掉被选择器接管的遗留合并口控。
fn drop_legacy_mouth_control(metadata: &mut PsbValue, labels: &[String]) {
    let Some(controls) = member_mut(metadata, "mouthControl") else {
        return;
    };
    let Some(controls) = list_mut(controls) else {
        return;
    };
    controls.retain(|control| {
        let label = member(control, "label").and_then(|value| match value {
            PsbValue::String(text) => Some(text.as_str()),
            _ => None,
        });
        !label.is_some_and(|label| labels.iter().any(|candidate| candidate == label))
    });
}

/// 步骤 2：把时间线里的选择器取值换成选项序号，然后删掉 `lower`/`upper`。
fn convert_selector_values(metadata: &mut PsbValue, selectors: &[RangedSelector]) -> Result<(), NormalizeError> {
    let Some(timelines) = member_mut(metadata, "timelineControl") else {
        return Ok(());
    };
    let Some(timelines) = list_mut(timelines) else {
        return Ok(());
    };
    for selector in selectors {
        for timeline in timelines.iter_mut() {
            let Some(tracks) = member_mut(timeline, "variableList") else {
                continue;
            };
            let Some(tracks) = list_mut(tracks) else {
                continue;
            };
            for track in tracks.iter_mut() {
                let matches = member(track, "label").is_some_and(|value| match value {
                    PsbValue::String(text) => text.as_str() == selector.label,
                    _ => false,
                });
                if !matches {
                    continue;
                }
                let Some(frames) = member_mut(track, "frameList").and_then(list_mut) else {
                    continue;
                };
                for frame in frames.iter_mut() {
                    let Some(content) = member_mut(frame, "content") else {
                        continue;
                    };
                    if matches!(content, PsbValue::Null) {
                        continue;
                    }
                    let Some(value) = member(content, "value").and_then(number_of) else {
                        continue;
                    };
                    let index = selector
                        .ranges
                        .iter()
                        .position(|range| range.is_some_and(|(lower, upper)| value >= lower && value <= upper))
                        .ok_or_else(|| NormalizeError::SelectorRange { label: selector.label.clone(), value })?;
                    if let Some(slot) = member_mut(content, "value") {
                        *slot = PsbValue::Number(PsbNumber::Integer(index as i64));
                    }
                }
            }
        }
    }
    // 区间信息用完即删，旧运行时只认序号。
    let Some(controls) = member_mut(metadata, "selectorControl").and_then(list_mut) else {
        return Ok(());
    };
    for control in controls.iter_mut() {
        let is_ranged = member(control, "label").is_some_and(|value| match value {
            PsbValue::String(text) => selectors.iter().any(|selector| selector.label == text.as_str()),
            _ => false,
        });
        if !is_ranged {
            continue;
        }
        let Some(entries) = object_mut(control) else {
            continue;
        };
        let Some(PsbValue::List(options)) = entries.get_mut("optionList") else {
            continue;
        };
        for option in options.iter_mut() {
            if let Some(entries) = object_mut(option) {
                entries.remove("lower");
                entries.remove("upper");
            }
        }
    }
    Ok(())
}

/// 步骤 3：中性 RGB 的打包 `cw` 权重换算成旧版 `opa`，并把 mask 的隐藏位换成旧版标记。
fn convert_opacity(model: &mut PsbValue) -> Result<(), NormalizeError> {
    let Some(object) = member_mut(model, "object").and_then(object_mut) else {
        return Ok(());
    };
    for object_value in object.values_mut() {
        let Some(motions) = member_mut(object_value, "motion").and_then(object_mut) else {
            continue;
        };
        for motion in motions.values_mut() {
            let Some(layers) = member_mut(motion, "layer").and_then(list_mut) else {
                continue;
            };
            convert_layers(layers)?;
        }
    }
    Ok(())
}

fn convert_layers(layers: &mut [PsbValue]) -> Result<(), NormalizeError> {
    for layer in layers.iter_mut() {
        if let Some(frames) = member_mut(layer, "frameList").and_then(list_mut) {
            for frame in frames.iter_mut() {
                let Some(content) = member_mut(frame, "content") else {
                    continue;
                };
                if matches!(content, PsbValue::Null) {
                    continue;
                }
                let Some(entries) = object_mut(content) else {
                    continue;
                };
                // 只处理「打了隐藏位」的那些帧，其余保持原样。
                let Some(cw) = entries.get("cw").and_then(number_of) else {
                    continue;
                };
                let mask = entries.get("mask").and_then(number_of).unwrap_or_default();
                // JS 的位运算走 ToInt32/ToUint32，这里显式做同样宽度的截断。
                if (mask as i64 as i32) & i32::MIN == 0 {
                    continue;
                }
                if !matches!((cw as i64 as u32) >> 8, 0x80_80_80 | 0x7f_7f_7f) {
                    return Err(NormalizeError::CwColor);
                }
                let previous = entries.get("opa").and_then(number_of).unwrap_or(255.0);
                let alpha = (cw as i64 as u32) & 0xff;
                entries.insert("opa".into(), PsbValue::Number(PsbNumber::Integer((previous * alpha as f64 / 255.0).round() as i64)));
                let updated = (((mask as i64 as i32) & 0x7fff_ffff) as u32 | 0x400) as i64;
                entries.insert("mask".into(), PsbValue::Number(PsbNumber::Integer(updated)));
                entries.remove("cw");
            }
        }
        if let Some(children) = member_mut(layer, "children").and_then(list_mut) {
            convert_layers(children)?;
        }
    }
    Ok(())
}

/// 步骤 4：规格与贴图格式。
fn set_spec(model: &mut PsbValue) {
    if let Some(entries) = object_mut(model) {
        entries.insert("spec".into(), PsbValue::String("ems".into()));
    }
    let Some(sources) = member_mut(model, "source").and_then(object_mut) else {
        return;
    };
    for source in sources.values_mut() {
        if let Some(texture) = member_mut(source, "texture").and_then(object_mut) {
            texture.insert("type".into(), PsbValue::String("RGBA8".into()));
        }
    }
}

/// 步骤 5：换掉顶层合成入口，并记录基础入口。
fn set_base_entry(model: &mut PsbValue) -> Result<(), NormalizeError> {
    let entry = member(model, "object")
        .and_then(|value| member(value, "all_parts"))
        .and_then(|value| member(value, "motion"))
        .and_then(|value| member(value, ENTRY_LABEL));
    if !truthy(entry) {
        return Err(NormalizeError::MissingEntry);
    }
    let mut base = HashMap::new();
    base.insert("chara".into(), PsbValue::String("all_parts".into()));
    base.insert("motion".into(), PsbValue::String(ENTRY_LABEL.into()));
    if let Some(metadata) = member_mut(model, "metadata").and_then(object_mut) {
        metadata.insert("base".into(), PsbValue::Object(base));
    }
    Ok(())
}

/// 模型自己的完整结构入口；顶层合成入口在 3.9 上渲染成黑色剪影。
pub const ENTRY_LABEL: &str = "全体構造";

/// 按 `prepareModel()` 的顺序就地规范化整棵模型树。
pub fn prepare_model(model: &mut PsbValue) -> Result<(), NormalizeError> {
    let selectors = {
        let metadata = member(model, "metadata").ok_or(NormalizeError::MissingEntry)?;
        ranged_selectors(metadata)
    };
    let labels: Vec<String> = selectors.iter().map(|selector| selector.label.clone()).collect();
    if let Some(metadata) = member_mut(model, "metadata") {
        drop_legacy_mouth_control(metadata, &labels);
        convert_selector_values(metadata, &selectors)?;
    }
    convert_opacity(model)?;
    set_spec(model);
    set_base_entry(model)
}
