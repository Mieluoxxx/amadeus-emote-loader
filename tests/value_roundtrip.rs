//! 值转换的对称性：PSB 值 ↔ FreeMote 风格 JSON 必须可逆，尤其是资源引用字符串。

use emote_parser::{json_to_psb, psb_to_json};
use emote_psb::value::{number::PsbNumber, PsbValue};
use serde_json::json;

#[test]
fn 资源引用按约定转换() {
    assert_eq!(psb_to_json(&PsbValue::Resource(0)), json!("#resource#0"));
    assert_eq!(psb_to_json(&PsbValue::ExtraResource(123)), json!("#resource@123"));
    assert_eq!(json_to_psb(&json!("#resource#0")), PsbValue::Resource(0));
    assert_eq!(json_to_psb(&json!("#resource@123")), PsbValue::ExtraResource(123));
    // 普通字符串不能误判成资源引用。
    assert_eq!(json_to_psb(&json!("#resource")), PsbValue::String("#resource".into()));
    assert_eq!(json_to_psb(&json!("#resourceX1")), PsbValue::String("#resourceX1".into()));
}

#[test]
fn 数值与嵌套结构可逆() {
    let value = PsbValue::Object(
        [
            ("整型".into(), PsbValue::Number(PsbNumber::Integer(-2147483648))),
            ("双精度".into(), PsbValue::Number(PsbNumber::Double(0.1))),
            ("假".into(), PsbValue::Bool(false)),
            ("空".into(), PsbValue::Null),
            ("表".into(), PsbValue::List(vec![
                PsbValue::Number(PsbNumber::Integer(1)),
                PsbValue::String("文字".into()),
                PsbValue::Resource(2),
            ])),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(json_to_psb(&psb_to_json(&value)), value);
}

#[test]
fn 排序不改变内容() {
    let value = json!({ "b": [1, { "d": 2, "c": 3 }], "a": "x" });
    assert_eq!(emote_parser::sorted_object(&value), value);
}
