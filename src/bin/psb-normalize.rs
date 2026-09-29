//! 规范化对照工具：吃一份原始解包 JSON，跑兼容规范化，写出结果。
//!
//! 与实验室的 JS 参照实现（`tools/normalize-json.mjs` 输出的 `*.expect.json`）逐字段比对：
//!
//! ```bash
//! cargo run --release --bin psb-normalize -- raw.json out.json
//! node tools/compare-json.mjs expect.json out.json
//! ```

use std::{env, fs, process::ExitCode, time::Instant};

use amadeus_emote_loader::{json_to_psb, prepare_model, psb_to_json, sorted_object};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("用法: psb-normalize <raw.json> <out.json>");
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let text = match fs::read_to_string(input) {
        Ok(text) => text,
        Err(error) => { eprintln!("读取失败 {input}: {error}"); return ExitCode::FAILURE; }
    };
    let json: serde_json::Value = match serde_json::from_str(&text) {
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

    let serialize_started = Instant::now();
    let output_text = serde_json::to_string(&sorted_object(&psb_to_json(&model))).expect("序列化 JSON");
    if let Err(error) = fs::write(output, &output_text) {
        eprintln!("写入失败 {output}: {error}");
        return ExitCode::FAILURE;
    }

    println!("输入={}B 输出={}B 解析={parsed:?} 规范化={normalize:?} 序列化={:?} 合计={:?}",
        text.len(), output_text.len(), serialize_started.elapsed(), started.elapsed());
    ExitCode::SUCCESS
}
