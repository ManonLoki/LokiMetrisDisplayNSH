/** Colors from the source Android App.kt / CyberMotion.kt dark scheme. */
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
  edgeDuration: 8_000,
  diagonalStroke: 22,
  connectionSize: 9,
  connectionRadius: 2.5,
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

export function cyberAlpha(color: string, opacity: number): string {
  return `${color}${Math.round(Math.max(0, Math.min(1, opacity)) * 255).toString(16).padStart(2, "0")}`;
}
