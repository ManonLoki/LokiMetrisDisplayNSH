//! 小型的、独立编码的 GIF 测试输入;每个字面量后面都跟着一次清除。

// 测试临时文件使用合法的本地 URI，兼容 Windows 盘符并编码空格等特殊字符。
pub(super) fn file_url(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    let raw = raw.replace('\\', "/");
    let encoded: String = raw
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"/:-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

// 测试用 GIF 的一帧
pub(super) struct TestFrame {
    // 帧矩形：x、y、宽、高
    pub rect: [u16; 4],
    pub indices: Vec<u8>,
    // 延迟（百分之一秒）
    pub delay: u16,
    pub disposal: u8,
    pub transparent: Option<u8>,
    pub interlaced: bool,
    pub palette: Option<Vec<[u8; 3]>>,
}

// 构造默认帧：延迟 10、处置 1
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

// 用“清除码 + 字面量”的方式编码 LZW（3 位码宽），结束时写结束码
pub(super) fn literal_data(indices: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0;
    // 每个索引前插入清除码 4，末尾写结束码 5
    for code in indices.iter().flat_map(|index| [4, *index]).chain([5]) {
        accumulator |= u32::from(code) << bits;
        bits += 3;
        while bits >= 8 {
            bytes.push(accumulator as u8);
            accumulator >>= 8;
            bits -= 8;
        }
    }
    // 剩余不足一字节的位补齐输出
    if bits > 0 {
        bytes.push(accumulator as u8);
    }
    bytes
}

// 拼装一个完整的 GIF89a 字节流
pub(super) fn gif(width: u16, height: u16, repeat: Option<u16>, frames: Vec<TestFrame>) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    // 带 4 色全局调色板
    bytes.extend([0x81, 0, 0]);
    // 全局调色板：黑、红、蓝、绿
    bytes.extend([0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 255, 0]);
    if let Some(repeat) = repeat {
        bytes.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01");
        bytes.extend(repeat.to_le_bytes());
        bytes.push(0);
    }
    // 每帧：图形控制扩展、图像描述符、可选局部调色板与图像数据
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
        // 隔行与局部调色板标志
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
        // 数据按 255 字节分块
        for block in literal_data(&frame.indices).chunks(255) {
            bytes.push(block.len() as u8);
            bytes.extend(block);
        }
        bytes.push(0);
    }
    bytes.push(0x3b);
    bytes
}
