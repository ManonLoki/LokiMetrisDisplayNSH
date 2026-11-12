//! GIF 块解析,以及在 PNG 编码之前的全画布 RGBA 合成。
use super::{MAX_FRAME_PIXELS, MAX_FRAMES, MAX_PIXELS, MAX_SIDE, lzw};

#[cfg(test)]
// 测试用：解码后的一帧
pub(super) struct Frame {
    pub pixels: Vec<u8>,
    pub duration_ms: u32,
}

#[cfg(test)]
pub(super) struct Animation {
    pub width: u16,
    pub height: u16,
    pub plays: Option<u32>,
    pub frames: Vec<Frame>,
}

// 动画的基本信息：尺寸与播放次数
pub(super) struct AnimationInfo {
    pub width: u16,
    pub height: u16,
    pub plays: Option<u32>,
}

#[derive(Clone, Copy, Default)]
// 图形控制扩展：延迟、处置方式与透明色
struct Control {
    delay: u16,
    disposal: u8,
    transparent: Option<u8>,
}

// 带边界检查的字节读取器
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    // 取出 count 字节，越界报“数据截断”
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        // 计算读取终点，溢出则报错
        let end = self.position.checked_add(count).ok_or("GIF长度溢出")?;
        // 取出区间，数据不足则报截断
        let value = self.bytes.get(self.position..end).ok_or("GIF数据截断")?;
        self.position = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn word(&mut self) -> Result<u16, String> {
        let value = self.take(2)?;
        // 按小端组成 16 位整数
        Ok(u16::from_le_bytes([value[0], value[1]]))
    }

    // 读取由子块组成的数据，遇到长度 0 的块结束
    fn blocks(&mut self) -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        loop {
            let count = usize::from(self.byte()?);
            // 长度为 0 表示结束
            if count == 0 {
                return Ok(data);
            }
            data.extend_from_slice(self.take(count)?);
        }
    }

    // 读取调色板：由打包字节决定是否存在与大小
    fn palette(&mut self, packed: u8) -> Result<Vec<[u8; 4]>, String> {
        if packed & 0x80 == 0 {
            return Ok(Vec::new());
        }
        let count = 1usize << ((packed & 7) + 1);
        // 每色 3 字节，补不透明的 alpha
        Ok(self
            // 读取 3 倍颜色数的字节
            .take(count * 3)?
            .as_chunks::<3>()
            .0
            .iter()
            // 补上不透明的 Alpha
            .map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect())
    }

    fn extension(&mut self, control: &mut Control, plays: &mut Option<u32>) -> Result<(), String> {
        match self.byte()? {
            0xf9 => {
                // 图形控制块长度必须为 4
                if self.byte()? != 4 {
                    // 否则报错
                    return Err("GIF图形控制块长度无效".into());
                }
                let packed = self.byte()?;
                let delay = self.word()?;
                let transparent = self.byte()?;
                // 终止符与处置方式必须有效
                if self.byte()? != 0 || (packed >> 2) & 7 > 3 {
                    return Err("GIF图形控制块无效".into());
                }
                *control = Control {
                    delay,
                    // 处置方式取自标志位
                    disposal: (packed >> 2) & 7,
                    // 标志位最低位为 1 时有透明色
                    transparent: (packed & 1 != 0).then_some(transparent),
                };
            }
            // 应用扩展：只识别循环次数
            0xff => {
                // 应用扩展：名称长度
                let count = usize::from(self.byte()?);
                let name = self.take(count)?;
                let data = self.blocks()?;
                if name == b"NETSCAPE2.0" || name == b"ANIMEXTS1.0" {
                    // 数据格式须为三字节且首字节为 1
                    if data.len() != 3 || data[0] != 1 {
                        // 否则报错
                        return Err("GIF循环扩展无效".into());
                    }
                    // 重复次数（小端 16 位）
                    let repeat = u16::from_le_bytes([data[1], data[2]]);
                    // 重复 0 表示无限循环；否则总次数为重复次数加一
                    *plays = (repeat != 0).then_some(u32::from(repeat) + 1);
                }
            }
            0x01 => return Err("GIF文本图形扩展不受支持".into()),
            _ => {
                // 跳过其他扩展的数据
                self.blocks()?;
            }
        }
        Ok(())
    }
}

