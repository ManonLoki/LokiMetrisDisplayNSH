// 引入测试用 GIF 构造函数
use super::super::fixtures::{frame, gif};
use super::*;

// 红、蓝、绿三种 RGBA 颜色
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

#[test]
// 真实的 GIF87a 与最小的 GIF89a 夹具都能解码出完整画布
fn real_gif87a_and_minimal_gif89a_fixtures_decode_complete_canvases() {
    // 真实的双帧 GIF：64x64、两帧
    let animation = decode(include_bytes!("../../tests/fixtures/assets/two-frame.gif")).unwrap();
    assert_eq!(
        (animation.width, animation.height, animation.frames.len()),
        (64, 64, 2)
    );
    // 无限循环
    assert_eq!(animation.plays, None);
    // 第一帧 1 秒
    assert_eq!(animation.frames[0].duration_ms, 1000);
    // 两帧像素不同
    assert_ne!(animation.frames[0].pixels, animation.frames[1].pixels);
    // 最小的 1x1 GIF
    let single = decode(include_bytes!("../../tests/fixtures/assets/hook.gif")).unwrap();
    assert_eq!(
        (single.width, single.height, single.frames.len()),
        (1, 1, 1)
    );
    // 该像素是透明的
    assert_eq!(single.frames[0].pixels, [0, 0, 0, 0]);
}

#[test]
// 透明色与“恢复到上一帧”“恢复背景”两种处置方式能正确合成矩形帧
fn transparency_and_restore_previous_and_background_compose_rectangular_frames() {
    // 第一帧：两个红色像素
    let first = frame([0, 0, 2, 1], &[1, 1]);
    // 第二帧：左侧涂蓝，处置方式 3（恢复到上一帧）
    let mut second = frame([0, 0, 1, 1], &[2]);
    second.disposal = 3;
    // 第三帧：右侧涂绿，处置方式 2（恢复背景）
    let mut third = frame([1, 0, 1, 1], &[3]);
    third.disposal = 2;
    third.transparent = Some(0);
    // 第四帧：透明像素，保留画布原色
    let mut fourth = frame([0, 0, 1, 1], &[0]);
    fourth.transparent = Some(0);
    let animation = decode(&gif(2, 1, None, vec![first, second, third, fourth])).unwrap();
    // 第一帧：两红
    assert_eq!(animation.frames[0].pixels, [RED, RED].concat());
    // 第二帧：蓝红
    assert_eq!(animation.frames[1].pixels, [BLUE, RED].concat());
    // 第三帧：第二帧已恢复，所以左边回到红色
    assert_eq!(animation.frames[2].pixels, [RED, GREEN].concat());
    // 第四帧：右侧已按背景（透明）清除
    assert_eq!(animation.frames[3].pixels, [RED, [0; 4]].concat());
    // 另一组：不透明时恢复背景使用 GIF 的背景色
    let mut erased = frame([1, 0, 1, 1], &[2]);
    erased.disposal = 2;
    let animation = decode(&gif(
        2,
        1,
        None,
        vec![
            frame([0, 0, 2, 1], &[1, 1]),
            // 擦除帧
            erased,
            // 一像素帧
            frame([0, 0, 1, 1], &[3]),
        ],
    ))
    // 构造成功
    .unwrap();
    // 被清除的位置是黑色背景
    assert_eq!(animation.frames[2].pixels, [GREEN, [0, 0, 0, 255]].concat());
}

