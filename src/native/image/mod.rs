//! 应用自有的 GIF 帧;NSH 使用其自带的 Image 显示静态 PNG。
// GIF 解码与画布合成
mod decode;
// GIF 的 LZW 解压
mod lzw;
// 图片来源的安全读取
mod source;
// 后台解码线程
mod worker;

#[cfg(test)]
mod fixtures;

use base64::{Engine, engine::general_purpose::STANDARD};
use nsh_core::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    // 互斥锁与一次性初始化
    sync::{Mutex, OnceLock},
};

/// 已编码的来源在分配或 GIF 块解析之前就受到限制。
// 编码后的来源在分配和解析前先限制为 32 MiB
const MAX_SOURCE: usize = 32 * 1024 * 1024;
/// 细长的图片同样有界;对正方形而言像素上限更严格。
// 单边最大像素；狭长图同样受限，方图另受像素总数约束
const MAX_SIDE: u16 = 4096;
const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_FRAMES: usize = 512;
/// 每个动画的全画布像素工作量;各帧按顺序编码并释放。
// 每个动画全画布像素工作量上限；帧按序编码并释放
const MAX_FRAME_PIXELS: usize = 128 * 1024 * 1024;
/// PNG、其 Base64 表示和公开的 JSON 都保持有界。
// PNG、其 Base64 与公开 JSON 的总大小上限
const MAX_JSON: usize = 24 * 1024 * 1024;
// 解码结果缓存的总字节上限
const MAX_CACHE: usize = 32 * 1024 * 1024;
const CACHE_ENTRIES: usize = 4;

/// 非动画来源的清单:没有帧,播放一次。
const STATIC_IMAGE: &str =
    "{\"animated\":false,\"width\":0,\"height\":0,\"plays\":1,\"frames\":[]}";

/// 一帧:PNG 数据 URI 与时长。
// 一帧：PNG 数据 URI 与时长
#[derive(Clone, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct PreparedFrame {
    /// 来源。
    pub source: String,
    /// 持续毫秒数。
    pub duration_ms: u32,
}

/// 解码后的动画清单;静态图或不可解码的来源为非动画且没有帧。
#[derive(Clone, Serialize, Deserialize, JsObject)]
pub struct Prepared {
    /// 多于一帧才算动画。
    pub animated: bool,
    /// 宽度。
    pub width: u16,
    /// 高度。
    pub height: u16,
    /// 播放次数,null 表示无限循环。
    pub plays: Option<u32>,
    /// 各帧。
    pub frames: Vec<PreparedFrame>,
}

// 缓存条目：原始字节与对应的 JSON
struct CacheEntry {
    input: Vec<u8>,
    json: String,
}

// 把 GIF 的每一帧编码为 PNG，生成清单 JSON
fn encode(bytes: &[u8]) -> Result<String, String> {
    let mut frames = Vec::new();
    // 已编码字节数估计，用于限额
    let mut encoded_bytes = 0;
    let animation = decode::decode_each(bytes, |pixels, duration_ms, width, height| {
        let mut png_bytes = Vec::new();
        {
            // 8 位 RGBA、快速压缩的 PNG
            let mut encoder =
                // 创建 PNG 编码器
                png::Encoder::new(&mut png_bytes, u32::from(width), u32::from(height));
            // 设置 RGBA 颜色
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            // 写文件头，失败则报错
            let mut writer = encoder.write_header().map_err(|_| "GIF帧PNG头编码失败")?;
            writer
                .write_image_data(pixels)
                .map_err(|_| "GIF帧PNG编码失败")?;
            // 结束编码，失败则报错
            writer.finish().map_err(|_| "GIF帧PNG编码结束失败")?;
        }
        // 估算 Base64 后的大小，超限即失败
        encoded_bytes += png_bytes.len().div_ceil(3) * 4 + 128;
        if encoded_bytes > MAX_JSON {
            return Err("GIF帧编码结果超过限额".into());
        }
        frames.push(PreparedFrame {
            // 以 data URL 形式保存 PNG
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

// 准备图片：命中缓存直接返回，否则解码并按容量写入缓存
fn prepare(source: &str) -> Result<String, String> {
    // 全局缓存，最近使用的排在队尾
    static CACHE: OnceLock<Mutex<VecDeque<CacheEntry>>> = OnceLock::new();
    // 在各应用引擎之间串行化有界解码;缓存使用方仍持有
    // 各自独立的 JS 时钟。精确的来源字节可避免缓存键冲突。
    // 跨应用引擎串行解码；缓存使用者各自持有独立的 JS 时钟，按完整字节比较避免键冲突
    let mut cache = CACHE
        // 首次使用时创建缓存
        .get_or_init(Mutex::default)
        // 加锁
        .lock()
        // 锁中毒时仍取回内部状态，不让一次 panic 连带拖垮其他线程
        .unwrap_or_else(|p| p.into_inner());
    let (bytes, gif_mime) = source::load(source)?;
    // 命中缓存：移到队尾并返回
    if let Some(index) = cache.iter().position(|entry| entry.input == bytes) {
        let entry = cache.remove(index).expect("matched GIF cache entry exists");
        let json = entry.json.clone();
        // 放回队尾，保持最近使用
        cache.push_back(entry);
        return Ok(json);
    }
    // 非 GIF：内联 GIF 声明但头部不对则报错，否则交给标准 Image 解码
    if !bytes.starts_with(b"GIF") {
        // 文件头声明是 GIF 却无效
        return if gif_mime {
            Err("GIF内联来源的文件头无效".into())
        // 否则分支
        } else {
            // 否则按非动画图处理
            Ok(STATIC_IMAGE.into())
        };
    }
    let json = encode(&bytes)?;
    // 总大小：输入加输出
    let size = bytes.len() + json.len();
    if size <= MAX_CACHE {
        // 从最旧的开始淘汰直到放得下
        while !cache.is_empty()
            // 并且缓存条目数已达上限
            && (cache.len() >= CACHE_ENTRIES
                || cache
                    .iter()
                    // 每项的输入与结果大小
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

// 声明 nsh 原生模块
#[module]
/// 准备有界且不可变的 PNG 帧,不改动原始图片。
// 准备有界、不可变的 PNG 帧，不改动原图
pub struct ImageFrames;

#[module]
impl ImageFrames {
    /// 准备动图帧:只有内联 GIF 和本地文件会被解码,其余来源(静态 PNG/JPEG 等)
    /// 返回非动图的空结果,由标准 Image 渲染。
    pub async fn prepare(source: String) -> Result<Prepared, String> {
        if !source.starts_with("file:///") && !source.starts_with("data:image/gif;base64,") {
            return Ok(Prepared {
                animated: false,
                width: 0,
                height: 0,
                plays: Some(1),
                frames: Vec::new(),
            });
        }
        let json = worker::prepare(source).await?;
        serde_json::from_str(&json).map_err(|_| "图片帧结果无效".to_owned())
    }
}

#[cfg(test)]
#[path = "image_test.rs"]
mod image_test;
