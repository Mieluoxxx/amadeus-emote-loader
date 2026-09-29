//! 部件显隐派生：把选择器取值展开成"哪些部件开、哪些关"的一组变量写入。
//!
//! 这是 `amadeus-next-emote/psb.mjs` 里 `partVariables()` 的移植。旧运行时不会自己按选择器
//! 推导部件显隐，消费方必须每帧把结果写回去。
//!
//! 消费方每帧都调的话不值得跨进程往返，所以 `bake` 会把这里的映射**展开成查表用的数据**，
//! 运行时只做一次区间查找 —— 语义仍然只有这一份实现。

use std::collections::BTreeMap;

use emote_psb::value::PsbValue;

use crate::normalize::{member_of, number_of, object_entries, string_of, truthy};

#[derive(Debug, Clone, PartialEq)]
pub enum PartError {
    /// 传入了不属于 `customSelectorList` 的标签。
    Unsupported { label: String },
    /// 取值没有命中恰好一个选项区间。
    Range { label: String, value: f64 },
}

impl PartError {
    /// 与实验室 i18n 的 `error.*` 词条一一对应。
    pub const fn key(&self) -> &'static str {
        match self {
            Self::Unsupported { .. } => "error.partControlUnsupported",
            Self::Range { .. } => "error.partControlRange",
        }
    }
}

impl std::fmt::Display for PartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { label } => write!(formatter, "{}：{label}", self.key()),
            Self::Range { label, value } => write!(formatter, "{}：{label}={value}", self.key()),
        }
    }
}

impl std::error::Error for PartError {}

/// 一个选择器的一条选项：区间 + 该选项命中的时候要写的变量。
#[derive(Debug, Clone)]
pub struct PartOption {
    pub lower: f64,
    pub upper: f64,
    /// 选项本身带标签时也要把这个标签写进去（JS 里 `if (option.label)`）。
    pub label: Option<String>,
    pub on_value: Option<f64>,
    pub off_value: Option<f64>,
}

/// 一个部件选择器的完整映射。
#[derive(Debug, Clone)]
pub struct PartSelector {
    pub label: String,
    /// 取值缺省时用的默认值（来自 `variableMetaInfoList`，找不到就是 0）。
    pub default_value: f64,
    pub options: Vec<PartOption>,
}

fn default_of(metadata: &PsbValue, label: &str) -> f64 {
    object_entries(member_of(metadata, "variableMetaInfoList").unwrap_or(&PsbValue::Null))
        .iter()
        .find(|variable| member_of(variable, "id").and_then(string_of).as_deref() == Some(label))
        .and_then(|variable| member_of(variable, "defaultValue"))
        .and_then(number_of)
        .unwrap_or(0.0)
}

/// 取出 `metadata.customSelectorList` 里启用的选择器。
pub fn part_selectors(metadata: &PsbValue) -> Vec<PartSelector> {
    object_entries(member_of(metadata, "customSelectorList").unwrap_or(&PsbValue::Null))
        .iter()
        .filter(|control| truthy(member_of(control, "enabled")))
        .filter_map(|control| {
            let label = member_of(control, "label").and_then(string_of)?;
            let options = object_entries(member_of(control, "optionList").unwrap_or(&PsbValue::Null))
                .iter()
                .filter_map(|option| {
                    let mut label = member_of(option, "label").and_then(string_of);
                    if label.as_deref() == Some("") {
                        label = None;
                    }
                    Some(PartOption {
                        lower: member_of(option, "lower").and_then(number_of)?,
                        upper: member_of(option, "upper").and_then(number_of)?,
                        label,
                        on_value: member_of(option, "onValue").and_then(number_of),
                        off_value: member_of(option, "offValue").and_then(number_of),
                    })
                })
                .collect();
            Some(PartSelector { default_value: default_of(metadata, &label), label, options })
        })
        .collect()
}

/// 按下选择器取值算出一组要写入的变量。`values` 里出现的标签必须是已知选择器。
pub fn part_variables(metadata: &PsbValue, values: &BTreeMap<String, f64>) -> Result<BTreeMap<String, f64>, PartError> {
    let selectors = part_selectors(metadata);
    for label in values.keys() {
        if !selectors.iter().any(|selector| &selector.label == label) {
            return Err(PartError::Unsupported { label: label.clone() });
        }
    }

    let mut result = BTreeMap::new();
    for selector in &selectors {
        let value = values.get(&selector.label).copied().unwrap_or(selector.default_value);
        let selected = selector
            .options
            .iter()
            .position(|option| value >= option.lower && value <= option.upper)
            .filter(|_| value.is_finite())
            .ok_or_else(|| PartError::Range { label: selector.label.clone(), value })?;
        // 恰好命中一个：区间若有重叠，JS 的 `selected.length !== 1` 也会报错。
        let hits = selector.options.iter().filter(|option| value >= option.lower && value <= option.upper).count();
        if hits != 1 {
            return Err(PartError::Range { label: selector.label.clone(), value });
        }
        result.insert(selector.label.clone(), value);
        for (index, option) in selector.options.iter().enumerate() {
            let Some(label) = &option.label else {
                continue;
            };
            let outcome = if index == selected { option.on_value } else { option.off_value };
            if let Some(outcome) = outcome {
                result.insert(label.clone(), outcome);
            }
        }
    }
    Ok(result)
}

/// 供消费方用的查表数据：每个选择器一条记录，选项里直接写好「选中它时要写的变量」。
///
/// 消费方每帧只需一次区间查找 + 一次合并，不必跨进程调用，语义仍由 `part_variables` 单点定义。
pub fn part_table(metadata: &PsbValue) -> serde_json::Value {
    let selectors: Vec<serde_json::Value> = part_selectors(metadata)
        .into_iter()
        .map(|selector| {
            let options: Vec<serde_json::Value> = (0..selector.options.len())
                .map(|selected| {
                    let option = &selector.options[selected];
                    let mut writes = serde_json::Map::new();
                    // 与 part_variables 一致：每个带标签的选项要么写 onValue，要么写 offValue。
                    for (index, other) in selector.options.iter().enumerate() {
                        let Some(label) = &other.label else {
                            continue;
                        };
                        let outcome = if index == selected { other.on_value } else { other.off_value };
                        if let Some(outcome) = outcome {
                            writes.insert(label.clone(), number_json(outcome));
                        }
                    }
                    serde_json::json!({
                        "lower": number_json(option.lower),
                        "upper": number_json(option.upper),
                        "writes": serde_json::Value::Object(writes),
                    })
                })
                .collect();
            serde_json::json!({
                "label": selector.label,
                "defaultValue": number_json(selector.default_value),
                "options": options,
            })
        })
        .collect();
    serde_json::Value::Array(selectors)
}

fn number_json(value: f64) -> serde_json::Value {
    if value.fract() == 0.0 {
        serde_json::Value::Number(serde_json::Number::from(value as i64))
    } else {
        serde_json::Number::from_f64(value).map_or(serde_json::Value::Null, serde_json::Value::Number)
    }
}
