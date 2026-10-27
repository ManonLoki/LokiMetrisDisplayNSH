/** 原生图片解码边界；TSX 仅消费准备好的静态帧，不解析 GIF 或 JSON。 */
import { ImageFrames } from "loki-metris-mobile-display/native";
import type { ImageAsset } from "nsh";
import type { ImageSequence } from "./image-playback";

export interface PreparedImage extends ImageSequence {
  animated: boolean;
  width: number;
  height: number;
}

export async function prepareImage(source: string | ImageAsset): Promise<PreparedImage> {
  const url = typeof source === "string" ? source : source.url;
  // 产品图片来自已校验的私有缓存；其他 NSH 资源仍交给官方静态图片组件。
  if (!url.startsWith("file:///") && !url.startsWith("data:image/gif;base64,")) {
    return { animated: false, width: 0, height: 0, plays: 1, frames: [] };
  }
  return JSON.parse(await ImageFrames.prepare(url)) as PreparedImage;
}
