//! 阶段 0 对照工具：读 PSB，导出 FreeMote 风格 JSON，并打印头部信息。
//!
//! ```bash
//! cargo run --release --bin psb-dump -- <input.psb> <output.json>
//! ```

use std::{env, fs::File, io::BufReader, process::ExitCode, time::Instant};

use emote_parser::{psb_to_json, sorted_object};
use emote_psb::{psb::read::PsbFile, value::PsbValue};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("用法: psb-dump <input.psb> <output.json>");
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let file = match File::open(input) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("打开失败 {input}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut psb = match PsbFile::open(BufReader::new(file)) {
        Ok(psb) => psb,
        Err(error) => {
            eprintln!("解析失败 {input}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let opened = started.elapsed();
    let read_started = Instant::now();
    let root: PsbValue = match psb.deserialize_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("读取根对象失败: {error}");
            return ExitCode::FAILURE;
        }
    };
    let read = read_started.elapsed();
    let json = sorted_object(&psb_to_json(&root));
    let text = serde_json::to_string(&json).expect("序列化 JSON");

    if let Err(error) = std::fs::write(output, &text) {
        eprintln!("写入失败 {output}: {error}");
        return ExitCode::FAILURE;
    }

    println!("版本={} 加密={} 名字={} 字符串={} 资源={} 附加资源={} 校验和={:?}",
        psb.version, psb.encrypted, psb.names.len(), psb.strings.len(), psb.resources(), psb.extra_resources(), psb.checksum);
    println!("PSB 头部={opened:?} 读根对象={read:?} 合计={:?} JSON={}B", started.elapsed(), text.len());
    ExitCode::SUCCESS
}
