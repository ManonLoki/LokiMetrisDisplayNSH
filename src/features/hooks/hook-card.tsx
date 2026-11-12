import { Canvas, Text, View, type Accessor, type Size, type ViewRef } from "nsh";
import type { HookItem } from "../../shared/contracts/snapshot";
import { Rules } from "loki-metris-mobile-display/native";
import { CyberArrival } from "../../shared/ui/effects";
import { registerRemoteTarget, RemoteRing } from "../../shared/ui/remote-focus";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
import { HookStatus, hookStatusTone } from "./hook-status";
import { HookImageViewport } from "./hook-image";

// Hook 卡片：显示代理名、状态与图片
export function HookCard(props: { hook: Accessor<HookItem>; connected: Accessor<boolean>; width: Accessor<number>; height: Accessor<number>;
  active: Accessor<boolean>; onRemoteFocus: () => void;
  onImageLoad?: (hash: string, size: Size) => void }) {
  // 卡片较小时使用紧凑样式
  const small = () => props.width() < 260 || props.height() < 220;
  // 内边距随卡片尺寸收缩
  const padding = () => Math.min(small() ? 10 : 18, props.width() * 0.06, props.height() * 0.08);
  // 子元素间距同样随尺寸收缩
  const gap = () => Math.min(small() ? 6 : 10, props.height() * 0.04);
  // 根据行为状态得到色调
  const tone = () => hookStatusTone(props.hook().behavior);
  // 遥控器可将焦点移到卡片并翻页，卡片本身没有确认动作。
  const id = `hook-card-${props.hook().slot}`;
  let node: ViewRef | undefined;
  registerRemoteTarget(id, { node: () => node, disabled: () => false, focused: props.onRemoteFocus });
  // 入场动效容器，order 决定入场先后
  return <CyberArrival order={2 + Math.min(4, props.hook().slot)} active={props.active}
    style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
    <View testID={id} ref={(value: ViewRef) => { node = value; }}
      // 样式：相对定位与背景
      style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0, backgroundColor: color.surface,
        // 卡片圆角、边框、内边距与间距
        borderRadius: 18, borderWidth: 1, borderColor: cyberAlpha(tone(), 0.25), padding: padding(), gap: gap() }}>
      {/* 静态光层背景 */}
      <HookCardSurface tone={tone()} />
      {/* 标题行：代理名与状态徽标 */}
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: small() ? 5 : 10 }}>
        {/* 代理名，单行超出省略 */}
        <Text style={{ numberOfLines: 1, textOverflow: "ellipsis", flex: 1, minWidth: 0, color: color.ink,
          fontSize: small() ? 13 : 18, lineHeight: small() ? 18 : 24, fontWeight: 600 }}>{Rules.agentDisplayName(props.hook().agent)}</Text>
        {/* 状态徽标 */}
        <HookStatus behavior={props.hook().behavior} compact={small()} />
      </View>
      {/* 图片区域 */}
      <HookImageViewport hook={props.hook} connected={props.connected} active={props.active} onImageLoad={props.onImageLoad} />
      <RemoteRing id={id} radius={18} />
    </View>
  </CyberArrival>;
}

/** 静态光层不参与卡片测量，也不随快照启动额外动画。 */
// 卡片背景光层：渐变底色、角落辉光与渐变描边
export function HookCardSurface(props: { tone: string; quiet?: boolean }) {
  // 不接收事件、对无障碍隐藏、内缩 1px 以避开卡片边框
  return <Canvas accessible={false} accessibilityElementsHidden
    // 对子孙元素隐藏读屏
    importantForAccessibility="no-hide-descendants"
    style={{ pointerEvents: "none", position: "absolute", top: 1, right: 1, bottom: 1, left: 1 }} draw={(cx, { width: w, height: h }) => {
      if (w <= 2 || h <= 2) return;
      // quiet 模式降低光效强度
      const strength = props.quiet ? 0.45 : 1;
      const radius = Math.min(17, w / 2, h / 2);
      // 保存状态并裁剪为圆角矩形
      cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, radius); cx.clip();
      // 底色渐变：左上较亮，向右下透明
      const surface = cx.createLinearGradient(0, 0, w * 0.4, h);
      surface.addColorStop(0, cyberAlpha(color.surfaceContainer, props.quiet ? 0.5 : 0.85));
      surface.addColorStop(1, cyberAlpha(color.surface, 0));
      cx.fillStyle = surface; cx.fillRect(0, 0, w, h);
      // 左上角的径向辉光，使用状态色调
      const glow = cx.createRadialGradient(0, 0, 0, 0, 0, Math.max(w, h * 0.7));
      glow.addColorStop(0, cyberAlpha(props.tone, 0.085 * strength));
      glow.addColorStop(1, cyberAlpha(props.tone, 0));
      cx.fillStyle = glow; cx.fillRect(0, 0, w, h);
      // 描边渐变：白、状态色、紫依次过渡
      const edge = cx.createLinearGradient(0, 0, w, h * 0.45);
      edge.addColorStop(0, cyberAlpha(color.ink, 0.13 * strength));
      edge.addColorStop(0.45, cyberAlpha(props.tone, 0.06 * strength));
      edge.addColorStop(1, cyberAlpha(color.violet, 0.075 * strength));
      // 描一圈 1px 渐变边
      cx.strokeStyle = edge; cx.lineWidth = 1;
      // 路径位于像素中线以保持清晰
      cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, radius); cx.stroke();
      cx.restore();
    }} />;
}
