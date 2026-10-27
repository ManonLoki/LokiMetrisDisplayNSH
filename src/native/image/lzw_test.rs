use super::*;

fn pack(codes: &[(usize, usize)]) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut accumulator, mut bits) = (0u32, 0);
    for &(code, width) in codes {
        accumulator |= (code as u32) << bits;
        bits += width;
        while bits >= 8 {
            out.push(accumulator as u8);
            accumulator >>= 8;
            bits -= 8;
        }
    }
    if bits != 0 {
        out.push(accumulator as u8);
    }
    out
}

#[test]
fn dictionary_self_reference_and_clear_codes_cross_byte_boundaries() {
    let bytes = pack(&[(4, 3), (0, 3), (6, 3), (5, 3)]);
    assert_eq!(decode(&bytes, 2, 3).unwrap(), [0, 0, 0]);
    let bytes = pack(&[(4, 3), (0, 3), (1, 3), (4, 3), (1, 3), (0, 3), (5, 3)]);
    assert_eq!(decode(&bytes, 2, 4).unwrap(), [0, 1, 1, 0]);
}

#[test]
fn dictionary_grows_to_twelve_bits_and_accepts_a_late_reset() {
    let mut codes = vec![(4, 3)];
    let (mut width, mut next) = (3, 6);
    for index in 0..5000 {
        codes.push((index % 4, width));
        if index > 0 && next < 4096 {
            next += 1;
            if next == 1 << width && width < 12 {
                width += 1;
            }
        }
    }
    assert_eq!(width, 12);
    codes.extend([(4, width), (3, 3), (5, 3)]);
    let actual = decode(&pack(&codes), 2, 5001).unwrap();
    assert_eq!(
        actual[..5000],
        (0..5000).map(|index| (index % 4) as u8).collect::<Vec<_>>()
    );
    assert_eq!(actual[5000], 3);
}

#[test]
fn invalid_dictionary_codes_sizes_and_pixel_counts_fail() {
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
    assert!(decode(&pack(&[(4, 3), (0, 3), (5, 3)]), 2, 2).is_err());
    assert!(decode(&pack(&[(4, 3), (0, 3), (1, 3), (5, 3)]), 2, 1).is_err());
    assert!(decode(&[0x44], 1, 1).is_err());
    assert!(decode(&[0x44], 9, 1).is_err());
    assert!(decode(&[], 2, 1).is_err());
    assert!(decode(&[0x44], 2, 2).is_err());
}

#[test]
fn the_existing_one_pixel_gif_accepts_only_a_partial_end_prefix_after_exact_pixels() {
    assert_eq!(decode(&[0x44], 2, 1).unwrap(), [0]);
    assert!(
        decode(&[0x04], 2, 1).is_err(),
        "zero tail is not the incomplete EOI prefix"
    );
}
