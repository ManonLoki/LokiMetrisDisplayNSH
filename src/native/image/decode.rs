//! GIF block parsing and full-canvas RGBA composition before PNG encoding.
use super::{MAX_FRAME_PIXELS, MAX_FRAMES, MAX_PIXELS, MAX_SIDE, lzw};

#[cfg(test)]
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

pub(super) struct AnimationInfo {
    pub width: u16,
    pub height: u16,
    pub plays: Option<u32>,
}

#[derive(Clone, Copy, Default)]
struct Control {
    delay: u16,
    disposal: u8,
    transparent: Option<u8>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self.position.checked_add(count).ok_or("GIF长度溢出")?;
        let value = self.bytes.get(self.position..end).ok_or("GIF数据截断")?;
        self.position = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn word(&mut self) -> Result<u16, String> {
        let value = self.take(2)?;
        Ok(u16::from_le_bytes([value[0], value[1]]))
    }

    fn blocks(&mut self) -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        loop {
            let count = usize::from(self.byte()?);
            if count == 0 {
                return Ok(data);
            }
            data.extend_from_slice(self.take(count)?);
        }
    }

    fn palette(&mut self, packed: u8) -> Result<Vec<[u8; 4]>, String> {
        if packed & 0x80 == 0 {
            return Ok(Vec::new());
        }
        let count = 1usize << ((packed & 7) + 1);
        Ok(self
            .take(count * 3)?
            .as_chunks::<3>()
            .0
            .iter()
            .map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect())
    }

    fn extension(&mut self, control: &mut Control, plays: &mut Option<u32>) -> Result<(), String> {
        match self.byte()? {
            0xf9 => {
                if self.byte()? != 4 {
                    return Err("GIF图形控制块长度无效".into());
                }
                let packed = self.byte()?;
                let delay = self.word()?;
                let transparent = self.byte()?;
                if self.byte()? != 0 || (packed >> 2) & 7 > 3 {
                    return Err("GIF图形控制块无效".into());
                }
                *control = Control {
                    delay,
                    disposal: (packed >> 2) & 7,
                    transparent: (packed & 1 != 0).then_some(transparent),
                };
            }
            0xff => {
                let count = usize::from(self.byte()?);
                let name = self.take(count)?;
                let data = self.blocks()?;
                if name == b"NETSCAPE2.0" || name == b"ANIMEXTS1.0" {
                    if data.len() != 3 || data[0] != 1 {
                        return Err("GIF循环扩展无效".into());
                    }
                    let repeat = u16::from_le_bytes([data[1], data[2]]);
                    *plays = (repeat != 0).then_some(u32::from(repeat) + 1);
                }
            }
            0x01 => return Err("GIF文本图形扩展不受支持".into()),
            _ => {
                self.blocks()?;
            }
        }
        Ok(())
    }
}

fn canvas_bytes(width: u16, height: u16) -> Result<usize, String> {
    let pixels = usize::from(width) * usize::from(height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        return Err("GIF尺寸超过解码限额".into());
    }
    Ok(pixels * 4)
}

fn compose(
    canvas: &mut [u8],
    screen_width: u16,
    rect: [u16; 4],
    indices: &[u8],
    palette: &[[u8; 4]],
    control: Control,
    interlaced: bool,
) -> Result<(), String> {
    let [left, top, width, height] = rect.map(usize::from);
    let rows: Vec<usize> = if interlaced {
        [(0, 8), (4, 8), (2, 4), (1, 2)]
            .into_iter()
            .flat_map(|(start, step)| (start..height).step_by(step))
            .collect()
    } else {
        (0..height).collect()
    };
    for (source_row, row) in rows.into_iter().enumerate() {
        for x in 0..width {
            let index = indices[source_row * width + x];
            if control.transparent == Some(index) {
                continue;
            }
            let color = palette.get(usize::from(index)).ok_or("GIF颜色索引越界")?;
            let offset = ((top + row) * usize::from(screen_width) + left + x) * 4;
            canvas[offset..offset + 4].copy_from_slice(color);
        }
    }
    Ok(())
}

pub(super) fn decode_each(
    bytes: &[u8],
    mut emit: impl FnMut(&[u8], u32, u16, u16) -> Result<(), String>,
) -> Result<AnimationInfo, String> {
    let mut reader = Reader { bytes, position: 0 };
    if !matches!(reader.take(6)?, b"GIF87a" | b"GIF89a") {
        return Err("GIF文件头无效".into());
    }
    let width = reader.word()?;
    let height = reader.word()?;
    let length = canvas_bytes(width, height)?;
    let packed = reader.byte()?;
    let background = usize::from(reader.byte()?);
    reader.byte()?; // Pixel aspect does not change the logical canvas contract.
    let global_palette = reader.palette(packed)?;
    let background = global_palette.get(background).copied().unwrap_or([0; 4]);
    let mut canvas = vec![0; length];
    let mut frame_count = 0;
    let mut control = Control::default();
    let mut plays = Some(1);
    loop {
        match reader.byte()? {
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
                if frame_count >= MAX_FRAMES || frame_count + 1 > MAX_FRAME_PIXELS / (length / 4) {
                    return Err("GIF帧数或总像素处理量超过限额".into());
                }
                let rect = [
                    reader.word()?,
                    reader.word()?,
                    reader.word()?,
                    reader.word()?,
                ];
                let [left, top, frame_width, frame_height] = rect;
                if frame_width == 0
                    || frame_height == 0
                    || u32::from(left) + u32::from(frame_width) > u32::from(width)
                    || u32::from(top) + u32::from(frame_height) > u32::from(height)
                {
                    return Err("GIF帧矩形越界".into());
                }
                let packed = reader.byte()?;
                let local_palette = reader.palette(packed)?;
                let palette = if local_palette.is_empty() {
                    &global_palette
                } else {
                    &local_palette
                };
                if palette.is_empty() {
                    return Err("GIF缺少颜色表".into());
                }
                let minimum = reader.byte()?;
                let data = reader.blocks()?;
                let indices = lzw::decode(
                    &data,
                    minimum,
                    usize::from(frame_width) * usize::from(frame_height),
                )?;
                let clear = if control.transparent.is_some() {
                    [0; 4]
                } else {
                    background
                };
                if frame_count == 0 {
                    for pixel in canvas.as_chunks_mut::<4>().0 {
                        pixel.copy_from_slice(&clear);
                    }
                }
                let saved = (control.disposal == 3).then(|| canvas.clone());
                compose(
                    &mut canvas,
                    width,
                    rect,
                    &indices,
                    palette,
                    control,
                    packed & 0x40 != 0,
                )?;
                let duration_ms = if control.delay < 2 {
                    100
                } else {
                    u32::from(control.delay) * 10
                };
                emit(&canvas, duration_ms, width, height)?;
                frame_count += 1;
                if control.disposal == 2 {
                    for y in top..top + frame_height {
                        for x in left..left + frame_width {
                            let offset = (usize::from(y) * usize::from(width) + usize::from(x)) * 4;
                            canvas[offset..offset + 4].copy_from_slice(&clear);
                        }
                    }
                } else if let Some(saved) = saved {
                    canvas = saved;
                }
                control = Control::default();
            }
            _ => return Err("GIF块标记无效".into()),
        }
    }
}

#[cfg(test)]
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
