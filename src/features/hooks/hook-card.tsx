import { Canvas, Text, View, type Accessor, type Bounds, type Size } from "nsh";
import type { HookItem } from "../../shared/contracts/snapshot";
import { agentDisplayName } from "../../shared/domain/agents";
import { CyberArrival, CyberPressable } from "../../shared/ui/effects";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
import { HookStatus, hookStatusTone } from "./hook-status";
import { HookImageViewport } from "./hook-image";

export function HookCard(props: { hook: Accessor<HookItem>; connected: Accessor<boolean>; width: Accessor<number>; height: Accessor<number>;
  showImage: Accessor<boolean>; active: Accessor<boolean>; onPress: () => void; onRemoteFocus?: () => void;
  onFrame?: (bounds: Bounds) => void; onImageLoad?: (hash: string, size: Size) => void }) {
  const small = () => props.width() < 260 || props.height() < 220;
  const padding = () => Math.min(small() ? 10 : 18, props.width() * 0.06, props.height() * 0.08);
  const gap = () => Math.min(small() ? 6 : 10, props.height() * 0.04);
  const tone = () => hookStatusTone(props.hook().behavior);
  return <CyberArrival order={2 + Math.min(4, props.hook().slot)} active={props.active}
    style={{ flex: 1, minWidth: 0, minHeight: 0 }} onLayout={props.onFrame}>
    <CyberPressable testID={`hook-card-${props.hook().slot}`} active={props.active} onPress={props.onPress}
      onRemoteFocus={props.onRemoteFocus}
      accessibilityLabel={`查看 ${agentDisplayName(props.hook().agent)} 的详情`}
      hoverStyle={{ borderColor: cyberAlpha(tone(), 0.48), shadowColor: tone(), shadowOpacity: 0.1, shadowRadius: 8 }}
      focusStyle={{ borderColor: cyberAlpha(tone(), 0.85), shadowColor: tone(), shadowOpacity: 0.18, shadowRadius: 9 }}
      style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0, backgroundColor: color.surface,
        borderRadius: 18, borderWidth: 1, borderColor: cyberAlpha(tone(), 0.25), padding: padding(), gap: gap() }}>
      <HookCardSurface tone={tone()} />
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: small() ? 5 : 10 }}>
        <Text numberOfLines={1} ellipsizeMode="tail" style={{ flex: 1, minWidth: 0, color: color.ink,
          fontSize: small() ? 13 : 18, lineHeight: small() ? 18 : 24, fontWeight: 600 }}>{agentDisplayName(props.hook().agent)}</Text>
        <HookStatus behavior={props.hook().behavior} compact={small()} />
      </View>
      <HookImageViewport hook={props.hook} connected={props.connected} showImage={props.showImage} active={props.active} onImageLoad={props.onImageLoad} />
    </CyberPressable>
  </CyberArrival>;
}

/** 静态光层不参与卡片测量，也不随快照启动额外动画。 */
export function HookCardSurface(props: { tone: string; quiet?: boolean }) {
  return <Canvas pointerEvents="none" accessible={false} accessibilityElementsHidden
    importantForAccessibility="no-hide-descendants"
    style={{ position: "absolute", top: 1, right: 1, bottom: 1, left: 1 }} draw={(cx, { width: w, height: h }) => {
      if (w <= 2 || h <= 2) return;
      const strength = props.quiet ? 0.45 : 1;
      const radius = Math.min(17, w / 2, h / 2);
      cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, radius); cx.clip();
      const surface = cx.createLinearGradient(0, 0, w * 0.4, h);
      surface.addColorStop(0, cyberAlpha(color.surfaceContainer, props.quiet ? 0.5 : 0.85));
      surface.addColorStop(1, cyberAlpha(color.surface, 0));
      cx.fillStyle = surface; cx.fillRect(0, 0, w, h);
      const glow = cx.createRadialGradient(0, 0, 0, 0, 0, Math.max(w, h * 0.7));
      glow.addColorStop(0, cyberAlpha(props.tone, 0.085 * strength));
      glow.addColorStop(1, cyberAlpha(props.tone, 0));
      cx.fillStyle = glow; cx.fillRect(0, 0, w, h);
      const edge = cx.createLinearGradient(0, 0, w, h * 0.45);
      edge.addColorStop(0, cyberAlpha(color.ink, 0.13 * strength));
      edge.addColorStop(0.45, cyberAlpha(props.tone, 0.06 * strength));
      edge.addColorStop(1, cyberAlpha(color.violet, 0.075 * strength));
      cx.strokeStyle = edge; cx.lineWidth = 1;
      cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, radius); cx.stroke();
      cx.restore();
    }} />;
}
