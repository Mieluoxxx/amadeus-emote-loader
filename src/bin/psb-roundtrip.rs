//! 写回验证：读 PSB → 用 `emote-psb` 重新写出 → 再读回 → 比对两棵树是否完全相同。
//! 阶段 1 要自己产出给运行时的字节，先确认「写」这一半可信。
//!
//! ```bash
//! cargo run --release --bin psb-roundtrip -- <input.psb> <output.psb>
//! ```

use std::{env, fs::File, io::{BufReader, Cursor, Read}, process::ExitCode, sync::Arc, time::Instant};

use emote_psb::{
    psb::{read::PsbFile, write::PsbWriter},
    value::PsbValue,
};

// 资源用 Arc<[u8]> 持有：写回时可以直接 clone 一个游标，不用把上百 MB 的贴图再拷一遍。
fn read_all(psb: &mut PsbFile<BufReader<File>>, count: usize, extra: bool) -> Result<Vec<Arc<[u8]>>, String> {
    let mut pool = Vec::with_capacity(count);
    for index in 0..count {
        let mut bytes = Vec::new();
        let stream = if extra { psb.open_extra_resource(index) } else { psb.open_resource(index) }.map_err(|error| error.to_string())?;
        if let Some(mut stream) = stream {
            stream.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
        }
        pool.push(Arc::from(bytes.into_boxed_slice()));
    }
    Ok(pool)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("用法: psb-roundtrip <input.psb> <output.psb>");
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let mut psb = match PsbFile::open(BufReader::new(File::open(input).expect("打开输入"))) {
        Ok(psb) => psb,
        Err(error) => { eprintln!("解析失败: {error}"); return ExitCode::FAILURE; }
    };
    let version = psb.version;
    let encrypted = psb.encrypted;
    let (resource_count, extra_count) = (psb.resources(), psb.extra_resources());
    let resources = read_all(&mut psb, resource_count, false).expect("读取资源");
    let extras = read_all(&mut psb, extra_count, true).expect("读取附加资源");
    let read_started = Instant::now();
    let root: PsbValue = match psb.deserialize_root() {
        Ok(root) => root,
        Err(error) => { eprintln!("读取根对象失败: {error}"); return ExitCode::FAILURE; }
    };
    let read = read_started.elapsed();
    let input_size = std::fs::metadata(input).map(|meta| meta.len()).unwrap_or_default();
    drop(psb);

    let write_started = Instant::now();
    let mut out = File::create(output).expect("创建输出");
    let mut writer = match PsbWriter::new(version, encrypted, &root, &mut out) {
        Ok(writer) => writer,
        Err(error) => { eprintln!("写入初始化失败: {error}"); return ExitCode::FAILURE; }
    };
    for bytes in &resources { writer.add_resource(Cursor::new(bytes.clone())).expect("写入资源"); }
    for bytes in &extras { writer.add_extra(Cursor::new(bytes.clone())).expect("写入附加资源"); }
    if let Err(error) = writer.finish() {
        eprintln!("写入失败: {error}");
        return ExitCode::FAILURE;
    }
    drop(out);
    let write = write_started.elapsed();

    let mut reread = PsbFile::open(BufReader::new(File::open(output).expect("重开输出"))).expect("重解析输出");
    let second: PsbValue = reread.deserialize_root().expect("重读根对象");
    let second_counts = (reread.resources(), reread.extra_resources());
    let second_resources = read_all(&mut reread, second_counts.0, false).expect("重读资源");
    let second_extras = read_all(&mut reread, second_counts.1, true).expect("重读附加资源");
    let output_size = std::fs::metadata(output).map(|meta| meta.len()).unwrap_or_default();
    let same_tree = root == second;
    let same_resources = second_resources == resources && second_extras == extras;

    println!("版本={version} 加密={encrypted} 资源={resource_count} 附加={extra_count}");
    println!("输入={input_size}B 输出={output_size}B 读={read:?} 写={write:?} 合计={:?}", started.elapsed());
    println!("树相同={same_tree} 资源字节相同={same_resources}");
    if !same_tree || !same_resources {
        eprintln!("回读的树或资源与原文件不一致");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