// 校验画布尺寸并返回 RGBA 字节数
fn canvas_bytes(width: u16, height: u16) -> Result<usize, String> {
    let pixels = usize::from(width) * usize::from(height);
    // 尺寸为零、单边或像素总数超限都拒绝
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        return Err("GIF尺寸超过解码限额".into());
    }
    // 返回画布字节数（RGBA）
    Ok(pixels * 4)
}

// 把一帧的索引按调色板合成到画布上，处理隔行与透明
fn compose(
    canvas: &mut [u8],
    screen_width: u16,
    rect: [u16; 4],
    indices: &[u8],
    palette: &[[u8; 4]],
    control: Control,
    interlaced: bool,
) -> Result<(), String> {
    // 解出矩形的左、上、宽、高
    let [left, top, width, height] = rect.map(usize::from);
    // 隔行：按 0/8、4/8、2/4、1/2 四趟的顺序排列行
    let rows: Vec<usize> = if interlaced {
        // 交错行的四趟扫描（起始行与步长）
        [(0, 8), (4, 8), (2, 4), (1, 2)]
            .into_iter()
            .flat_map(|(start, step)| (start..height).step_by(step))
            .collect()
    // 否则分支
    } else {
        // 非交错则按顺序
        (0..height).collect()
    };
    for (source_row, row) in rows.into_iter().enumerate() {
        for x in 0..width {
            // 取该像素的调色板索引
            let index = indices[source_row * width + x];
            // 透明像素保留画布原色
            if control.transparent == Some(index) {
                continue;
            }
            // 取调色板颜色，越界则报错
            let color = palette.get(usize::from(index)).ok_or("GIF颜色索引越界")?;
            // 计算像素在画布里的偏移
            let offset = ((top + row) * usize::from(screen_width) + left + x) * 4;
            canvas[offset..offset + 4].copy_from_slice(color);
        }
    }
    Ok(())
}

