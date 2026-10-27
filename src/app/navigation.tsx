import { Canvas, Text, View, type Accessor } from "nsh";
import { CyberPressable } from "../shared/ui/effects";
import { cyberColors as color } from "../shared/ui/theme";

export type DisplayPage = "panel" | "settings";

/** The source App.kt draws these icons; no icon font or replacement artwork. */
export function MenuIcon(props: { page: DisplayPage; selected: boolean }) {
  return <Canvas accessible={false} style={{ width: 21, height: 21 }} draw={(cx, { width: w, height: h }) => {
    const tint = props.selected ? color.onSecondaryContainer : color.muted;
    cx.fillStyle = tint; cx.strokeStyle = tint; cx.lineWidth = 1.8; cx.lineCap = "round";
    if (props.page === "panel") {
      const gap = 3.5, cell = (w - gap) / 2;
      for (let x = 0; x < 2; x++) for (let y = 0; y < 2; y++) {
        cx.beginPath(); cx.roundRect(x * (cell + gap), y * (cell + gap), cell, cell, 2); cx.fill();
      }
    } else {
      for (let row = 0; row < 3; row++) {
        const y = h * (0.2 + row * 0.3), x = w * (row === 1 ? 0.68 : 0.32);
        cx.beginPath(); cx.moveTo(0, y); cx.lineTo(w, y); cx.stroke();
        cx.beginPath(); cx.arc(x, y, 3, 0, Math.PI * 2); cx.fill();
      }
    }
  }} />;
}

export function BrandMark() {
  return <View accessible={false} style={{ width: 38, height: 38, borderRadius: 10, backgroundColor: color.cyan, padding: 10 }}>
    <Canvas style={{ flex: 1 }} draw={(cx, { width: w, height: h }) => {
      cx.strokeStyle = color.onPrimary; cx.fillStyle = color.onPrimary; cx.lineWidth = 3; cx.lineCap = "round";
      cx.beginPath(); cx.moveTo(0, 0); cx.lineTo(0, h); cx.lineTo(w, h); cx.stroke();
      cx.beginPath(); cx.arc(w, 2.5, 2.5, 0, Math.PI * 2); cx.fill();
    }} />
  </View>;
}

export function CyberNavigation(props: { placement: "side" | "bottom"; selected: Accessor<DisplayPage>; onSelect: (page: DisplayPage) => void }) {
  const side = props.placement === "side";
  return <View testID={side ? "nav-side" : "nav-bottom"} style={side
    ? { width: 104, height: "100%", alignItems: "center", paddingTop: 16 }
    : { width: "100%", height: 68, flexDirection: "row", backgroundColor: "#111D2DF7", alignItems: "center" }}>
    {side ? <View style={{ marginBottom: 30 }}><BrandMark /></View> : null}
    {(["panel", "settings"] as const).map((page) => () => {
      const selected = props.selected() === page;
      return <CyberPressable testID={`menu-${page}`} selected={selected} role="tab" onPress={() => props.onSelect(page)}
        style={{ ...(side ? { width: 80, height: 68, marginBottom: 12 } : { flex: 1, height: 68 }),
          alignItems: "center", justifyContent: "center", gap: 4, borderRadius: 16 }}>
        <View style={{ width: side ? 56 : 64, height: 32, borderRadius: 16,
          alignItems: "center", justifyContent: "center", backgroundColor: selected ? color.primaryContainer : "transparent" }}>
          <MenuIcon page={page} selected={selected} />
        </View>
        <Text style={{ color: selected ? color.violet : color.muted, fontSize: side ? 12 : 11, lineHeight: 16,
          fontWeight: "500" }}>{page === "panel" ? "展示台" : "设置"}</Text>
      </CyberPressable>;
    })}
  </View>;
}
