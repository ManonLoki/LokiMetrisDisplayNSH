/** 原生图片解码的结果形状;解码与来源判定都在 Rust 的 ImageFrames.prepare 中完成。 */
import type { ImageSequence } from "./image-playback";

// 准备好的图片：是否动图、尺寸与帧序列
export interface PreparedImage extends ImageSequence {
  // 是否为动图
  animated: boolean;
  // 宽度
  width: number;
  // 高度
  height: number;
}
