//! 导出 PSB 里的资源与附加资源，用于逐字节比对两份烘焙产物。
//!
//! ```bash
//! cargo run --release --bin psb-resources -- <input.psb> <输出目录>
//! ```

use std::{env, fs, io::{BufReader, Read}, path::Path, process::ExitCode};

use emote_psb::psb::read::PsbFile;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("用法: psb-resources <input.psb> <输出目录>");
        return ExitCode::FAILURE;
    };
    if let Err(error) = dump(input, Path::new(output)) {
        eprintln!("导出失败：{error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn dump(input: &str, output: &Path) -> Result<(), String> {
    fs::create_dir_all(output).map_err(|error| error.to_string())?;
    let mut psb = PsbFile::open(BufReader::new(fs::File::open(input).map_err(|error| error.to_string())?)).map_err(|error| error.to_string())?;
    let resources = psb.resources();
    let extras = psb.extra_resources();
    for index in 0..resources {
        let mut bytes = Vec::new();
        if let Some(mut stream) = psb.open_resource(index).map_err(|error| error.to_string())? {
            stream.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
        }
        fs::write(output.join(format!("resource-{index}.bin")), &bytes).map_err(|error| error.to_string())?;
    }
    for index in 0..extras {
        let mut bytes = Vec::new();
        if let Some(mut stream) = psb.open_extra_resource(index).map_err(|error| error.to_string())? {
            stream.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
        }
        fs::write(output.join(format!("extra-{index}.bin")), &bytes).map_err(|error| error.to_string())?;
    }
    println!("{input} → {}：资源 {resources} 项、附加 {extras} 项", output.display());
    Ok(())
}
