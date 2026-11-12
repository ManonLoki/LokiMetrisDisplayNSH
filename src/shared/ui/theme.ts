import { token } from "nsh";

/** 颜色的唯一来源是 nsh.json 的 `theme`，这里只给界面代码起语义别名；界面代码不写十六进制颜色。 */
export const cyberColors = {
  get canvas() { return token("background"); },
  get surface() { return token("card"); },
  get surfaceContainer() { return token("surface-container"); },
  get ink() { return token("foreground"); },
  get muted() { return token("muted-foreground"); },
  get cyan() { return token("primary"); },
  get violet() { return token("secondary"); },
  get onPrimary() { return token("primary-foreground"); },
  get onSecondary() { return token("secondary-foreground"); },
  get primaryContainer() { return token("primary-container"); },
  get secondaryContainer() { return token("secondary-container"); },
  get onSecondaryContainer() { return token("on-secondary-container"); },
  get outline() { return token("outline"); },
  get outlineVariant() { return token("border"); },
  get error() { return token("destructive"); },
  get onError() { return token("on-error"); },
  get disconnected() { return token("disconnected"); },
  get usageSurface() { return token("usage-surface"); },
  get usageMuted() { return token("usage-muted"); },
  get asking() { return token("asking"); },
  get switchOff() { return token("switch-off"); },
  get switchOnThumb() { return token("switch-on-thumb"); },
  get success() { return token("success"); },
  get warning() { return token("warning"); },
  get danger() { return token("danger"); },
};

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

// 给颜色附加透明度：token 以 #rrggbbaa 给出，替换其透明度字节
export function cyberAlpha(color: string, opacity: number): string {
  return `${color.slice(0, 7)}${Math.round(Math.max(0, Math.min(1, opacity)) * 255).toString(16).padStart(2, "0")}`;
}
