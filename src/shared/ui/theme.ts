/** 取自源 Android App.kt / CyberMotion.kt 深色方案的颜色。 */
export const cyberColors = {
  canvas: "#080D17",
  surface: "#111D2D",
  surfaceContainer: "#17243A",
  ink: "#E9F2FF",
  muted: "#96ABC5",
  cyan: "#67E8F9",
  violet: "#AE8CFF",
  onPrimary: "#06252E",
  onSecondary: "#21103B",
  primaryContainer: "#123D50",
  secondaryContainer: "#2C244A",
  // 次色容器上的前景色
  onSecondaryContainer: "#DCCEFF",
  outline: "#60849C",
  outlineVariant: "#244052",
  error: "#FF8B9C",
  onError: "#420E1C",
  disconnected: "#8FA4BC",
} as const;

/** 共用动效尺寸；Token 绕边沿用原 iOS CyberSurface 的八秒周期。 */
export const cyberMotion = {
  gridStep: 24,
  gridStroke: 0.5,
  edgeRadius: 18,
  edgeStroke: 1,
  // 边缘光绕行周期（毫秒）
  edgeDuration: 8_000,
  diagonalStroke: 22,
  connectionSize: 9,
  connectionRadius: 2.5,
  // 连接指示点呼吸周期
  connectionDuration: 1_800,
  arrivalDuration: 420,
  arrivalStagger: 65,
  arrivalOffset: 18,
  arrivalScale: 0.98,
  pressDuration: 120,
  pressScale: 0.96,
  numberFadeIn: 240,
  numberSlideIn: 300,
  numberFadeOut: 150,
  numberSlideOut: 220,
} as const;

// 给颜色附加透明度：限制到 0–1 后转为两位十六进制
export function cyberAlpha(color: string, opacity: number): string {
  // 把透明度限制到 0 到 1 并转为两位十六进制
  return `${color}${Math.round(Math.max(0, Math.min(1, opacity)) * 255).toString(16).padStart(2, "0")}`;
}
