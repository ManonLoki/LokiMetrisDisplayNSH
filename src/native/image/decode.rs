//! GIF 块解析,以及在 PNG 编码之前的全画布 RGBA 合成。
// 引入各项上限与 LZW 解压
use super::{MAX_FRAME_PIXELS, MAX_FRAMES, MAX_PIXELS, MAX_SIDE, lzw};

#[cfg(test)]
// 测试用：解码后的一帧
pub(super) struct Frame {
    // 像素数据
    pub pixels: Vec<u8>,
    // 持续毫秒数
    pub duration_ms: u32,
}

#[cfg(test)]
// 测试用：完整动画
pub(super) struct Animation {
    // 宽度
    pub width: u16,
    // 高度
    pub height: u16,
    // 播放次数
    pub plays: Option<u32>,
    // 各帧
    pub frames: Vec<Frame>,
}

// 动画的基本信息：尺寸与播放次数
pub(super) struct AnimationInfo {
    // 宽度
    pub width: u16,
    // 高度
    pub height: u16,
    // 播放次数
    pub plays: Option<u32>,
}

#[derive(Clone, Copy, Default)]
// 图形控制扩展：延迟、处置方式与透明色
struct Control {
    // 帧延迟
    delay: u16,
    // 处置方式
    disposal: u8,
    // 透明色索引
    transparent: Option<u8>,
}

// 带边界检查的字节读取器
struct Reader<'a> {
    // 待读取的字节
    bytes: &'a [u8],
    // 读取位置
    position: usize,
}

// 读取器的方法
impl<'a> Reader<'a> {
    // 取出 count 字节，越界报“数据截断”
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        // 计算读取终点，溢出则报错
        let end = self.position.checked_add(count).ok_or("GIF长度溢出")?;
        // 取出区间，数据不足则报截断
        let value = self.bytes.get(self.position..end).ok_or("GIF数据截断")?;
        // 前进读取位置
        self.position = end;
        // 返回读到的字节
        Ok(value)
    }

    // 读一个字节
    fn byte(&mut self) -> Result<u8, String> {
        // 读取一个字节
        Ok(self.take(1)?[0])
    }

    // 读小端 16 位
    fn word(&mut self) -> Result<u16, String> {
        // 读取两个字节
        let value = self.take(2)?;
        // 按小端组成 16 位整数
        Ok(u16::from_le_bytes([value[0], value[1]]))
    }

    // 读取由子块组成的数据，遇到长度 0 的块结束
    fn blocks(&mut self) -> Result<Vec<u8>, String> {
        // 累积数据块
        let mut data = Vec::new();
        // 循环读取子块
        loop {
            // 子块长度
            let count = usize::from(self.byte()?);
            // 长度为 0 表示结束
            if count == 0 {
                // 返回累积的数据
                return Ok(data);
            }
            // 追加该子块内容
            data.extend_from_slice(self.take(count)?);
        }
    }

    // 读取调色板：由打包字节决定是否存在与大小
    fn palette(&mut self, packed: u8) -> Result<Vec<[u8; 4]>, String> {
        // 无调色板
        if packed & 0x80 == 0 {
            // 没有内容
            return Ok(Vec::new());
        }
        // 颜色数
        let count = 1usize << ((packed & 7) + 1);
        // 每色 3 字节，补不透明的 alpha
        Ok(self
            // 读取 3 倍颜色数的字节
            .take(count * 3)?
            // 按三字节分组
            .as_chunks::<3>()
            // 取分组部分
            .0
            // 遍历
            .iter()
            // 补上不透明的 Alpha
            .map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            // 收集为调色板
            .collect())
    }

    // 处理扩展块
    fn extension(&mut self, control: &mut Control, plays: &mut Option<u32>) -> Result<(), String> {
        // 按扩展标签处理
        match self.byte()? {
            // 图形控制扩展
            0xf9 => {
                // 图形控制块长度必须为 4
                if self.byte()? != 4 {
                    // 否则报错
                    return Err("GIF图形控制块长度无效".into());
                }
                // 打包的标志位
                let packed = self.byte()?;
                // 帧延迟
                let delay = self.word()?;
                // 透明色索引
                let transparent = self.byte()?;
                // 终止符与处置方式必须有效
                if self.byte()? != 0 || (packed >> 2) & 7 > 3 {
                    // 标志位不合法
                    return Err("GIF图形控制块无效".into());
                }
                *control = Control {
                    // 延迟
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
                // 名称
                let name = self.take(count)?;
                // 数据子块
                let data = self.blocks()?;
                // 是循环扩展
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
            // 文本扩展不受支持
            0x01 => return Err("GIF文本图形扩展不受支持".into()),
            // 其他扩展直接跳过
            _ => {
                // 跳过其他扩展的数据
                self.blocks()?;
            }
        }
        // 完成
        Ok(())
    }
}

// 校验画布尺寸并返回 RGBA 字节数
fn canvas_bytes(width: u16, height: u16) -> Result<usize, String> {
    // 总像素数
    let pixels = usize::from(width) * usize::from(height);
    // 尺寸为零、单边或像素总数超限都拒绝
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        // 超过解码限额
        return Err("GIF尺寸超过解码限额".into());
    }
    // 返回画布字节数（RGBA）
    Ok(pixels * 4)
}

