// 引入被测模块
use super::*;

// 把（码, 位宽）序列打包为字节
fn pack(codes: &[(usize, usize)]) -> Vec<u8> {
    // 输出字节
    let mut out = Vec::new();
    // 位累加器与已累积的位数
    let (mut accumulator, mut bits) = (0u32, 0);
    // 逐个码按低位在前写入
    for &(code, width) in codes {
        accumulator |= (code as u32) << bits;
        bits += width;
        // 凑满一字节就输出
        while bits >= 8 {
            out.push(accumulator as u8);
            accumulator >>= 8;
            bits -= 8;
        }
    }
    // 最后不足一字节的位补齐
    if bits != 0 {
        out.push(accumulator as u8);
    }
    out
}

#[test]
// 字典的自引用与清除码可以跨越字节边界
fn dictionary_self_reference_and_clear_codes_cross_byte_boundaries() {
    // 清除码、0、字典自引用（码 6）、结束码
    let bytes = pack(&[(4, 3), (0, 3), (6, 3), (5, 3)]);
    // 自引用展开为 0、0 加首字符 0
    assert_eq!(decode(&bytes, 2, 3).unwrap(), [0, 0, 0]);
    // 中途再次出现清除码，字典重置后继续解码
    let bytes = pack(&[(4, 3), (0, 3), (1, 3), (4, 3), (1, 3), (0, 3), (5, 3)]);
    assert_eq!(decode(&bytes, 2, 4).unwrap(), [0, 1, 1, 0]);
}

#[test]
// 字典增长到 12 位，并接受较晚的重置
fn dictionary_grows_to_twelve_bits_and_accepts_a_late_reset() {
    // 构造足够长的数据，使码宽从 3 增长到 12
    let mut codes = vec![(4, 3)];
    let (mut width, mut next) = (3, 6);
    for index in 0..5000 {
        codes.push((index % 4, width));
        // 模拟解码器的字典增长与码宽增加
        if index > 0 && next < 4096 {
            next += 1;
            if next == 1 << width && width < 12 {
                width += 1;
            }
        }
    }
    // 码宽已到 12 位上限
    assert_eq!(width, 12);
    // 字典满后又出现清除码，随后码宽回到 3
    codes.extend([(4, width), (3, 3), (5, 3)]);
    let actual = decode(&pack(&codes), 2, 5001).unwrap();
    // 前 5000 个像素与输入一致
    assert_eq!(
        actual[..5000],
        (0..5000).map(|index| (index % 4) as u8).collect::<Vec<_>>()
    );
    // 重置后的像素也正确
    assert_eq!(actual[5000], 3);
}

#[test]
// 非法的字典码、大小与像素数量都会失败
fn invalid_dictionary_codes_sizes_and_pixel_counts_fail() {
    // 非法码序列：码 7 越界、码 6 在没有前一个码时无效、缺少初始清除码
    for codes in [
        vec![(4, 3), (7, 3)],
        vec![(4, 3), (6, 3)],
        vec![(0, 3), (5, 3)],
    ] {
        assert!(
            decode(&pack(&codes), 2, 1).is_err(),
            "invalid codes {codes:?}"
        );
    }
    // 像素不足
    assert!(decode(&pack(&[(4, 3), (0, 3), (5, 3)]), 2, 2).is_err());
    // 像素多于画布
    assert!(decode(&pack(&[(4, 3), (0, 3), (1, 3), (5, 3)]), 2, 1).is_err());
    // 最小码长超出 2 到 8 的范围
    assert!(decode(&[0x44], 1, 1).is_err());
    assert!(decode(&[0x44], 9, 1).is_err());
    // 空数据
    assert!(decode(&[], 2, 1).is_err());
    // 数据截断
    assert!(decode(&[0x44], 2, 2).is_err());
}

#[test]
// 已有的 1 像素 GIF 只在像素齐全后才接受结束码的部分前缀
fn the_existing_one_pixel_gif_accepts_only_a_partial_end_prefix_after_exact_pixels() {
    // 结束码缺末位，但像素已齐全，可接受
    assert_eq!(decode(&[0x44], 2, 1).unwrap(), [0]);
    // 全零的尾部不是结束码的前缀，必须拒绝
    assert!(
        decode(&[0x04], 2, 1).is_err(),
        "zero tail is not the incomplete EOI prefix"
    );
}