#[test]
// 局部调色板与隔行四趟扫描按逻辑行排列
fn local_palette_and_four_interlace_passes_use_logical_rows() {
    // 局部调色板覆盖全局调色板
    let mut local = frame([0, 0, 1, 1], &[1]);
    local.palette = Some(vec![[0, 0, 0], [7, 8, 9]]);
    let animation = decode(&gif(1, 1, None, vec![local])).unwrap();
    assert_eq!(animation.frames[0].pixels, [7, 8, 9, 255]);
    // 隔行：8 行的数据按四趟顺序存放
    let mut interlaced = frame([0, 0, 1, 8], &[0, 0, 2, 2, 1, 3, 1, 3]);
    interlaced.interlaced = true;
    let animation = decode(&gif(1, 8, None, vec![interlaced])).unwrap();
    // 解码后各行回到逻辑顺序
    assert_eq!(
        animation.frames[0].pixels,
        [
            [0, 0, 0, 255],
            RED,
            BLUE,
            GREEN,
            [0, 0, 0, 255],
            // 红
            RED,
            // 蓝
            BLUE,
            // 绿
            GREEN
        ]
        // 拼接像素
        .concat()
    );
}

#[test]
// Netscape 的重复次数转为总播放次数，过短的延迟被规范化
fn netscape_repeats_become_total_plays_and_short_delays_are_normalized() {
    // Netscape 重复次数换算成总播放次数：无扩展为 1，0 为无限，n 为 n+1
    for (repeat, plays) in [
        (None, Some(1)),
        (Some(0), None),
        (Some(1), Some(2)),
        (Some(u16::MAX), Some(65536)),
    ] {
        let animation = decode(&gif(1, 1, repeat, vec![frame([0, 0, 1, 1], &[1])])).unwrap();
        assert_eq!(animation.plays, plays, "repeat {repeat:?}");
    }
    // 过短的延迟（小于 2/100 秒）按 100 毫秒处理，其余乘 10
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
// 截断的块、保留的处置方式与非法矩形都会失败
fn truncated_blocks_reserved_disposal_and_invalid_rectangles_fail() {
    let bytes = gif(1, 1, None, vec![frame([0, 0, 1, 1], &[1])]);
    // 在每个位置截断都应报错
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "truncation at {end}");
    }
    // 空矩形与越界矩形都应失败
    for rect in [[0, 0, 0, 1], [1, 0, 1, 1], [65535, 0, 65535, 1]] {
        assert!(decode(&gif(1, 1, None, vec![frame(rect, &[1])])).is_err());
    }
    // 保留的处置方式 7
    let mut reserved = frame([0, 0, 1, 1], &[1]);
    reserved.disposal = 7;
    assert!(decode(&gif(1, 1, None, vec![reserved])).is_err());
    // 颜色索引超出调色板
    let mut missing_color = frame([0, 0, 1, 1], &[3]);
    missing_color.palette = Some(vec![[0; 3], [255; 3]]);
    assert!(decode(&gif(1, 1, None, vec![missing_color])).is_err());
}

#[test]
// 尺寸、帧数与总像素处理量都有硬上限
fn dimensions_frame_count_and_total_pixel_work_have_hard_limits() {
    // 尺寸为零、单边超限或像素总数超限都应失败
    for (width, height) in [(0, 1), (4097, 1), (2048, 8192)] {
        assert!(decode(&gif(width, height, None, vec![frame([0, 0, 1, 1], &[1])])).is_err());
    }
    // 513 帧超过 512 的上限
    let many = (0..513).map(|_| frame([0, 0, 1, 1], &[1])).collect();
    assert!(decode(&gif(1, 1, None, many)).is_err());
    // 最大画布恰为 64 MiB
    assert_eq!(canvas_bytes(4096, 4096).unwrap(), 64 * 1024 * 1024);
    // 1024x1024 的 129 帧超过总像素处理量
    let large = (0..129).map(|_| frame([0, 0, 1, 1], &[1])).collect();
    let mut emitted = 0;
    let result = decode_each(&gif(1024, 1024, None, large), |_, _, _, _| {
        emitted += 1;
        Ok(())
    });
    assert!(result.is_err());
    // 超限的那一帧在合成与编码之前就被拒绝，只输出 128 帧
    assert_eq!(
        emitted, 128,
        "reject the excess frame before composition or encoding"
    );
}
