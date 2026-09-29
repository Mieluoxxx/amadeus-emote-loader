//! 部件显隐的单测：区间命中、on/off 写入、默认值、两类错误。

use std::collections::BTreeMap;

use amadeus_emote_loader::{json_to_psb, model_metadata, part_table, part_variables, PartError};
use serde_json::{json, Value};

fn metadata() -> Value {
    json!({
        "customSelectorList": [
            { "enabled": 1, "label": "arm_type", "optionList": [
                { "label": "arm_type_a", "lower": 0, "upper": 0, "onValue": 100, "offValue": 0 },
                { "label": "arm_type_b", "lower": 1, "upper": 1, "onValue": 100, "offValue": 0 }
            ] },
            { "enabled": 1, "label": "head_type", "optionList": [
                { "label": "fade_head_a", "lower": 0, "upper": 0, "onValue": 100, "offValue": 0 },
                { "label": "", "lower": 1, "upper": 1, "onValue": 0, "offValue": 0 }
            ] },
            { "enabled": 0, "label": "disabled_type", "optionList": [
                { "label": "x", "lower": 0, "upper": 0, "onValue": 1, "offValue": 0 }
            ] }
        ],
        "variableMetaInfoList": [ { "id": "arm_type", "defaultValue": 1 }, { "id": "head_type", "defaultValue": 0 } ]
    })
}

fn run(values: &[(&str, f64)]) -> Result<BTreeMap<String, f64>, PartError> {
    let model = json_to_psb(&json!({ "metadata": metadata() }));
    let values: BTreeMap<String, f64> = values.iter().map(|(key, value)| ((*key).to_string(), *value)).collect();
    part_variables(model_metadata(&model).expect("metadata"), &values)
}

#[test]
fn 命中区间时写_on_其余写_off() {
    let result = run(&[("arm_type", 1.0)]).expect("成功");
    assert_eq!(result.get("arm_type"), Some(&1.0), "选择器自身的取值也要写进去");
    assert_eq!(result.get("arm_type_a"), Some(&0.0), "B 档时 A 应该不可见");
    assert_eq!(result.get("arm_type_b"), Some(&100.0));
    let other = run(&[("arm_type", 0.0)]).expect("成功");
    assert_eq!(other.get("arm_type_a"), Some(&100.0));
    assert_eq!(other.get("arm_type_b"), Some(&0.0));
    assert_eq!(result.get("fade_head_a"), Some(&100.0), "head_type 用默认值 0 → 命中第一个选项");
}

#[test]
fn 空标签的选项不写入() {
    let result = run(&[("head_type", 1.0)]).expect("成功");
    assert!(!result.contains_key(""), "标签为空字符串的选项不能写进结果");
    assert_eq!(result.get("fade_head_a"), Some(&0.0), "未被选中的选项写 offValue");
}

#[test]
fn 缺省取值回落到默认值() {
    let result = run(&[]).expect("成功");
    assert_eq!(result.get("arm_type"), Some(&1.0), "arm_type 的默认值是 1");
}

#[test]
fn 未启用的选择器不参与() {
    assert_eq!(run(&[("disabled_type", 0.0)]), Err(PartError::Unsupported { label: "disabled_type".into() }));
}

#[test]
fn 未知标签报错() {
    assert_eq!(run(&[("没有这个", 0.0)]), Err(PartError::Unsupported { label: "没有这个".into() }));
}

#[test]
fn 取值越界报错() {
    assert_eq!(run(&[("arm_type", 2.0)]), Err(PartError::Range { label: "arm_type".into(), value: 2.0 }));
}

#[test]
fn 查表数据与直接计算等价() {
    let model = json_to_psb(&json!({ "metadata": metadata() }));
    let table = part_table(model_metadata(&model).expect("metadata"));
    let selectors = table.as_array().expect("数组");
    assert_eq!(selectors.len(), 2, "只导出启用的选择器");
    // 取 arm_type=1：查表拿到的写入应与 part_variables 一致（除选择器自身取值）。
    let arm = &selectors[0];
    assert_eq!(arm["defaultValue"], json!(1));
    let option = arm["options"].as_array().expect("数组").iter().find(|option| option["lower"] == json!(1)).expect("第二个选项");
    assert_eq!(option["writes"], json!({ "arm_type_a": 0, "arm_type_b": 100 }));
    let direct = run(&[("arm_type", 1.0)]).expect("成功");
    assert_eq!(direct.get("arm_type_a"), option["writes"]["arm_type_a"].as_f64().as_ref());
}