// 逐帧解码：每合成一帧调用 emit，不一次性保存所有帧
pub(super) fn decode_each(
    bytes: &[u8],
    mut emit: impl FnMut(&[u8], u32, u16, u16) -> Result<(), String>,
) -> Result<AnimationInfo, String> {
    let mut reader = Reader { bytes, position: 0 };
    // 文件头必须是 GIF87a 或 GIF89a
    if !matches!(reader.take(6)?, b"GIF87a" | b"GIF89a") {
        return Err("GIF文件头无效".into());
    }
    let width = reader.word()?;
    let height = reader.word()?;
    let length = canvas_bytes(width, height)?;
    let packed = reader.byte()?;
    let background = usize::from(reader.byte()?);
    // 像素宽高比不影响逻辑画布，直接跳过
    reader.byte()?; // 像素宽高比不改变逻辑画布约定。
    let global_palette = reader.palette(packed)?;
    // 背景色取自全局调色板，缺失则透明
    let background = global_palette.get(background).copied().unwrap_or([0; 4]);
    let mut canvas = vec![0; length];
    let mut frame_count = 0;
    let mut control = Control::default();
    // 没有循环扩展时只播放一次
    let mut plays = Some(1);
    loop {
        match reader.byte()? {
            // 文件结束：至少要有一帧
            0x3b => {
                if frame_count == 0 {
                    return Err("GIF没有图像帧".into());
                }
                return Ok(AnimationInfo {
                    width,
                    height,
                    plays,
                });
            }
            0x21 => reader.extension(&mut control, &mut plays)?,
            0x2c => {
                // 帧数与总像素处理量受限
                if frame_count >= MAX_FRAMES || frame_count + 1 > MAX_FRAME_PIXELS / (length / 4) {
                    // 帧数或总处理像素超过限额
                    return Err("GIF帧数或总像素处理量超过限额".into());
                }
                let rect = [
                    reader.word()?,
                    reader.word()?,
                    reader.word()?,
                    reader.word()?,
                ];
                // 帧矩形的左、上、宽、高
                let [left, top, frame_width, frame_height] = rect;
                // 矩形不能为空或越出画布
                if frame_width == 0
                    || frame_height == 0
                    || u32::from(left) + u32::from(frame_width) > u32::from(width)
                    || u32::from(top) + u32::from(frame_height) > u32::from(height)
                {
                    return Err("GIF帧矩形越界".into());
                }
                let packed = reader.byte()?;
                // 局部调色板优先，否则用全局
                let local_palette = reader.palette(packed)?;
                // 优先使用局部调色板
                let palette = if local_palette.is_empty() {
                    // 否则用全局调色板
                    &global_palette
                // 否则分支
                } else {
                    &local_palette
                };
                if palette.is_empty() {
                    return Err("GIF缺少颜色表".into());
                }
                let minimum = reader.byte()?;
                // 读取压缩的图像数据
                let data = reader.blocks()?;
                let indices = lzw::decode(
                    &data,
                    minimum,
                    usize::from(frame_width) * usize::from(frame_height),
                )?;
                // 清除色：有透明色则全透明，否则背景色
                let clear = if control.transparent.is_some() {
                    [0; 4]
                // 否则分支
                } else {
                    background
                };
                // 第一帧前用清除色填满画布
                if frame_count == 0 {
                    for pixel in canvas.as_chunks_mut::<4>().0 {
                        pixel.copy_from_slice(&clear);
                    }
                }
                // 处置方式 3：先保存画布，以便绘制后恢复
                let saved = (control.disposal == 3).then(|| canvas.clone());
                compose(
                    &mut canvas,
                    width,
                    rect,
                    &indices,
                    palette,
                    control,
                    // 标志位里的交错标记
                    packed & 0x40 != 0,
                )?;
                // 延迟小于 2/100 秒按 100 毫秒处理，其余乘 10
                let duration_ms = if control.delay < 2 {
                    // 默认延迟 100 毫秒
                    100
                // 否则分支
                } else {
                    // 延迟单位是 10 毫秒
                    u32::from(control.delay) * 10
                };
                // 输出这一帧的完整画布
                emit(&canvas, duration_ms, width, height)?;
                frame_count += 1;
                // 处置方式 2：把本帧区域恢复为清除色
                if control.disposal == 2 {
                    for y in top..top + frame_height {
                        for x in left..left + frame_width {
                            let offset = (usize::from(y) * usize::from(width) + usize::from(x)) * 4;
                            canvas[offset..offset + 4].copy_from_slice(&clear);
                        }
                    }
                // 处置方式 3：恢复到绘制前的画布
                } else if let Some(saved) = saved {
                    // 恢复到绘制前的画布
                    canvas = saved;
                }
                // 控制信息只对紧随其后的一帧有效
                control = Control::default();
            }
            _ => return Err("GIF块标记无效".into()),
        }
    }
}

#[cfg(test)]
// 测试用：一次性解码出所有帧
pub(super) fn decode(bytes: &[u8]) -> Result<Animation, String> {
    let mut frames = Vec::new();
    let info = decode_each(bytes, |pixels, duration_ms, _, _| {
        frames.push(Frame {
            pixels: pixels.to_vec(),
            duration_ms,
        });
        Ok(())
    })?;
    Ok(Animation {
        width: info.width,
        height: info.height,
        plays: info.plays,
        frames,
    })
}

#[cfg(test)]
#[path = "decode_test.rs"]
mod decode_test;
