/** 取自源 Android App.kt / CyberMotion.kt 深色方案的颜色。 */
// 深色主题色板
export const cyberColors = {
  // 画布底色
  canvas: "#080D17",
  // 卡片表面色
  surface: "#111D2D",
  // 表面容器色
  surfaceContainer: "#17243A",
  // 主文字色
  ink: "#E9F2FF",
  // 弱化文字色
  muted: "#96ABC5",
  // 主强调色（青）
  cyan: "#67E8F9",
  // 次强调色（紫）
  violet: "#AE8CFF",
  // 主色上的前景色
  onPrimary: "#06252E",
  // 次色上的前景色
  onSecondary: "#21103B",
  // 主色容器
  primaryContainer: "#123D50",
  // 次色容器
  secondaryContainer: "#2C244A",
  // 次色容器上的前景色
  onSecondaryContainer: "#DCCEFF",
  // 轮廓线
  outline: "#60849C",
  // 弱轮廓线
  outlineVariant: "#244052",
  // 错误色
  error: "#FF8B9C",
  // 错误色上的前景色
  onError: "#420E1C",
  // 未连接状态色
  disconnected: "#8FA4BC",
// 常量结束
} as const;

/** 共用动效尺寸；Token 绕边沿用原 iOS CyberSurface 的八秒周期。 */
// 动效参数集合
export const cyberMotion = {
  // 背景网格间距
  gridStep: 24,
  // 网格线宽
  gridStroke: 0.5,
  // 边缘光圆角
  edgeRadius: 18,
  // 边缘光线宽
  edgeStroke: 1,
  // 边缘光绕行周期（毫秒）
  edgeDuration: 8_000,
  // 斜向扫描线宽
  diagonalStroke: 22,
  // 连接指示点尺寸
  connectionSize: 9,
  // 连接指示点圆角
  connectionRadius: 2.5,
  // 连接指示点呼吸周期
  connectionDuration: 1_800,
  // 入场时长
  arrivalDuration: 420,
  // 入场错峰间隔
  arrivalStagger: 65,
  // 入场位移
  arrivalOffset: 18,
  // 入场起始缩放
  arrivalScale: 0.98,
  // 按压动画时长
  pressDuration: 120,
  // 按压缩放
  pressScale: 0.96,
  // 数字淡入时长
  numberFadeIn: 240,
  // 数字滑入时长
  numberSlideIn: 300,
  // 数字淡出时长
  numberFadeOut: 150,
  // 数字滑出时长
  numberSlideOut: 220,
// 常量结束
} as const;

// 给颜色附加透明度：限制到 0–1 后转为两位十六进制
export function cyberAlpha(color: string, opacity: number): string {
  // 把透明度限制到 0 到 1 并转为两位十六进制
  return `${color}${Math.round(Math.max(0, Math.min(1, opacity)) * 255).toString(16).padStart(2, "0")}`;
}
