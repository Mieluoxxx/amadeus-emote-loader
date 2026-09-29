//! 贴图解码：PNG → 紧密排列的 RGBA8。
//!
//! 浏览器那条链路的做法是 `createImageBitmap` → canvas → `getImageData`，这里要等价地离线完成。
//! 注意 canvas 的中转是有损的：它内部按预乘存储，`getImageData` 再反乘回去，
//! 半透明像素会掉精度，alpha=0 的像素 RGB 还可能被抹成 0。原样解码 PNG 不会有这个问题。

use png::{BitDepth, ColorType, Decoder, Transformations};

#[derive(Debug)]
pub enum TextureError {
    /// PNG 解码失败。
    Png(String),
    /// 只支持 8 位通道的 PNG（现有素材都是 RGBA8）。
    Unsupported(String),
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(message) => write!(formatter, "PNG 解码失败：{message}"),
            Self::Unsupported(message) => write!(formatter, "不支持的贴图格式：{message}"),
        }
    }
}

impl std::error::Error for TextureError {}

/// 一张解码好的贴图：RGBA8，行间无填充。
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 解码 PNG 为 RGBA8。调色板、灰度、tRNS 都由 `Transformations` 统一展开。
pub fn decode_png(bytes: &[u8]) -> Result<Texture, TextureError> {
    let mut decoder = Decoder::new(std::io::Cursor::new(bytes));
    // EXPAND 展开调色板/灰度/tRNS，ALPHA 把没有 alpha 通道的补成不透明，之后一定是 RGBA。
    decoder.set_transformations(Transformations::EXPAND | Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|error| TextureError::Png(error.to_string()))?;
    let info = reader.info();
    if info.bit_depth != BitDepth::Eight {
        return Err(TextureError::Unsupported(format!("位深 {:?}", info.bit_depth)));
    }
    if info.color_type != ColorType::Rgba {
        return Err(TextureError::Unsupported(format!("颜色类型 {:?}", info.color_type)));
    }
    let (width, height) = (info.width, info.height);

    let mut buffer = vec![0u8; reader.output_buffer_size().ok_or_else(|| TextureError::Unsupported("输出缓冲尺寸未知".into()))?];
    let frame = reader.next_frame(&mut buffer).map_err(|error| TextureError::Png(error.to_string()))?;
    buffer.truncate(frame.buffer_size());
    if buffer.len() != (width as usize) * (height as usize) * 4 {
        return Err(TextureError::Unsupported(format!("RGBA 长度 {} 与 {width}x{height} 不符", buffer.len())));
    }
    Ok(Texture { width, height, rgba: buffer })
}
