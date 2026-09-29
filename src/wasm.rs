//! wasm32 入口：与浏览器之间只走一个约定好的字节协议，不引入 wasm-bindgen。
//!
//! 为什么手写 ABI：实验室页面必须保持「静态服务器 + 零构建」的性质，产物只有一个 `.wasm`
//! 加一个薄 shim。协议做成**零拷贝**：JS 把每段输入直接分配到 wasm 线性内存里，
//! `emote_bake` 接管这些缓冲的所有权，省掉上百 MB 的二次拷贝。
//!
//! ## 协议
//!
//! JS 侧：
//! 1. `emote_alloc(len)` 拿到一段内存，写入模型 JSON（UTF-8）；贴图 RGBA 与附加资源同理。
//! 2. 在另一段内存里写「表」：每段两个 u32（指针、长度），顺序是先 `resource_count` 段贴图、再附加资源。
//! 3. 调 `emote_bake(json_ptr, json_len, table_ptr, table_entries, resource_count)`。
//!    该调用**接管** json 与表中每段缓冲（内部释放），表本身仍归 JS 管。
//! 4. 读返回值：`[u32 totalLen][u32 headerLen][header JSON][psb bytes]`，用完调 `emote_free_output`。
//!
//! `header` 成功时是 `{"metadata":…,"snaps":…,"parts":…}`；失败时是 `{"error":"error.xxx",…}`，
//! 与实验室 `i18n.mjs` 的词条对应。

use std::io::Cursor;

use emote_psb::psb::write::PsbWriter;
use serde_json::Value;

use crate::{detach_snap_tracks, json_to_psb, meta_document, prepare_model};

/// 把 JS 分配的一段内存接过来，之后由 Rust 释放。
///
/// # Safety
/// `ptr`/`len` 必须来自同一次 `emote_alloc`，且调用后 JS 不能再碰它。
unsafe fn take(ptr: *mut u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    Vec::from_raw_parts(ptr, len, len)
}

/// 分配一段内存交给 JS 写入。
#[no_mangle]
pub extern "C" fn emote_alloc(len: u32) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len as usize);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// 释放 JS 自己分配、且没有交给 `emote_bake` 的缓冲（例如那张表）。
///
/// # Safety
/// 参数必须来自 `emote_alloc`，且未被 `emote_bake` 接管。
#[no_mangle]
pub unsafe extern "C" fn emote_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(ptr, len as usize, len as usize));
}

/// 释放 `emote_bake` 的输出缓冲。
///
/// # Safety
/// `ptr` 必须是 `emote_bake` 的返回值，且只释放一次。
#[no_mangle]
pub unsafe extern "C" fn emote_free_output(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    let head = std::slice::from_raw_parts(ptr, 4);
    let total = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
    drop(Vec::from_raw_parts(ptr, total, total));
}

/// 烘焙一份模型：模型 JSON + 贴图 + 附加资源 → 成品 PSB + 随附数据。
///
/// # Safety
/// 三个指针都必须来自 `emote_alloc`；调用成功后它们的释放责任转交给本函数。
#[no_mangle]
pub unsafe extern "C" fn emote_bake(json_ptr: *mut u8, json_len: u32, table_ptr: *const u32, table_entries: u32, resource_count: u32) -> *mut u8 {
    let json = take(json_ptr, json_len as usize);
    let table = std::slice::from_raw_parts(table_ptr, table_entries as usize * 2);
    let mut blobs: Vec<Vec<u8>> = Vec::with_capacity(table_entries as usize);
    for entry in table.chunks_exact(2) {
        blobs.push(take(entry[0] as *mut u8, entry[1] as usize));
    }

    let outcome = bake(&json, blobs, resource_count as usize);
    let (header, psb) = match outcome {
        Ok(pair) => pair,
        Err(error) => (error, Vec::new()),
    };
    package(&header, &psb)
}

fn bake(json: &[u8], blobs: Vec<Vec<u8>>, resource_count: usize) -> Result<(Value, Vec<u8>), Value> {
    let text = std::str::from_utf8(json).map_err(|error| failure("error.unsupportedData", &error.to_string()))?;
    let raw: Value = serde_json::from_str(text).map_err(|error| failure("error.unsupportedData", &error.to_string()))?;

    let mut model = json_to_psb(&raw);
    prepare_model(&mut model).map_err(|error| failure(error.key(), &error.to_string()))?;
    let tracks = detach_snap_tracks(&mut model);

    if resource_count > blobs.len() {
        return Err(failure("error.missingResource", &format!("贴图 {resource_count} 段，实际只有 {} 段", blobs.len())));
    }

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = PsbWriter::new(4, false, &model, &mut cursor).map_err(|error| failure("error.encodePsb", &error.to_string()))?;
    // 直接把缓冲交给写入器（所有权转移，不复制）：这是这个协议存在的意义。
    let mut pool = blobs.into_iter();
    for _ in 0..resource_count {
        writer.add_resource(Cursor::new(pool.next().unwrap_or_default())).map_err(|error| failure("error.encodePsb", &error.to_string()))?;
    }
    for blob in pool {
        writer.add_extra(Cursor::new(blob)).map_err(|error| failure("error.encodePsb", &error.to_string()))?;
    }
    writer.finish().map_err(|error| failure("error.encodePsb", &error.to_string()))?;
    let psb = cursor.into_inner();

    // 随附数据与原生 `bake --meta` 共用同一个构造函数。
    Ok((meta_document(&model, &tracks), psb))
}

fn failure(key: &str, detail: &str) -> Value {
    serde_json::json!({ "error": key, "detail": detail })
}

/// 打包成 `[u32 totalLen][u32 headerLen][header][psb]`，并交给 JS。
fn package(header: &Value, psb: &[u8]) -> *mut u8 {
    let text = header.to_string().into_bytes();
    let total = 8 + text.len() + psb.len();
    let mut out = Vec::<u8>::with_capacity(total);
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(&text);
    out.extend_from_slice(psb);
    let mut out = std::mem::ManuallyDrop::new(out);
    out.as_mut_ptr()
}
