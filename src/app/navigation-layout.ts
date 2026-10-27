/** 导航断点属于应用外壳，业务组件不依赖该布局策略。 */
export const WIDE_MIN_PT = 700;

export function navPlacement(width: number): "bottom" | "side" {
  return width >= WIDE_MIN_PT ? "side" : "bottom";
}
