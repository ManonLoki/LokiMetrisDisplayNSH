//! 应用自有的 GIF 帧;NSH 使用其自带的 Image 显示静态 PNG。
// GIF 解码与画布合成
mod decode;
// GIF 的 LZW 解压
mod lzw;
// 图片来源的安全读取
mod source;
// 后台解码线程
mod worker;

// 测试夹具
#[cfg(test)]
mod fixtures;

// 引入 base64
use base64::{Engine, engine::general_purpose::STANDARD};
// 引入 nsh 模块宏
use nsh_core::prelude::*;
// 引入序列化
use serde::{Deserialize, Serialize};
// 引入队列与同步原语
use std::{
    // 双端队列
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
// 像素总数上限
const MAX_PIXELS: usize = 16 * 1024 * 1024;
// 帧数上限
const MAX_FRAMES: usize = 512;
/// 每个动画的全画布像素工作量;各帧按顺序编码并释放。
// 每个动画全画布像素工作量上限；帧按序编码并释放
const MAX_FRAME_PIXELS: usize = 128 * 1024 * 1024;
/// PNG、其 Base64 表示和公开的 JSON 都保持有界。
// PNG、其 Base64 与公开 JSON 的总大小上限
const MAX_JSON: usize = 24 * 1024 * 1024;
// 解码结果缓存的总字节上限
const MAX_CACHE: usize = 32 * 1024 * 1024;
// 缓存条目数上限
const CACHE_ENTRIES: usize = 4;

/// 非动画来源的清单:没有帧,播放一次。
// 非动画结果
const STATIC_IMAGE: &str = "{\"animated\":false,\"width\":0,\"height\":0,\"plays\":1,\"frames\":[]}";

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
// 解码后的动画清单
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
    // 输入字节
    input: Vec<u8>,
    // 结果 JSON
    json: String,
}

// 把 GIF 的每一帧编码为 PNG，生成清单 JSON
fn encode(bytes: &[u8]) -> Result<String, String> {
    // 帧列表
    let mut frames = Vec::new();
    // 已编码字节数估计，用于限额
    let mut encoded_bytes = 0;
    // 逐帧解码并回调
    let animation = decode::decode_each(bytes, |pixels, duration_ms, width, height| {
        // PNG 缓冲
        let mut png_bytes = Vec::new();
        {
            // 8 位 RGBA、快速压缩的 PNG
            let mut encoder =
                // 创建 PNG 编码器
                png::Encoder::new(&mut png_bytes, u32::from(width), u32::from(height));
            // 设置 RGBA 颜色
            encoder.set_color(png::ColorType::Rgba);
            // 设置 8 位深度
            encoder.set_depth(png::BitDepth::Eight);
            // 使用快速压缩
            encoder.set_compression(png::Compression::Fast);
            // 写文件头，失败则报错
            let mut writer = encoder.write_header().map_err(|_| "GIF帧PNG头编码失败")?;
            // 写入像素
            writer
                // 写入像素数据
                .write_image_data(pixels)
                // PNG 编码失败
                .map_err(|_| "GIF帧PNG编码失败")?;
            // 结束编码，失败则报错
            writer.finish().map_err(|_| "GIF帧PNG编码结束失败")?;
        }
        // 估算 Base64 后的大小，超限即失败
        encoded_bytes += png_bytes.len().div_ceil(3) * 4 + 128;
        // 编码结果超限
        if encoded_bytes > MAX_JSON {
            // 报超出限额
            return Err("GIF帧编码结果超过限额".into());
        }
        // 加入一帧
        frames.push(PreparedFrame {
            // 以 data URL 形式保存 PNG
            source: format!("data:image/png;base64,{}", STANDARD.encode(png_bytes)),
            // 持续毫秒数
            duration_ms,
        });
        // 完成
        Ok(())
        // 出错即返回
    })?;
    // 构造清单
    let prepared = Prepared {
        // 多于一帧才算动画
        animated: frames.len() > 1,
        // 宽
        width: animation.width,
        // 高
        height: animation.height,
        // 循环次数
        plays: animation.plays,
        // 各帧
        frames,
    };
    // 序列化清单
    let json = serde_json::to_string(&prepared).map_err(|_| "GIF帧清单编码失败")?;
    // 超出限额
    if json.len() > MAX_JSON {
        // 报错
        return Err("GIF帧清单超过限额".into());
    }
    // 返回 JSON
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
        // 锁中毒则崩溃
        .expect("GIF frame cache poisoned");
    // 安全读取来源
    let (bytes, gif_mime) = source::load(source)?;
    // 命中缓存：移到队尾并返回
    if let Some(index) = cache.iter().position(|entry| entry.input == bytes) {
        // 取出命中的缓存项
        let entry = cache.remove(index).expect("matched GIF cache entry exists");
        // 复制其 JSON
        let json = entry.json.clone();
        // 放回队尾，保持最近使用
        cache.push_back(entry);
        // 返回 JSON
        return Ok(json);
    }
    // 非 GIF：内联 GIF 声明但头部不对则报错，否则交给标准 Image 解码
    if !bytes.starts_with(b"GIF") {
        // 文件头声明是 GIF 却无效
        return if gif_mime {
            // 报文件头无效
            Err("GIF内联来源的文件头无效".into())
        // 否则分支
        } else {
            // 否则按非动画图处理
            Ok(STATIC_IMAGE.into())
        };
    }
    // 编码为帧清单
    let json = encode(&bytes)?;
    // 总大小：输入加输出
    let size = bytes.len() + json.len();
    // 太大的结果不缓存
    if size <= MAX_CACHE {
        // 从最旧的开始淘汰直到放得下
        while !cache.is_empty()
            // 并且缓存条目数已达上限
            && (cache.len() >= CACHE_ENTRIES
                // 或者
                || cache
                    // 遍历
                    .iter()
                    // 每项的输入与结果大小
                    .map(|entry| entry.input.len() + entry.json.len())
                    // 求和
                    .sum::<usize>()
                    // 加上本次大小
                    + size
                    // 超过字节上限
                    > MAX_CACHE)
        {
            // 淘汰最旧的缓存项
            cache.pop_front();
        }
        // 放入新缓存项
        cache.push_back(CacheEntry {
            // 输入字节
            input: bytes,
            // 结果 JSON
            json: json.clone(),
        });
    }
    // 返回 JSON
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
            return Ok(Prepared { animated: false, width: 0, height: 0, plays: Some(1), frames: Vec::new() });
        }
        let json = worker::prepare(source).await?;
        serde_json::from_str(&json).map_err(|_| "图片帧结果无效".to_owned())
    }
}

#[cfg(test)]
#[path = "image_test.rs"]
mod image_test;
