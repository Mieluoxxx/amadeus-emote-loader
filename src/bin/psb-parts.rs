//! 部件显隐的对照工具：读模型 JSON 与一份选择器取值，输出要写入的变量（或错误 key）。
//!
//! ```bash
//! cargo run --release --bin psb-parts -- <model.json> <values.json> <out.json>
//! ```

use std::{collections::BTreeMap, env, fs, process::ExitCode};

use amadeus_emote_loader::{json_to_psb, model_metadata, part_variables};
use serde_json::Value;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (Some(model), Some(values), Some(output)) = (args.first(), args.get(1), args.get(2)) else {
        eprintln!("用法: psb-parts <model.json> <values.json> <out.json>");
        return ExitCode::FAILURE;
    };

    let model = match fs::read_to_string(model).map_err(|error| error.to_string()).and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string())) {
        Ok(json) => json,
        Err(error) => { eprintln!("读取模型失败：{error}"); return ExitCode::FAILURE; }
    };
    let parsed = json_to_psb(&model);
    let Some(metadata) = model_metadata(&parsed) else {
        eprintln!("模型里没有 metadata");
        return ExitCode::FAILURE;
    };

    let values: BTreeMap<String, f64> = match fs::read_to_string(values)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<BTreeMap<String, Value>>(&text).map_err(|error| error.to_string()))
    {
        Ok(map) => map.into_iter().filter_map(|(key, value)| Some((key, value.as_f64()?))).collect(),
        Err(error) => { eprintln!("读取取值失败：{error}"); return ExitCode::FAILURE; }
    };

    let result = match part_variables(metadata, &values) {
        Ok(variables) => serde_json::json!({ "result": Value::Object(variables.into_iter().map(|(key, value)| (key, number_json(value))).collect()) }),
        Err(error) => {
            let mut payload = serde_json::Map::new();
            payload.insert("error".into(), Value::String(error.key().into()));
            match &error {
                amadeus_emote_loader::PartError::Unsupported { label } => {
                    payload.insert("label".into(), Value::String(label.clone()));
                }
                amadeus_emote_loader::PartError::Range { label, value } => {
                    payload.insert("label".into(), Value::String(label.clone()));
                    payload.insert("value".into(), number_json(*value));
                }
            }
            Value::Object(payload)
        }
    };
    if let Err(error) = fs::write(output, serde_json::to_string(&result).expect("序列化")) {
        eprintln!("写入失败 {output}: {error}");
        return ExitCode::FAILURE;
    }
    println!("{output}: {}", serde_json::to_string(&result).expect("序列化").chars().take(120).collect::<String>());
    ExitCode::SUCCESS
}

fn number_json(value: f64) -> Value {
    if value.fract() == 0.0 {
        Value::Number(serde_json::Number::from(value as i64))
    } else {
        serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}
