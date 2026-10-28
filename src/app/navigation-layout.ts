/** 导航断点属于应用外壳，业务组件不依赖该布局策略。 */
// 宽度达到 700pt 时使用侧边导航
export const WIDE_MIN_PT = 700;

// 根据窗口宽度决定导航栏的位置
export function navPlacement(width: number): "bottom" | "side" {
  // 宽屏放侧边，窄屏放底部
  return width >= WIDE_MIN_PT ? "side" : "bottom";
}
