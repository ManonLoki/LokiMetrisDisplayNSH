import { Storage } from "nsh";
import type { Bounds, Size } from "nsh";

/** 测试专用观测器。产品入口不把布局和图片解码计数写入磁盘。 */
export function observations() {
  const frames: Record<string, { x: number; y: number; width: number; height: number }> = {};
  const images: Record<string, Size> = {};
  return {
    onFrame(id: "usage" | "hook", bounds: Bounds) {
      const next = { x: Math.round(bounds.origin.x), y: Math.round(bounds.origin.y),
        width: Math.round(bounds.size.width), height: Math.round(bounds.size.height) };
      if (JSON.stringify(frames[id]) === JSON.stringify(next)) return;
      frames[id] = next;
      void Storage.setItem("loki.frames", JSON.stringify(frames));
    },
    onImageLoad(hash: string, size: Size) {
      if (images[hash]?.width === size.width && images[hash]?.height === size.height) return;
      images[hash] = size;
      void Storage.setItem("loki.imageLoads", JSON.stringify(images));
    },
  };
}
