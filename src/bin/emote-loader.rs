//! 烘焙：模型目录 → 现成 PSB。
//!
//! ```bash
//! emote-loader bake --model <模型目录> --out <file.psb> [--snaps <file.json>]
//! ```
//!
//! 链路与实验室页面完全一致（原始 JSON → 兼容规范化 → 摘除离散轨道 → 内嵌贴图 → PSB v4），
//! 区别只是贴图在 Rust 侧解码、不再需要浏览器。

use std::{collections::BTreeMap, env, fs, path::{Path, PathBuf}, process::ExitCode, sync::Arc, time::Instant};

use amadeus_emote_loader::{decode_png, detach_snap_tracks, json_to_psb, meta_document, prepare_model};
use emote_psb::psb::write::PsbWriter;
use serde_json::Value;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprintln!("用法: emote-loader bake --model <模型目录> --out <file.psb> [--snaps <file.json>]");
        return ExitCode::FAILURE;
    };
    match command.as_str() {
        "bake" => bake(&args[1..]),
        other => {
            eprintln!("未知子命令 {other}（目前只有 bake）");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    model: PathBuf,
    out: PathBuf,
    snaps: Option<PathBuf>,
    /// 消费方要的随附数据：metadata / snaps / partVariables 查表。
    /// 省略时写在 `<out>.meta.json`（把 .psb 换成 .meta.json）。
    meta: Option<PathBuf>,
}

fn parse(args: &[String]) -> Option<Options> {
    let mut model = None;
    let mut out = None;
    let mut snaps = None;
    let mut meta = None;
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1);
        match args[index].as_str() {
            "--model" => model = value.map(PathBuf::from),
            "--out" => out = value.map(PathBuf::from),
            "--snaps" => snaps = value.map(PathBuf::from),
            "--meta" => meta = value.map(PathBuf::from),
            other => {
                eprintln!("未知参数 {other}");
                return None;
            }
        }
        index += 2;
    }
    let out = out?;
    let meta = meta.or_else(|| Some(out.with_extension("meta.json")));
    Some(Options { model: model?, out, snaps, meta })
}

/// 资源索引：`Resources` 的键是 `"0"`，`ExtraResources`/`ExtraFlattenArrays` 的键是 `"@0"`。
fn indexed(entries: &Value) -> Vec<(usize, &Value)> {
    let Some(object) = entries.as_object() else {
        return Vec::new();
    };
    let mut result: Vec<(usize, &Value)> = object
        .iter()
        .filter_map(|(key, value)| Some((key.trim_start_matches('@').parse().ok()?, value)))
        .collect();
    result.sort_by_key(|(index, _)| *index);
    result
}

/// 索引必须是 0..n 连续的，PSB 的资源表按下标引用。
fn dense_size(entries: &Value, what: &str) -> Result<usize, String> {
    let indexed = indexed(entries);
    for (position, (index, _)) in indexed.iter().enumerate() {
        if position != *index {
            return Err(format!("{what} 索引不连续：期望 {position}，实际 {index}"));
        }
    }
    Ok(indexed.len())
}

fn bake(args: &[String]) -> ExitCode {
    let Some(options) = parse(args) else {
        return ExitCode::FAILURE;
    };
    match bake_model(&options) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("烘焙失败：{error}");
            ExitCode::FAILURE
        }
    }
}

