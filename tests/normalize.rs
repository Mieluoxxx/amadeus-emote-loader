//! 规范化规则的单测：一份手写的小模型覆盖全部 5 条规则与 3 个错误分支。

use amadeus_emote_loader::{json_to_psb, prepare_model, psb_to_json, sorted_object, NormalizeError};
use serde_json::{json, Value};

fn normalize(input: Value) -> Result<Value, NormalizeError> {
    let mut model = json_to_psb(&input);
    prepare_model(&mut model)?;
    Ok(sorted_object(&psb_to_json(&model)))
}

/// 最小可用模型：一个带区间的选择器、一条被接管的口控、一条要改的时间线轨道、两帧要换算的 `cw`。
fn sample() -> Value {
    json!({
        "spec": "win",
        "source": { "tex#000": { "texture": { "type": "png" } } },
        "metadata": {
            "selectorControl": [{
                "enabled": 1,
                "label": "face_mouth",
                "optionList": [
                    { "label": "fade_mouth_0", "lower": 0, "upper": 9, "onValue": 0, "offValue": 1 },
                    { "label": "fade_mouth_1", "lower": 10, "upper": 19, "onValue": 0, "offValue": 1 }
                ]
            }],
            "mouthControl": [
                { "enabled": 1, "label": "face_mouth", "talkLabel": "face_talk" },
                { "enabled": 1, "label": "keep_me" }
            ],
            "timelineControl": [{
                "label": "表情_笑",
                "variableList": [
                    { "label": "face_mouth", "frameList": [
                        { "time": 0, "type": 2, "content": { "easing": 0, "value": 10 } },
                        { "time": 11, "type": 0, "content": null }
                    ] },
                    { "label": "head_LR", "frameList": [{ "time": 0, "type": 2, "content": { "easing": 0, "value": 100 } }] }
                ]
            }]
        },
        "object": { "all_parts": { "motion": { "全体構造": { "layer": [{
            "frameList": [{ "content": { "mask": 2147483648i64, "cw": 2155905024i64 } }],
            "children": [{ "frameList": [{ "content": { "opacity": 1, "mask": 2147483648i64, "cw": 2139062016i64 } }] }]
        }] } } } }
    })
}

#[test]
fn 选择器取值换成序号并删掉区间() {
    let out = normalize(sample()).expect("规范化成功");
    let selector = &out["metadata"]["selectorControl"][0];
    assert_eq!(selector["optionList"][0].get("lower"), None, "区间必须删掉");
    assert_eq!(selector["optionList"][1].get("upper"), None, "区间必须删掉");
    // 10 落在第二段区间 → 序号 1
    assert_eq!(out["metadata"]["timelineControl"][0]["variableList"][0]["frameList"][0]["content"]["value"], json!(1));
    // 选择器之外的轨道原样保留
    assert_eq!(out["metadata"]["timelineControl"][0]["variableList"][1]["frameList"][0]["content"]["value"], json!(100));
    // content 为 null 的帧不受影响
    assert_eq!(out["metadata"]["timelineControl"][0]["variableList"][0]["frameList"][1]["content"], Value::Null);
}

#[test]
fn 被接管的口控被丢掉其余保留() {
    let out = normalize(sample()).expect("规范化成功");
    let controls = out["metadata"]["mouthControl"].as_array().expect("数组");
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0]["label"], json!("keep_me"));
}

#[test]
fn cw_重量换算成_opa_并换掉隐藏位() {
    let out = normalize(sample()).expect("规范化成功");
    let layer = &out["object"]["all_parts"]["motion"]["全体構造"]["layer"][0];
    let first = &layer["frameList"][0]["content"];
    assert_eq!(first.get("cw"), None, "cw 必须删掉");
    assert_eq!(first["opa"], json!(0), "0x80808000 的低字节是 0，透明度应为 0");
    assert_eq!(first["mask"], json!(1024), "0x80000000 换成 0x400");
    let nested = &layer["children"][0]["frameList"][0]["content"];
    assert_eq!(nested["opa"], json!(0), "0x7f7f7f00 同样按中性处理");
    assert_eq!(nested["opacity"], json!(1), "无关字段保持原样");
}

#[test]
fn 非中性_cw_报错() {
    let mut model = json_to_psb(&sample());
    model = {
        let mut json = psb_to_json(&model);
        json["object"]["all_parts"]["motion"]["全体構造"]["layer"][0]["frameList"][0]["content"]["cw"] = json!(0x11223300i64);
        json_to_psb(&json)
    };
    assert_eq!(prepare_model(&mut model), Err(NormalizeError::CwColor));
}

#[test]
fn 选择器取值越界报错() {
    let mut json = sample();
    json["metadata"]["timelineControl"][0]["variableList"][0]["frameList"][0]["content"]["value"] = json!(99);
    let mut model = json_to_psb(&json);
    assert_eq!(prepare_model(&mut model), Err(NormalizeError::SelectorRange { label: "face_mouth".into(), value: 99.0 }));
}

#[test]
fn 缺少入口报错() {
    let mut json = sample();
    json["object"] = json!({ "all_parts": { "motion": {} } });
    let mut model = json_to_psb(&json);
    assert_eq!(prepare_model(&mut model), Err(NormalizeError::MissingEntry));
}

#[test]
fn 规格与贴图格式与基础入口() {
    let out = normalize(sample()).expect("规范化成功");
    assert_eq!(out["spec"], json!("ems"));
    assert_eq!(out["source"]["tex#000"]["texture"]["type"], json!("RGBA8"));
    assert_eq!(out["metadata"]["base"], json!({ "chara": "all_parts", "motion": "全体構造" }));
}

#[test]
fn 没有选择器时也能跑通() {
    let mut json = sample();
    json["metadata"]["selectorControl"] = json!([]);
    let out = normalize(json).expect("规范化成功");
    assert_eq!(out["metadata"]["mouthControl"].as_array().expect("数组").len(), 2, "没有区间选择器就不该丢口控");
    assert_eq!(out["metadata"]["timelineControl"][0]["variableList"][0]["frameList"][0]["content"]["value"], json!(10), "原值保留");
}
