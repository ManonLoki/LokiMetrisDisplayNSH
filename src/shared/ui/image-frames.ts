/** 原生图片解码边界；TSX 仅消费准备好的静态帧，不解析 GIF 或 JSON。 */
// 引入原生图片帧解码模块
import { ImageFrames } from "loki-metris-mobile-display/native";
// 引入图片资源类型
import type { ImageAsset } from "nsh";
// 引入帧序列类型
import type { ImageSequence } from "./image-playback";

// 解码后的图片
export interface PreparedImage extends ImageSequence {
  // 是否为动图
  animated: boolean;
  // 像素宽度
  width: number;
  // 像素高度
  height: number;
}

// 准备图片：本地缓存的 GIF 交给原生拆帧，其余视为静态图
export async function prepareImage(source: string | ImageAsset): Promise<PreparedImage> {
  // 取得地址，兼容字符串与资源对象
  const url = typeof source === "string" ? source : source.url;
  // 产品图片来自已校验的私有缓存；其他 NSH 资源仍交给官方静态图片组件。
  // 仅处理私有缓存文件与内嵌 GIF，其他来源直接标记为静态
  if (!url.startsWith("file:///") && !url.startsWith("data:image/gif;base64,")) {
    // 静态图返回空帧序列
    return { animated: false, width: 0, height: 0, plays: 1, frames: [] };
  }
  // 交给原生解码并解析结果
  return JSON.parse(await ImageFrames.prepare(url)) as PreparedImage;
}
