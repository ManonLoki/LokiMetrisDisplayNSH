// 引入存储
import { Storage } from "nsh";
// 引入边界与尺寸类型
import type { Bounds, Size } from "nsh";

/** 测试专用观测器。产品入口不把布局和图片解码计数写入磁盘。 */
// 创建测试观测器，产品入口不使用
export function observations() {
  // 各卡片的布局矩形
  const frames: Record<string, { x: number; y: number; width: number; height: number }> = {};
  // 各图片的解码尺寸
  const images: Record<string, Size> = {};
  return {
    // 记录布局矩形，数值不变时不写存储
    onFrame(id: "usage" | "hook", bounds: Bounds) {
      // 取整
      const next = { x: Math.round(bounds.origin.x), y: Math.round(bounds.origin.y),
        width: Math.round(bounds.size.width), height: Math.round(bounds.size.height) };
      if (JSON.stringify(frames[id]) === JSON.stringify(next)) return;
      frames[id] = next;
      void Storage.setItem("loki.frames", JSON.stringify(frames));
    },
    // 记录图片尺寸，不变则忽略
    onImageLoad(hash: string, size: Size) {
      if (images[hash]?.width === size.width && images[hash]?.height === size.height) return;
      images[hash] = size;
      void Storage.setItem("loki.imageLoads", JSON.stringify(images));
    },
  };
}