fn bake_model(options: &Options) -> Result<String, String> {
    let started = Instant::now();
    let name = options
        .model
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("模型目录名不可用：{}", options.model.display()))?
        .to_string();
    let model_json = fs::read_to_string(options.model.join(format!("{name}.psb.m.json"))).map_err(|error| error.to_string())?;
    let resx_text = fs::read_to_string(options.model.join(format!("{name}.psb.m.resx.json"))).map_err(|error| error.to_string())?;
    let read = started.elapsed();

    let mut model = json_to_psb(&serde_json::from_str(&model_json).map_err(|error| error.to_string())?);
    let resx: Value = serde_json::from_str(&resx_text).map_err(|error| error.to_string())?;
    if resx.get("PsbVersion").and_then(Value::as_u64) != Some(4) {
        return Err("只支持 PSB v4 资源索引".into());
    }

    let normalize_started = Instant::now();
    prepare_model(&mut model).map_err(|error| format!("{} {}", error.key(), error))?;
    let tracks = detach_snap_tracks(&mut model);
    let snapped: usize = tracks.iter().map(|(_, entries)| entries.len()).sum();
    let normalize = normalize_started.elapsed();

    // 附加资源：扁平浮点数组先拼成小端 f32，再与附加文件一起按下标排好。
    let textures_started = Instant::now();
    let full = |relative: &str| options.model.join(relative);
    // 两张表共用一套下标（扁平数组与附加文件混在一起），所以先按索引合并再检查连续性。
    let mut merged: BTreeMap<usize, Arc<[u8]>> = BTreeMap::new();
    for (index, values) in indexed(resx.get("ExtraFlattenArrays").unwrap_or(&Value::Null)) {
        let numbers = values.as_array().ok_or("ExtraFlattenArrays 的值必须是数组")?;
        let mut bytes = Vec::with_capacity(numbers.len() * 4);
        for number in numbers {
            bytes.extend_from_slice(&(number.as_f64().ok_or("扁平数组里出现非数值")? as f32).to_le_bytes());
        }
        merged.insert(index, Arc::from(bytes.into_boxed_slice()));
    }
    for (index, path) in indexed(resx.get("ExtraResources").unwrap_or(&Value::Null)) {
        let path = path.as_str().ok_or("ExtraResources 的值必须是路径")?;
        merged.insert(index, Arc::from(fs::read(full(path)).map_err(|error| format!("{path}: {error}"))?.into_boxed_slice()));
    }
    for (position, index) in merged.keys().enumerate() {
        if position != *index {
            return Err(format!("附加资源索引不连续：期望 {position}，实际 {index}"));
        }
    }
    let extra_count = merged.len();
    let extras: Vec<Arc<[u8]>> = merged.into_values().collect();

    // 贴图：PNG → RGBA8，索引与 `Resources` 的键一致。
    let resource_count = dense_size(resx.get("Resources").unwrap_or(&Value::Null), "Resources")?;
    let mut resources: Vec<Arc<[u8]>> = Vec::with_capacity(resource_count);
    let mut texture_pixels = 0usize;
    for (_, path) in indexed(resx.get("Resources").unwrap_or(&Value::Null)) {
        let path = path.as_str().ok_or("Resources 的值必须是路径")?;
        let bytes = fs::read(full(path)).map_err(|error| format!("{path}: {error}"))?;
        let texture = decode_png(&bytes).map_err(|error| format!("{path}: {error}"))?;
        texture_pixels += texture.rgba.len();
        resources.push(Arc::from(texture.rgba.into_boxed_slice()));
    }
    let textures = textures_started.elapsed();

    let write_started = Instant::now();
    let file = fs::File::create(&options.out).map_err(|error| error.to_string())?;
    let mut writer = PsbWriter::new(4, false, &model, file).map_err(|error| error.to_string())?;
    for bytes in &resources {
        writer.add_resource(std::io::Cursor::new(bytes.clone())).map_err(|error| error.to_string())?;
    }
    for bytes in &extras {
        writer.add_extra(std::io::Cursor::new(bytes.clone())).map_err(|error| error.to_string())?;
    }
    writer.finish().map_err(|error| error.to_string())?;
    let write = write_started.elapsed();

    // 消费方随附数据：与 wasm 入口共用同一个构造函数。
    if let Some(path) = &options.meta {
        let payload = meta_document(&model, &tracks);
        fs::write(path, serde_json::to_string(&payload).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    }

    if let Some(path) = &options.snaps {
        let map: BTreeMap<&String, Vec<Value>> = tracks
            .iter()
            .map(|(label, entries)| {
                (label, entries.iter().map(|(track, value)| Value::Array(vec![Value::String(track.clone()), Value::Number(serde_json::Number::from(*value as i64))])).collect())
            })
            .collect();
        fs::write(path, serde_json::to_string(&map).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    }

    let size = fs::metadata(&options.out).map(|meta| meta.len()).unwrap_or_default();
    Ok(format!(
        "{}：{size}B（贴图 {resource_count} 张 / {} 像素，附加 {extra_count} 项，摘除 {snapped} 条轨道）\n  读取={read:?} 规范化={normalize:?} 贴图={textures:?} 写出={write:?} 合计={:?}",
        options.out.display(),
        texture_pixels,
        started.elapsed()
    ))
}

/// 供未来子命令复用的路径检查。
#[allow(dead_code)]
fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.is_dir() { Ok(()) } else { Err(format!("不是目录：{}", path.display())) }
}
