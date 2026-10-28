// 引入被测模块
use super::*;
use serde_json::Value;
use std::{fs, io::Cursor};

// 同步地驱动异步准备流程，返回清单 JSON
fn prepared(source: String) -> Result<String, String> {
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Wake, Waker},
        time::{Duration, Instant},
    };
    // 唤醒时让测试线程继续运行
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        // 空唤醒器
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(ImageFrames::prepare(source));
    // 最多等 5 秒
    let until = Instant::now() + Duration::from_secs(5);
    // 轮询直到完成，否则停车等待唤醒
    loop {
        if let std::task::Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result.map(|value| serde_json::to_string(&value).unwrap());
        }
        // 超时说明解码线程没有唤醒消费者
        assert!(
            Instant::now() < until,
            "GIF decoder worker did not wake its consumer"
        );
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

// 把字节编码为内联 GIF 数据 URI
fn inline(bytes: &[u8]) -> String {
    format!("data:image/gif;base64,{}", STANDARD.encode(bytes))
}

// 解出某一帧 PNG 的像素
fn pixels(source: &str) -> Vec<u8> {
    // 去掉前缀并解码 base64
    let bytes = STANDARD
        .decode(source.strip_prefix("data:image/png;base64,").unwrap())
        .unwrap();
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(frame.buffer_size());
    pixels
}

#[test]
// 公开清单包含互不相同的静态 PNG 帧，并带有重复播放语义
fn public_manifest_contains_distinct_static_png_frames_and_repeat_semantics() {
    let source = inline(include_bytes!("../../tests/fixtures/assets/two-frame.gif"));
    // 同一来源两次准备，结果相同（复用不可变内容）
    let first = prepared(source.clone()).unwrap();
    assert_eq!(
        first,
        prepared(source).unwrap(),
        "prepared results reuse immutable content"
    );
    // 清单字段：动画、尺寸、无限循环、两帧、首帧 1 秒
    let value: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(value["animated"], true);
    assert_eq!(value["width"], 64);
    assert_eq!(value["height"], 64);
    assert_eq!(value["plays"], Value::Null);
    assert_eq!(value["frames"].as_array().unwrap().len(), 2);
    assert_eq!(value["frames"][0]["durationMs"], 1000);
    // 两帧各是一张完整的 PNG：一红一蓝
    let red = pixels(value["frames"][0]["source"].as_str().unwrap());
    let blue = pixels(value["frames"][1]["source"].as_str().unwrap());
    assert_eq!(red.len(), 64 * 64 * 4);
    assert_ne!(red, blue);
    assert!(red[0] > red[2] && blue[2] > blue[0]);
    // 标准 Image 组件只会消费静态 PNG，清单里没有暂停等动画控制
    assert!(
        !first.contains("paused"),
        "the stock Image consumes only static PNGs"
    );
}

#[test]
// 单帧与出错的情形不会退回到不受控的动画
fn single_frames_and_errors_do_not_fall_back_to_uncontrolled_animation() {
    // 单帧 GIF 不是动画
    let value: Value = serde_json::from_str(
        &prepare(&inline(include_bytes!(
            "../../tests/fixtures/assets/hook.gif"
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(value["animated"], false);
    // 断言
    assert_eq!(
        // 第一帧的像素
        pixels(value["frames"][0]["source"].as_str().unwrap()),
        // 应为全透明
        [0; 4]
    );
    // 非法的内联 GIF 都要报错，而不是退回不受控的动画
    for bad in [b"GIF89a".as_slice(), b"GIF90a-invalid", b"not a GIF"] {
        assert!(prepare(&inline(bad)).is_err(), "invalid inline GIF {bad:?}");
    }
}

#[test]
// 普通 PNG 走标准路径，不改写原文件
fn regular_pngs_use_the_stock_path_without_rewriting_originals() {
    // 临时的 PNG 文件
    let path = std::env::temp_dir().join(format!("loki-static-{}.png", uuid::Uuid::new_v4()));
    let bytes = include_bytes!("../../tests/fixtures/assets/hook.png");
    fs::write(&path, bytes).unwrap();
    // 普通 PNG 走标准路径：返回“非动画”
    let result = prepare(&fixtures::file_url(&path));
    // 原文件内容不变
    let untouched = fs::read(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(result.unwrap(), super::STATIC_IMAGE);
    assert_eq!(untouched, bytes);
}

#[test]
// 准备好的 PNG 保留透明度与有限的播放次数
fn prepared_pngs_preserve_transparency_and_finite_plays() {
    // 透明色为 0 的一帧，循环次数 2
    let mut frame = fixtures::frame([0, 0, 2, 1], &[0, 1]);
    frame.transparent = Some(0);
    let json = prepare(&inline(&fixtures::gif(2, 1, Some(2), vec![frame]))).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    // 重复 2 次即总共播放 3 次
    assert_eq!(value["plays"], 3);
    // 透明像素保持透明，另一像素为红色
    assert_eq!(
        pixels(value["frames"][0]["source"].as_str().unwrap()),
        [0, 0, 0, 0, 255, 0, 0, 255]
    );
}

#[test]
// 流式编码可以处理超过旧 RGBA 预算的普通动画
fn streamed_encoding_accepts_an_ordinary_animation_larger_than_the_old_rgba_budget() {
    // 部分纯色帧压缩效果很好,而保留全部完整 RGBA
    // 画布则需要 120 MiB。生产环境只保留一张合成画布。
    // 120 帧、每帧只涂一个像素，压缩得很小；若保留全部 RGBA 画布需要 120 MiB，而生产只保留一张合成画布
    let frames = (0..120)
        .map(|index| fixtures::frame([0, 0, 1, 1], &[1 + index % 2]))
        .collect();
    let bytes = fixtures::gif(512, 512, Some(0), frames);
    let value: Value = serde_json::from_str(&encode(&bytes).unwrap()).unwrap();
    assert_eq!(value["width"], 512);
    assert_eq!(value["frames"].as_array().unwrap().len(), 120);
    // 首帧第一个像素为红，末帧为蓝
    let first = pixels(value["frames"][0]["source"].as_str().unwrap());
    let last = pixels(value["frames"][119]["source"].as_str().unwrap());
    assert_eq!(first.len(), 512 * 512 * 4);
    assert_eq!(&first[..4], &[255, 0, 0, 255]);
    assert_eq!(&last[..4], &[0, 0, 255, 255]);
}

#[test]
// 8 到 32 MiB 之间的合法静态图仍走标准路径
fn valid_static_images_between_eight_and_thirty_two_mib_keep_the_stock_path() {
    let path = std::env::temp_dir().join(format!("loki-large-static-{}.png", uuid::Uuid::new_v4()));
    fs::write(
        &path,
        include_bytes!("../../tests/fixtures/assets/hook.png"),
    )
    .unwrap();
    // PNG 读取器允许 IEND 之后有多余字节;稀疏填充可避免
    // 仅为了演练来源嗅探就再保留一个 9 MiB 的测试缓冲区。
    // PNG 读取器允许 IEND 之后有多余字节；用稀疏扩展到 9 MiB，避免为测试多占一个 9 MiB 的缓冲
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(9 * 1024 * 1024)
        .unwrap();
    let result = prepare(&fixtures::file_url(&path));
    fs::remove_file(path).unwrap();
    // 返回非动画的清单
    assert_eq!(result.unwrap(), super::STATIC_IMAGE);
}
