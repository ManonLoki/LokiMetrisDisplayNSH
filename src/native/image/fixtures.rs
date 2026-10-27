//! Small independently encoded GIF test inputs; each literal follows a clear.

pub(super) struct TestFrame {
    pub rect: [u16; 4],
    pub indices: Vec<u8>,
    pub delay: u16,
    pub disposal: u8,
    pub transparent: Option<u8>,
    pub interlaced: bool,
    pub palette: Option<Vec<[u8; 3]>>,
}

pub(super) fn frame(rect: [u16; 4], indices: &[u8]) -> TestFrame {
    TestFrame {
        rect,
        indices: indices.into(),
        delay: 10,
        disposal: 1,
        transparent: None,
        interlaced: false,
        palette: None,
    }
}

pub(super) fn literal_data(indices: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0;
    for code in indices.iter().flat_map(|index| [4, *index]).chain([5]) {
        accumulator |= u32::from(code) << bits;
        bits += 3;
        while bits >= 8 {
            bytes.push(accumulator as u8);
            accumulator >>= 8;
            bits -= 8;
        }
    }
    if bits > 0 {
        bytes.push(accumulator as u8);
    }
    bytes
}

pub(super) fn gif(width: u16, height: u16, repeat: Option<u16>, frames: Vec<TestFrame>) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend([0x81, 0, 0]);
    bytes.extend([0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 255, 0]);
    if let Some(repeat) = repeat {
        bytes.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01");
        bytes.extend(repeat.to_le_bytes());
        bytes.push(0);
    }
    for frame in frames {
        bytes.extend([
            0x21,
            0xf9,
            4,
            (frame.disposal << 2) | u8::from(frame.transparent.is_some()),
        ]);
        bytes.extend(frame.delay.to_le_bytes());
        bytes.extend([frame.transparent.unwrap_or(0), 0, 0x2c]);
        for word in frame.rect {
            bytes.extend(word.to_le_bytes());
        }
        let mut packed = if frame.interlaced { 0x40 } else { 0 };
        if let Some(palette) = &frame.palette {
            packed |= 0x80 | (palette.len().ilog2() as u8 - 1);
        }
        bytes.push(packed);
        if let Some(palette) = frame.palette {
            for color in palette {
                bytes.extend(color);
            }
        }
        bytes.push(2);
        for block in literal_data(&frame.indices).chunks(255) {
            bytes.push(block.len() as u8);
            bytes.extend(block);
        }
        bytes.push(0);
    }
    bytes.push(0x3b);
    bytes
}
