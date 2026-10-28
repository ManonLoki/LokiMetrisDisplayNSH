//! 有界的 GIF LZW 索引;调色板与画布合成由调用方负责。

// 按位读取器，低位在前
struct Bits<'a> {
    // 待读取的字节
    bytes: &'a [u8],
    // 当前位偏移
    position: usize,
}

// 位读取器的方法
impl Bits<'_> {
    // 读取 width 位，不足返回 None
    fn read(&mut self, width: usize) -> Option<usize> {
        // 位数不够
        if self.position + width > self.bytes.len() * 8 {
            // 返回 None
            return None;
        }
        // 累积的值
        let mut value = 0;
        // 逐位拼出数值
        for bit in 0..width {
            // 位的位置
            let at = self.position + bit;
            // 按低位在前拼出该位
            value |= usize::from((self.bytes[at / 8] >> (at % 8)) & 1) << bit;
        }
        // 前进读取位置
        self.position += width;
        // 返回读到的值
        Some(value)
    }

    // 允许结束码缺最后几位：有些 1 像素网页 GIF 会省略 EOI 的末位；仅当像素已齐全且剩余位恰为结束码的低位前缀时接受
    fn partial_end(&mut self, end: usize, width: usize) -> bool {
        // 剩余位数
        let remaining = self.bytes.len() * 8 - self.position;
        // 一些现有的 1 像素网页 GIF 会省略 EOI 的最后一位。仅在
        // 所有预期像素都已出现之后,才接受其实际的低位前缀。
        remaining > 0
            // 且剩余位数不足一个码宽
            && remaining < width
            // 且剩余位等于结束码的低位
            && self.read(remaining) == Some(end & ((1 << remaining) - 1))
    }
}

// 解压 LZW：返回调色板索引，调色板与画布合成由调用方负责
pub(super) fn decode(bytes: &[u8], minimum: u8, pixels: usize) -> Result<Vec<u8>, String> {
    // 最小码长必须在 2 到 8
    if !(2..=8).contains(&minimum) || pixels == 0 {
        // 参数无效
        return Err("GIF LZW参数无效".into());
    }
    // 清除码
    let clear = 1usize << minimum;
    // 结束码
    let end = clear + 1;
    // 当前码宽
    let mut width = usize::from(minimum) + 1;
    // 下一个字典项
    let mut next = end + 1;
    // 上一个码
    let mut previous = None;
    // 字典：前缀与后缀
    let mut prefix = [0u16; 4096];
    // 后缀表
    let mut suffix = [0u8; 4096];
    // 展开字符串用的栈
    let mut stack = Vec::with_capacity(4096);
    // 输出的像素索引
    let mut output = Vec::with_capacity(pixels);
    // 位读取器
    let mut bits = Bits { bytes, position: 0 };
    // 数据必须以清除码开始
    if bits.read(width) != Some(clear) {
        // 缺少初始清除码
        return Err("GIF LZW缺少初始清除码".into());
    }
    // 循环读取编码
    loop {
        // 数据读完：像素齐全且结束码仅缺末位时才算正常
        let Some(code) = bits.read(width) else {
            // 遇到结束码：像素数量须正好
            return if output.len() == pixels && bits.partial_end(end, width) {
                // 返回输出
                Ok(output)
            // 否则分支
            } else {
                // 数据截断
                Err("GIF LZW数据截断".into())
            };
        };
        // 清除码：重置字典
        if code == clear {
            // 重置码宽
            width = usize::from(minimum) + 1;
            // 重置字典下标
            next = end + 1;
            // 清除上一个编码
            previous = None;
            // 继续下一个编码
            continue;
        }
        // 结束码：像素数量必须正好
        if code == end {
            // 没有结束码时像素须正好
            return if output.len() == pixels {
                // 返回输出
                Ok(output)
            // 否则分支
            } else {
                // 像素数量不足
                Err("GIF像素数量不足".into())
            };
        }
        // 码等于下一个字典项：特殊情形，内容是上一串加其首字符
        let repeated = code == next;
        // 当前编码对应的字典项
        let mut entry = if repeated {
            // 重复的编码引用上一项
            previous.ok_or("GIF LZW字典引用无效")?
        // 引用字典里已有的编码
        } else if code < next {
            // 否则取该编码
            code
        // 否则分支
        } else {
            // 字典引用越界
            return Err("GIF LZW字典引用越界".into());
        };
        // 沿前缀链展开，长度和合法性都要检查
        stack.clear();
        // 沿字典链回溯
        while entry >= clear {
            // 链条无效或过长
            if entry >= next || stack.len() >= 4095 {
                // 字典链无效
                return Err("GIF LZW字典链无效".into());
            }
            // 压入后缀
            stack.push(suffix[entry]);
            // 取前缀
            entry = usize::from(prefix[entry]);
        }
        // 串的首字符
        let first = entry as u8;
        // 压入首字节
        stack.push(first);
        // 输出不得超过画布像素数
        if output.len() + stack.len() + usize::from(repeated) > pixels {
            // 像素数量超出画布
            return Err("GIF像素数量超出画布".into());
        }
        // 栈是倒序的，反向写出
        output.extend(stack.iter().rev());
        // 重复编码
        if repeated {
            // 补上首字节
            output.push(first);
        }
        // 把“上一串加本串首字符”加入字典，字典满 4096 后不再增长
        if let Some(last) = previous
            // 且字典项数未满
            && next < 4096
        {
            // 新字典项的前缀
            prefix[next] = last as u16;
            // 新字典项的后缀
            suffix[next] = first;
            // 字典下标加一
            next += 1;
            // 字典填满当前码宽后加宽（最多 12 位）
            if next == (1 << width) && width < 12 {
                // 码宽加一
                width += 1;
            }
        }
        // 记下上一个编码
        previous = Some(code);
    }
}

// 测试模块
#[cfg(test)]
#[path = "lzw_test.rs"]
mod lzw_test;
