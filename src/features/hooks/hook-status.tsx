import { Canvas, Text, View } from "nsh";
import { hookLabel } from "./paging";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

export function hookStatusTone(behavior: string): string {
  return behavior === "running" ? color.cyan : behavior === "asking" ? "#F4C675"
    : behavior === "error" ? color.error : color.muted;
}

export function HookStatus(props: { behavior: string; compact?: boolean }) {
  const tone = () => hookStatusTone(props.behavior);
  return <View style={{ position: "relative", flexDirection: "row", gap: 4, alignItems: "center", alignSelf: "flex-start", borderRadius: 8,
    backgroundColor: cyberAlpha(tone(), 0.1), paddingHorizontal: props.compact ? 6 : 9, paddingVertical: props.compact ? 3 : 5 }}>
    <Canvas pointerEvents="none" accessible={false} accessibilityElementsHidden importantForAccessibility="no-hide-descendants"
      style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} draw={(cx, { width: w, height: h }) => {
        if (w <= 1 || h <= 1) return;
        cx.strokeStyle = cyberAlpha(tone(), 0.22); cx.lineWidth = 0.75;
        cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, Math.min(7.5, h / 2)); cx.stroke();
      }} />
    <View accessible={false} style={{ width: 4, height: 4, borderRadius: 2, backgroundColor: tone() }} />
    <Text numberOfLines={1} style={{ color: tone(), fontSize: props.compact ? 10 : 11,
      lineHeight: props.compact ? 14 : 16, fontWeight: 500 }}>{hookLabel(props.behavior)}</Text>
  </View>;
}