// 把一帧的索引按调色板合成到画布上，处理隔行与透明
fn compose(
    // 画布
    canvas: &mut [u8],
    // 画布宽度
    screen_width: u16,
    // 帧矩形
    rect: [u16; 4],
    // 调色板索引
    indices: &[u8],
    // 调色板
    palette: &[[u8; 4]],
    // 图形控制信息
    control: Control,
    // 是否交错
    interlaced: bool,
    // 返回结果类型
) -> Result<(), String> {
    // 解出矩形的左、上、宽、高
    let [left, top, width, height] = rect.map(usize::from);
    // 隔行：按 0/8、4/8、2/4、1/2 四趟的顺序排列行
    let rows: Vec<usize> = if interlaced {
        // 交错行的四趟扫描（起始行与步长）
        [(0, 8), (4, 8), (2, 4), (1, 2)]
            // 遍历四趟扫描
            .into_iter()
            // 展开为行号
            .flat_map(|(start, step)| (start..height).step_by(step))
            // 收集为列表
            .collect()
    // 否则分支
    } else {
        // 非交错则按顺序
        (0..height).collect()
    };
    // 逐行遍历
    for (source_row, row) in rows.into_iter().enumerate() {
        // 逐列遍历
        for x in 0..width {
            // 取该像素的调色板索引
            let index = indices[source_row * width + x];
            // 透明像素保留画布原色
            if control.transparent == Some(index) {
                // 透明像素不写入
                continue;
            }
            // 取调色板颜色，越界则报错
            let color = palette.get(usize::from(index)).ok_or("GIF颜色索引越界")?;
            // 计算像素在画布里的偏移
            let offset = ((top + row) * usize::from(screen_width) + left + x) * 4;
            // 写入四字节颜色
            canvas[offset..offset + 4].copy_from_slice(color);
        }
    }
    // 完成
    Ok(())
}

