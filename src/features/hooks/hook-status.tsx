import { Canvas, Text, View } from "nsh";
import { Rules } from "loki-metris-mobile-display/native";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

// 根据行为状态选择色调：运行青色、询问黄色、错误红色、其余灰色
export function hookStatusTone(behavior: string): string {
  return behavior === "running" ? color.cyan : behavior === "asking" ? color.asking
    // 否则按行为取颜色
    : behavior === "error" ? color.error : color.muted;
}

// 状态徽标：圆点加文案，外圈描边
export function HookStatus(props: { behavior: string; compact?: boolean }) {
  const tone = () => hookStatusTone(props.behavior);
  // 徽标容器，compact 时使用更小的内边距
  return <View style={{ position: "relative", flexDirection: "row", gap: 4, alignItems: "center", alignSelf: "flex-start", borderRadius: 8,
    backgroundColor: cyberAlpha(tone(), 0.1), paddingHorizontal: props.compact ? 6 : 9, paddingVertical: props.compact ? 3 : 5 }}>
    {/* 用画布绘制外圈描边，不拦截事件、对无障碍隐藏 */}
    <Canvas accessible={false} accessibilityElementsHidden importantForAccessibility="no-hide-descendants"
      style={{ pointerEvents: "none", position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} draw={(cx, { width: w, height: h }) => {
        if (w <= 1 || h <= 1) return;
        // 以色调的低透明度画圆角描边
        cx.strokeStyle = cyberAlpha(tone(), 0.22); cx.lineWidth = 0.75;
        cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, Math.min(7.5, h / 2)); cx.stroke();
      }} />
    {/* 状态圆点 */}
    <View accessible={false} style={{ width: 4, height: 4, borderRadius: 2, backgroundColor: tone() }} />
    {/* 状态文字 */}
    <Text style={{ numberOfLines: 1, color: tone(), fontSize: props.compact ? 10 : 11,
      lineHeight: props.compact ? 14 : 16, fontWeight: 500 }}>{Rules.hooksLabel(props.behavior)}</Text>
  </View>;
}
