//! 规范化对照工具：吃一份原始解包 JSON，跑兼容规范化（可选摘除离散轨道），写出结果。
//!
//! 与实验室的 JS 参照实现逐字段比对：
//!
//! ```bash
//! node tools/normalize-json.mjs            # 产出 raw / expect / expect.detached / snaps
//! cargo run --release --bin psb-normalize -- --detach --snaps snaps.json raw.json out.json
//! node tools/compare-json.mjs expect.detached.json out.json
//! ```

use std::{env, fs, process::ExitCode, time::Instant};

use amadeus_emote_loader::{detach_snap_tracks, json_to_psb, prepare_model, psb_to_json, sorted_object};
use serde_json::Value;

fn main() -> ExitCode {
    let mut detach = false;
    let mut snaps_path: Option<String> = None;
    let mut positional = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--detach" => detach = true,
            "--snaps" => snaps_path = args.next(),
            _ => positional.push(argument),
        }
    }
    let (Some(input), Some(output)) = (positional.first(), positional.get(1)) else {
        eprintln!("用法: psb-normalize [--detach] [--snaps <path>] <raw.json> <out.json>");
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let text = match fs::read_to_string(input) {
        Ok(text) => text,
        Err(error) => { eprintln!("读取失败 {input}: {error}"); return ExitCode::FAILURE; }
    };
    let json: Value = match serde_json::from_str(&text) {
        Ok(json) => json,
        Err(error) => { eprintln!("解析 JSON 失败: {error}"); return ExitCode::FAILURE; }
    };
    let parsed = started.elapsed();

    let mut model = json_to_psb(&json);
    let normalize_started = Instant::now();
    if let Err(error) = prepare_model(&mut model) {
        eprintln!("规范化失败 {}: {error}", error.key());
        return ExitCode::FAILURE;
    }
    let normalize = normalize_started.elapsed();

    let detach_started = Instant::now();
    let tracks = if detach { detach_snap_tracks(&mut model) } else { Vec::new() };
    let detach_took = detach_started.elapsed();
    let snapped: usize = tracks.iter().map(|(_, entries)| entries.len()).sum();

    let output_text = serde_json::to_string(&sorted_object(&psb_to_json(&model))).expect("序列化 JSON");
    if let Err(error) = fs::write(output, &output_text) {
        eprintln!("写入失败 {output}: {error}");
        return ExitCode::FAILURE;
    }

    if let Some(path) = snaps_path {
        // 与 JS 的 `Object.fromEntries(snaps)` 对齐：时间线标签 → [[轨道, 取值]…]
        let mut map = serde_json::Map::new();
        for (label, entries) in &tracks {
            let list = Value::Array(entries.iter().map(|(track, value)| Value::Array(vec![Value::String(track.clone()), value_json(*value)])).collect());
            map.insert(label.clone(), list);
        }
        let snaps_text = serde_json::to_string(&Value::Object(map)).expect("序列化 snaps");
        if let Err(error) = fs::write(&path, &snaps_text) {
            eprintln!("写入失败 {path}: {error}");
            return ExitCode::FAILURE;
        }
    }

    println!("输入={}B 输出={}B 解析={parsed:?} 规范化={normalize:?} 摘除={detach_took:?} 轨道={snapped} 合计={:?}",
        text.len(), output_text.len(), started.elapsed());
    ExitCode::SUCCESS
}

/// 取值一律当整数写回（轨道值本来就是离散的选择器序号）。
fn value_json(value: f64) -> Value {
    if value.fract() == 0.0 {
        Value::Number(serde_json::Number::from(value as i64))
    } else {
        serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}
