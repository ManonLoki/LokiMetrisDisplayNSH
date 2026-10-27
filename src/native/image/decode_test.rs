use super::super::fixtures::{frame, gif};
use super::*;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

#[test]
fn real_gif87a_and_minimal_gif89a_fixtures_decode_complete_canvases() {
    let animation = decode(include_bytes!("../../tests/fixtures/assets/two-frame.gif")).unwrap();
    assert_eq!(
        (animation.width, animation.height, animation.frames.len()),
        (64, 64, 2)
    );
    assert_eq!(animation.plays, None);
    assert_eq!(animation.frames[0].duration_ms, 1000);
    assert_ne!(animation.frames[0].pixels, animation.frames[1].pixels);
    let single = decode(include_bytes!("../../tests/fixtures/assets/hook.gif")).unwrap();
    assert_eq!(
        (single.width, single.height, single.frames.len()),
        (1, 1, 1)
    );
    assert_eq!(single.frames[0].pixels, [0, 0, 0, 0]);
}

#[test]
fn transparency_and_restore_previous_and_background_compose_rectangular_frames() {
    let first = frame([0, 0, 2, 1], &[1, 1]);
    let mut second = frame([0, 0, 1, 1], &[2]);
    second.disposal = 3;
    let mut third = frame([1, 0, 1, 1], &[3]);
    third.disposal = 2;
    third.transparent = Some(0);
    let mut fourth = frame([0, 0, 1, 1], &[0]);
    fourth.transparent = Some(0);
    let animation = decode(&gif(2, 1, None, vec![first, second, third, fourth])).unwrap();
    assert_eq!(animation.frames[0].pixels, [RED, RED].concat());
    assert_eq!(animation.frames[1].pixels, [BLUE, RED].concat());
    assert_eq!(animation.frames[2].pixels, [RED, GREEN].concat());
    assert_eq!(animation.frames[3].pixels, [RED, [0; 4]].concat());
    let mut erased = frame([1, 0, 1, 1], &[2]);
    erased.disposal = 2;
    let animation = decode(&gif(
        2,
        1,
        None,
        vec![
            frame([0, 0, 2, 1], &[1, 1]),
            erased,
            frame([0, 0, 1, 1], &[3]),
        ],
    ))
    .unwrap();
    assert_eq!(animation.frames[2].pixels, [GREEN, [0, 0, 0, 255]].concat());
}

#[test]
fn local_palette_and_four_interlace_passes_use_logical_rows() {
    let mut local = frame([0, 0, 1, 1], &[1]);
    local.palette = Some(vec![[0, 0, 0], [7, 8, 9]]);
    let animation = decode(&gif(1, 1, None, vec![local])).unwrap();
    assert_eq!(animation.frames[0].pixels, [7, 8, 9, 255]);
    let mut interlaced = frame([0, 0, 1, 8], &[0, 0, 2, 2, 1, 3, 1, 3]);
    interlaced.interlaced = true;
    let animation = decode(&gif(1, 8, None, vec![interlaced])).unwrap();
    assert_eq!(
        animation.frames[0].pixels,
        [
            [0, 0, 0, 255],
            RED,
            BLUE,
            GREEN,
            [0, 0, 0, 255],
            RED,
            BLUE,
            GREEN
        ]
        .concat()
    );
}

#[test]
fn netscape_repeats_become_total_plays_and_short_delays_are_normalized() {
    for (repeat, plays) in [
        (None, Some(1)),
        (Some(0), None),
        (Some(1), Some(2)),
        (Some(u16::MAX), Some(65536)),
    ] {
        let animation = decode(&gif(1, 1, repeat, vec![frame([0, 0, 1, 1], &[1])])).unwrap();
        assert_eq!(animation.plays, plays, "repeat {repeat:?}");
    }
    for (delay, duration) in [(0, 100), (1, 100), (2, 20), (7, 70)] {
        let mut item = frame([0, 0, 1, 1], &[1]);
        item.delay = delay;
        assert_eq!(
            decode(&gif(1, 1, None, vec![item])).unwrap().frames[0].duration_ms,
            duration
        );
    }
}

#[test]
fn truncated_blocks_reserved_disposal_and_invalid_rectangles_fail() {
    let bytes = gif(1, 1, None, vec![frame([0, 0, 1, 1], &[1])]);
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "truncation at {end}");
    }
    for rect in [[0, 0, 0, 1], [1, 0, 1, 1], [65535, 0, 65535, 1]] {
        assert!(decode(&gif(1, 1, None, vec![frame(rect, &[1])])).is_err());
    }
    let mut reserved = frame([0, 0, 1, 1], &[1]);
    reserved.disposal = 7;
    assert!(decode(&gif(1, 1, None, vec![reserved])).is_err());
    let mut missing_color = frame([0, 0, 1, 1], &[3]);
    missing_color.palette = Some(vec![[0; 3], [255; 3]]);
    assert!(decode(&gif(1, 1, None, vec![missing_color])).is_err());
}

#[test]
fn dimensions_frame_count_and_total_pixel_work_have_hard_limits() {
    for (width, height) in [(0, 1), (4097, 1), (2048, 8192)] {
        assert!(decode(&gif(width, height, None, vec![frame([0, 0, 1, 1], &[1])])).is_err());
    }
    let many = (0..513).map(|_| frame([0, 0, 1, 1], &[1])).collect();
    assert!(decode(&gif(1, 1, None, many)).is_err());
    assert_eq!(canvas_bytes(4096, 4096).unwrap(), 64 * 1024 * 1024);
    let large = (0..129).map(|_| frame([0, 0, 1, 1], &[1])).collect();
    let mut emitted = 0;
    let result = decode_each(&gif(1024, 1024, None, large), |_, _, _, _| {
        emitted += 1;
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(
        emitted, 128,
        "reject the excess frame before composition or encoding"
    );
}
