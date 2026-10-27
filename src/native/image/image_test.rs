use super::*;
use serde_json::Value;
use std::{fs, io::Cursor};

fn prepared(source: String) -> Result<String, String> {
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Wake, Waker},
        time::{Duration, Instant},
    };
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(ImageFrames::prepare(source));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let std::task::Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
        assert!(
            Instant::now() < until,
            "GIF decoder worker did not wake its consumer"
        );
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

fn inline(bytes: &[u8]) -> String {
    format!("data:image/gif;base64,{}", STANDARD.encode(bytes))
}

fn pixels(source: &str) -> Vec<u8> {
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
fn public_manifest_contains_distinct_static_png_frames_and_repeat_semantics() {
    let source = inline(include_bytes!("../../tests/fixtures/assets/two-frame.gif"));
    let first = prepared(source.clone()).unwrap();
    assert_eq!(
        first,
        prepared(source).unwrap(),
        "prepared results reuse immutable content"
    );
    let value: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(value["animated"], true);
    assert_eq!(value["width"], 64);
    assert_eq!(value["height"], 64);
    assert_eq!(value["plays"], Value::Null);
    assert_eq!(value["frames"].as_array().unwrap().len(), 2);
    assert_eq!(value["frames"][0]["durationMs"], 1000);
    let red = pixels(value["frames"][0]["source"].as_str().unwrap());
    let blue = pixels(value["frames"][1]["source"].as_str().unwrap());
    assert_eq!(red.len(), 64 * 64 * 4);
    assert_ne!(red, blue);
    assert!(red[0] > red[2] && blue[2] > blue[0]);
    assert!(
        !first.contains("paused"),
        "the stock Image consumes only static PNGs"
    );
}

#[test]
fn single_frames_and_errors_do_not_fall_back_to_uncontrolled_animation() {
    let value: Value = serde_json::from_str(
        &prepare(&inline(include_bytes!(
            "../../tests/fixtures/assets/hook.gif"
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(value["animated"], false);
    assert_eq!(
        pixels(value["frames"][0]["source"].as_str().unwrap()),
        [0; 4]
    );
    for bad in [b"GIF89a".as_slice(), b"GIF90a-invalid", b"not a GIF"] {
        assert!(prepare(&inline(bad)).is_err(), "invalid inline GIF {bad:?}");
    }
}

#[test]
fn regular_pngs_use_the_stock_path_without_rewriting_originals() {
    let path = std::env::temp_dir().join(format!("loki-static-{}.png", uuid::Uuid::new_v4()));
    let bytes = include_bytes!("../../tests/fixtures/assets/hook.png");
    fs::write(&path, bytes).unwrap();
    let result = prepare(&format!("file://{}", path.display()));
    let untouched = fs::read(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(result.unwrap(), "{\"animated\":false}");
    assert_eq!(untouched, bytes);
}

#[test]
fn prepared_pngs_preserve_transparency_and_finite_plays() {
    let mut frame = fixtures::frame([0, 0, 2, 1], &[0, 1]);
    frame.transparent = Some(0);
    let json = prepare(&inline(&fixtures::gif(2, 1, Some(2), vec![frame]))).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["plays"], 3);
    assert_eq!(
        pixels(value["frames"][0]["source"].as_str().unwrap()),
        [0, 0, 0, 0, 255, 0, 0, 255]
    );
}

#[test]
fn streamed_encoding_accepts_an_ordinary_animation_larger_than_the_old_rgba_budget() {
    // Partial solid frames compress well, even though retaining all full RGBA
    // canvases would need 120 MiB. Production retains one composition canvas.
    let frames = (0..120)
        .map(|index| fixtures::frame([0, 0, 1, 1], &[1 + index % 2]))
        .collect();
    let bytes = fixtures::gif(512, 512, Some(0), frames);
    let value: Value = serde_json::from_str(&encode(&bytes).unwrap()).unwrap();
    assert_eq!(value["width"], 512);
    assert_eq!(value["frames"].as_array().unwrap().len(), 120);
    let first = pixels(value["frames"][0]["source"].as_str().unwrap());
    let last = pixels(value["frames"][119]["source"].as_str().unwrap());
    assert_eq!(first.len(), 512 * 512 * 4);
    assert_eq!(&first[..4], &[255, 0, 0, 255]);
    assert_eq!(&last[..4], &[0, 0, 255, 255]);
}

#[test]
fn valid_static_images_between_eight_and_thirty_two_mib_keep_the_stock_path() {
    let path = std::env::temp_dir().join(format!("loki-large-static-{}.png", uuid::Uuid::new_v4()));
    fs::write(
        &path,
        include_bytes!("../../tests/fixtures/assets/hook.png"),
    )
    .unwrap();
    // PNG readers permit trailing bytes after IEND; sparse padding avoids
    // retaining another nine-MiB test buffer merely to exercise source sniffing.
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(9 * 1024 * 1024)
        .unwrap();
    let result = prepare(&format!("file://{}", path.display()));
    fs::remove_file(path).unwrap();
    assert_eq!(result.unwrap(), "{\"animated\":false}");
}
