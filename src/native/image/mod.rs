//! Application-owned GIF frames; NSH displays static PNGs using its stock Image.
mod decode;
mod lzw;
mod source;
mod worker;

#[cfg(test)]
mod fixtures;

use base64::{Engine, engine::general_purpose::STANDARD};
use nsh_core::prelude::*;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
};

/// Encoded sources are limited before allocation or GIF block parsing.
const MAX_SOURCE: usize = 32 * 1024 * 1024;
/// Long narrow images are bounded too; the pixel bound is stricter for squares.
const MAX_SIDE: u16 = 4096;
const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_FRAMES: usize = 512;
/// Full-canvas pixel work per animation; frames are encoded and released in order.
const MAX_FRAME_PIXELS: usize = 128 * 1024 * 1024;
/// PNGs, their Base64 representation and the public JSON remain bounded.
const MAX_JSON: usize = 24 * 1024 * 1024;
const MAX_CACHE: usize = 32 * 1024 * 1024;
const CACHE_ENTRIES: usize = 4;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedFrame {
    source: String,
    duration_ms: u32,
}

#[derive(Serialize)]
struct Prepared {
    animated: bool,
    width: u16,
    height: u16,
    plays: Option<u32>,
    frames: Vec<PreparedFrame>,
}

struct CacheEntry {
    input: Vec<u8>,
    json: String,
}

fn encode(bytes: &[u8]) -> Result<String, String> {
    let mut frames = Vec::new();
    let mut encoded_bytes = 0;
    let animation = decode::decode_each(bytes, |pixels, duration_ms, width, height| {
        let mut png_bytes = Vec::new();
        {
            let mut encoder =
                png::Encoder::new(&mut png_bytes, u32::from(width), u32::from(height));
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder.write_header().map_err(|_| "GIF帧PNG头编码失败")?;
            writer
                .write_image_data(pixels)
                .map_err(|_| "GIF帧PNG编码失败")?;
            writer.finish().map_err(|_| "GIF帧PNG编码结束失败")?;
        }
        encoded_bytes += png_bytes.len().div_ceil(3) * 4 + 128;
        if encoded_bytes > MAX_JSON {
            return Err("GIF帧编码结果超过限额".into());
        }
        frames.push(PreparedFrame {
            source: format!("data:image/png;base64,{}", STANDARD.encode(png_bytes)),
            duration_ms,
        });
        Ok(())
    })?;
    let prepared = Prepared {
        animated: frames.len() > 1,
        width: animation.width,
        height: animation.height,
        plays: animation.plays,
        frames,
    };
    let json = serde_json::to_string(&prepared).map_err(|_| "GIF帧清单编码失败")?;
    if json.len() > MAX_JSON {
        return Err("GIF帧清单超过限额".into());
    }
    Ok(json)
}

fn prepare(source: &str) -> Result<String, String> {
    static CACHE: OnceLock<Mutex<VecDeque<CacheEntry>>> = OnceLock::new();
    // Serialize bounded decodes across app engines; cache consumers still own
    // independent JS clocks. Exact source bytes prevent cache-key collisions.
    let mut cache = CACHE
        .get_or_init(Mutex::default)
        .lock()
        .expect("GIF frame cache poisoned");
    let (bytes, gif_mime) = source::load(source)?;
    if let Some(index) = cache.iter().position(|entry| entry.input == bytes) {
        let entry = cache.remove(index).expect("matched GIF cache entry exists");
        let json = entry.json.clone();
        cache.push_back(entry);
        return Ok(json);
    }
    if !bytes.starts_with(b"GIF") {
        return if gif_mime {
            Err("GIF内联来源的文件头无效".into())
        } else {
            Ok("{\"animated\":false}".into())
        };
    }
    let json = encode(&bytes)?;
    let size = bytes.len() + json.len();
    if size <= MAX_CACHE {
        while !cache.is_empty()
            && (cache.len() >= CACHE_ENTRIES
                || cache
                    .iter()
                    .map(|entry| entry.input.len() + entry.json.len())
                    .sum::<usize>()
                    + size
                    > MAX_CACHE)
        {
            cache.pop_front();
        }
        cache.push_back(CacheEntry {
            input: bytes,
            json: json.clone(),
        });
    }
    Ok(json)
}

#[module]
/// Prepares bounded, immutable PNG frames without changing the original image.
pub struct ImageFrames;

#[module]
impl ImageFrames {
    /// Returns a full-canvas frame manifest; malformed GIFs fail explicitly.
    pub async fn prepare(source: String) -> Result<String, String> {
        worker::prepare(source).await
    }
}

#[cfg(test)]
#[path = "image_test.rs"]
mod image_test;
