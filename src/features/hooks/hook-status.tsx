// 引入画布、文本、视图
import { Canvas, Text, View } from "nsh";
// 引入状态文案
import { Rules } from "loki-metris-mobile-display/native";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

// 根据行为状态选择色调：运行青色、询问黄色、错误红色、其余灰色
export function hookStatusTone(behavior: string): string {
  // 按行为取颜色
  return behavior === "running" ? color.cyan : behavior === "asking" ? "#F4C675"
    // 否则按行为取颜色
    : behavior === "error" ? color.error : color.muted;
}

// 状态徽标：圆点加文案，外圈描边
export function HookStatus(props: { behavior: string; compact?: boolean }) {
  // 当前色调
  const tone = () => hookStatusTone(props.behavior);
  // 徽标容器，compact 时使用更小的内边距
  return <View style={{ position: "relative", flexDirection: "row", gap: 4, alignItems: "center", alignSelf: "flex-start", borderRadius: 8,
    // 背景与内边距
    backgroundColor: cyberAlpha(tone(), 0.1), paddingHorizontal: props.compact ? 6 : 9, paddingVertical: props.compact ? 3 : 5 }}>
    {/* 用画布绘制外圈描边，不拦截事件、对无障碍隐藏 */}
    <Canvas pointerEvents="none" accessible={false} accessibilityElementsHidden importantForAccessibility="no-hide-descendants"
      // 样式：铺满
      style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} draw={(cx, { width: w, height: h }) => {
        // 尺寸过小时不绘制
        if (w <= 1 || h <= 1) return;
        // 以色调的低透明度画圆角描边
        cx.strokeStyle = cyberAlpha(tone(), 0.22); cx.lineWidth = 0.75;
        // 描出圆角边框
        cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, Math.min(7.5, h / 2)); cx.stroke();
      // 绘制回调结束
      }} />
    {/* 状态圆点 */}
    <View accessible={false} style={{ width: 4, height: 4, borderRadius: 2, backgroundColor: tone() }} />
    {/* 状态文字 */}
    <Text numberOfLines={1} style={{ color: tone(), fontSize: props.compact ? 10 : 11,
      // 行高与字重
      lineHeight: props.compact ? 14 : 16, fontWeight: 500 }}>{Rules.hooksLabel(props.behavior)}</Text>
  </View>;
}