// 逐帧解码：每合成一帧调用 emit，不一次性保存所有帧
pub(super) fn decode_each(
    // 字节
    bytes: &[u8],
    // 每帧回调
    mut emit: impl FnMut(&[u8], u32, u16, u16) -> Result<(), String>,
    // 返回动画信息
) -> Result<AnimationInfo, String> {
    // 创建读取器
    let mut reader = Reader { bytes, position: 0 };
    // 文件头必须是 GIF87a 或 GIF89a
    if !matches!(reader.take(6)?, b"GIF87a" | b"GIF89a") {
        // 文件头无效
        return Err("GIF文件头无效".into());
    }
    // 逻辑屏幕尺寸
    let width = reader.word()?;
    // 画布高度
    let height = reader.word()?;
    // 画布字节数
    let length = canvas_bytes(width, height)?;
    // 全局调色板信息
    let packed = reader.byte()?;
    // 背景色索引
    let background = usize::from(reader.byte()?);
    // 像素宽高比不影响逻辑画布，直接跳过
    reader.byte()?; // 像素宽高比不改变逻辑画布约定。
    // 读取全局调色板
    let global_palette = reader.palette(packed)?;
    // 背景色取自全局调色板，缺失则透明
    let background = global_palette.get(background).copied().unwrap_or([0; 4]);
    // 创建画布
    let mut canvas = vec![0; length];
    // 已解帧数
    let mut frame_count = 0;
    // 当前图形控制信息
    let mut control = Control::default();
    // 没有循环扩展时只播放一次
    let mut plays = Some(1);
    // 循环读取各数据块
    loop {
        // 按块标记处理
        match reader.byte()? {
            // 文件结束：至少要有一帧
            0x3b => {
                // 遇到结束标记时
                if frame_count == 0 {
                    // 没有任何图像帧
                    return Err("GIF没有图像帧".into());
                }
                // 返回动画信息
                return Ok(AnimationInfo {
                    // 宽
                    width,
                    // 高度
                    height,
                    // 播放次数
                    plays,
                });
            }
            // 扩展块
            0x21 => reader.extension(&mut control, &mut plays)?,
            // 图像块
            0x2c => {
                // 帧数与总像素处理量受限
                if frame_count >= MAX_FRAMES || frame_count + 1 > MAX_FRAME_PIXELS / (length / 4) {
                    // 帧数或总处理像素超过限额
                    return Err("GIF帧数或总像素处理量超过限额".into());
                }
                // 帧矩形
                let rect = [
                    // 左
                    reader.word()?,
                    // 上
                    reader.word()?,
                    // 宽
                    reader.word()?,
                    // 高
                    reader.word()?,
                ];
                // 帧矩形的左、上、宽、高
                let [left, top, frame_width, frame_height] = rect;
                // 矩形不能为空或越出画布
                if frame_width == 0
                    // 或者帧高为 0
                    || frame_height == 0
                    // 或者横向越界
                    || u32::from(left) + u32::from(frame_width) > u32::from(width)
                    // 或者纵向越界
                    || u32::from(top) + u32::from(frame_height) > u32::from(height)
                {
                    // 矩形越界
                    return Err("GIF帧矩形越界".into());
                }
                // 帧的标志位
                let packed = reader.byte()?;
                // 局部调色板优先，否则用全局
                let local_palette = reader.palette(packed)?;
                // 优先使用局部调色板
                let palette = if local_palette.is_empty() {
                    // 否则用全局调色板
                    &global_palette
                // 否则分支
                } else {
                    // 局部调色板
                    &local_palette
                };
                // 两个调色板都为空
                if palette.is_empty() {
                    // 缺少颜色表
                    return Err("GIF缺少颜色表".into());
                }
                // LZW 最小码长
                let minimum = reader.byte()?;
                // 读取压缩的图像数据
                let data = reader.blocks()?;
                // 解压得到索引
                let indices = lzw::decode(
                    // 压缩数据
                    &data,
                    // 最小码宽
                    minimum,
                    // 像素总数
                    usize::from(frame_width) * usize::from(frame_height),
                    // 解压出调色板索引
                )?;
                // 清除色：有透明色则全透明，否则背景色
                let clear = if control.transparent.is_some() {
                    // 透明
                    [0; 4]
                // 否则分支
                } else {
                    // 背景色
                    background
                };
                // 第一帧前用清除色填满画布
                if frame_count == 0 {
                    // 逐像素
                    for pixel in canvas.as_chunks_mut::<4>().0 {
                        // 填充清除色
                        pixel.copy_from_slice(&clear);
                    }
                }
                // 处置方式 3：先保存画布，以便绘制后恢复
                let saved = (control.disposal == 3).then(|| canvas.clone());
                // 合成当前帧
                compose(
                    // 画布
                    &mut canvas,
                    // 画布宽度
                    width,
                    // 帧矩形
                    rect,
                    // 调色板索引
                    &indices,
                    // 调色板
                    palette,
                    // 图形控制信息
                    control,
                    // 标志位里的交错标记
                    packed & 0x40 != 0,
                    // 出错即返回
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
                // 帧数加一
                frame_count += 1;
                // 处置方式 2：把本帧区域恢复为清除色
                if control.disposal == 2 {
                    // 按行遍历该帧矩形
                    for y in top..top + frame_height {
                        // 按列遍历
                        for x in left..left + frame_width {
                            // 计算偏移
                            let offset = (usize::from(y) * usize::from(width) + usize::from(x)) * 4;
                            // 填充清除色
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
            // 未知块标记
            _ => return Err("GIF块标记无效".into()),
        }
    }
}

#[cfg(test)]
// 测试用：一次性解码出所有帧
pub(super) fn decode(bytes: &[u8]) -> Result<Animation, String> {
    // 帧列表
    let mut frames = Vec::new();
    // 逐帧解码
    let info = decode_each(bytes, |pixels, duration_ms, _, _| {
        // 保存一帧
        frames.push(Frame {
            // 像素数据
            pixels: pixels.to_vec(),
            // 持续毫秒数
            duration_ms,
        });
        // 完成
        Ok(())
        // 出错即返回
    })?;
    // 构造动画
    Ok(Animation {
        // 宽
        width: info.width,
        // 高
        height: info.height,
        // 循环次数
        plays: info.plays,
        // 各帧
        frames,
    })
}

// 测试模块
#[cfg(test)]
#[path = "decode_test.rs"]
mod decode_test;
