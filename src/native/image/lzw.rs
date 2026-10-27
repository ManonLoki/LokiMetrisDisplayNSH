//! Bounded GIF LZW indices; the caller owns palette and canvas composition.

struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Bits<'_> {
    fn read(&mut self, width: usize) -> Option<usize> {
        if self.position + width > self.bytes.len() * 8 {
            return None;
        }
        let mut value = 0;
        for bit in 0..width {
            let at = self.position + bit;
            value |= usize::from((self.bytes[at / 8] >> (at % 8)) & 1) << bit;
        }
        self.position += width;
        Some(value)
    }

    fn partial_end(&mut self, end: usize, width: usize) -> bool {
        let remaining = self.bytes.len() * 8 - self.position;
        // Some existing 1-pixel web GIFs omit the last bit of EOI. Accept
        // only its actual low-bit prefix after every expected pixel exists.
        remaining > 0
            && remaining < width
            && self.read(remaining) == Some(end & ((1 << remaining) - 1))
    }
}

pub(super) fn decode(bytes: &[u8], minimum: u8, pixels: usize) -> Result<Vec<u8>, String> {
    if !(2..=8).contains(&minimum) || pixels == 0 {
        return Err("GIF LZW参数无效".into());
    }
    let clear = 1usize << minimum;
    let end = clear + 1;
    let mut width = usize::from(minimum) + 1;
    let mut next = end + 1;
    let mut previous = None;
    let mut prefix = [0u16; 4096];
    let mut suffix = [0u8; 4096];
    let mut stack = Vec::with_capacity(4096);
    let mut output = Vec::with_capacity(pixels);
    let mut bits = Bits { bytes, position: 0 };
    if bits.read(width) != Some(clear) {
        return Err("GIF LZW缺少初始清除码".into());
    }
    loop {
        let Some(code) = bits.read(width) else {
            return if output.len() == pixels && bits.partial_end(end, width) {
                Ok(output)
            } else {
                Err("GIF LZW数据截断".into())
            };
        };
        if code == clear {
            width = usize::from(minimum) + 1;
            next = end + 1;
            previous = None;
            continue;
        }
        if code == end {
            return if output.len() == pixels {
                Ok(output)
            } else {
                Err("GIF像素数量不足".into())
            };
        }
        let repeated = code == next;
        let mut entry = if repeated {
            previous.ok_or("GIF LZW字典引用无效")?
        } else if code < next {
            code
        } else {
            return Err("GIF LZW字典引用越界".into());
        };
        stack.clear();
        while entry >= clear {
            if entry >= next || stack.len() >= 4095 {
                return Err("GIF LZW字典链无效".into());
            }
            stack.push(suffix[entry]);
            entry = usize::from(prefix[entry]);
        }
        let first = entry as u8;
        stack.push(first);
        if output.len() + stack.len() + usize::from(repeated) > pixels {
            return Err("GIF像素数量超出画布".into());
        }
        output.extend(stack.iter().rev());
        if repeated {
            output.push(first);
        }
        if let Some(last) = previous
            && next < 4096
        {
            prefix[next] = last as u16;
            suffix[next] = first;
            next += 1;
            if next == (1 << width) && width < 12 {
                width += 1;
            }
        }
        previous = Some(code);
    }
}

#[cfg(test)]
#[path = "lzw_test.rs"]
mod lzw_test;
