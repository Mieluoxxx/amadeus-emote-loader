//! E-mote 解析器：把 PSB 二进制与 FreeMote 风格的 JSON 互相转换。
//!
//! 阶段 0 只需要「读」这一半：用 `emote-psb` 解码，再按 FreeMote 的约定还原成 JSON，
//! 好和磁盘上 `PsbDecompile` 的产物逐字段对照。

use emote_psb::value::{number::PsbNumber, PsbValue};
use serde_json::{Map, Number, Value};

pub mod normalize;
pub mod parts;
pub mod texture;
pub use normalize::{detach_snap_tracks, prepare_model, NormalizeError, ENTRY_LABEL};
pub use parts::{part_table, part_variables, part_selectors, PartError};
pub use texture::{decode_png, Texture, TextureError};

/// 资源引用在 FreeMote JSON 里的写法：`#resource#N` = 贴图资源，`#resource@N` = 附加资源。
/// 取模型树的 `metadata` 子树（消费方要的 `variableMetaInfoList` / `eyeControl` / `customSelectorList` 都在里面）。
pub fn model_metadata(model: &PsbValue) -> Option<&PsbValue> {
    match model {
        PsbValue::Object(entries) => entries.get("metadata"),
        _ => None,
    }
}

fn resource_string(value: &PsbValue) -> Option<String> {
    match value {
        PsbValue::Resource(index) => Some(format!("#resource#{index}")),
        PsbValue::ExtraResource(index) => Some(format!("#resource@{index}")),
        _ => None,
    }
}

/// PSB 树 → FreeMote 风格 JSON。
pub fn psb_to_json(value: &PsbValue) -> Value {
    match value {
        PsbValue::Null => Value::Null,
        PsbValue::Bool(flag) => Value::Bool(*flag),
        PsbValue::Number(PsbNumber::Integer(number)) => Value::Number(Number::from(*number)),
        PsbValue::Number(PsbNumber::Double(number)) => Number::from_f64(*number).map_or(Value::Null, Value::Number),
        PsbValue::Number(PsbNumber::Float(number)) => Number::from_f64(*number as f64).map_or(Value::Null, Value::Number),
        PsbValue::String(text) => Value::String(text.to_string()),
        PsbValue::Resource(_) | PsbValue::ExtraResource(_) => Value::String(resource_string(value).expect("资源引用")),
        PsbValue::List(items) => Value::Array(items.iter().map(psb_to_json).collect()),
        PsbValue::Object(entries) => {
            Value::Object(entries.iter().map(|(key, item)| (key.to_string(), psb_to_json(item))).collect())
        }
        // 编译期占位符（PSB 编译器指令）在实际模型数据里不出现；出现时保留标记而不是静默丢弃。
        intrinsic => Value::String(format!("__psb_intrinsic:{intrinsic:?}")),
    }
}

/// FreeMote 风格 JSON → PSB 树。资源引用字符串还原成 PSB 的资源类型。
pub fn json_to_psb(value: &Value) -> PsbValue {
    match value {
        Value::Null => PsbValue::Null,
        Value::Bool(flag) => PsbValue::Bool(*flag),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => PsbValue::Number(PsbNumber::Integer(integer)),
            None => PsbValue::Number(PsbNumber::Double(number.as_f64().unwrap_or_default())),
        },
        Value::String(text) => match parse_resource_string(text) {
            Some(resource) => resource,
            None => PsbValue::String(text.as_str().into()),
        },
        Value::Array(items) => PsbValue::List(items.iter().map(json_to_psb).collect()),
        Value::Object(entries) => PsbValue::Object(entries.iter().map(|(key, item)| (key.as_str().into(), json_to_psb(item))).collect()),
    }
}

fn parse_resource_string(text: &str) -> Option<PsbValue> {
    let rest = text.strip_prefix("#resource")?;
    let (kind, digits) = rest.split_at_checked(1)?;
    let index = digits.parse().ok()?;
    match kind {
        "#" => Some(PsbValue::Resource(index)),
        "@" => Some(PsbValue::ExtraResource(index)),
        _ => None,
    }
}

/// 只保留根对象用得到的键，便于对照时忽略顺序差异。
pub fn sorted_object(value: &Value) -> Value {
    match value {
        Value::Object(entries) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = entries.keys().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), sorted_object(&entries[key]));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted_object).collect()),
        other => other.clone(),
    }
}
