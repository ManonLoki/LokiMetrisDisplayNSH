// 引入画布、文本、视图及类型
import { Canvas, Text, View, type Accessor, type Bounds, type Size } from "nsh";
// 引入 Hook 条目类型
import type { HookItem } from "../../shared/contracts/snapshot";
// 引入代理显示名
import { agentDisplayName } from "../../shared/domain/agents";
// 引入入场动效与可按压容器
import { CyberArrival, CyberPressable } from "../../shared/ui/effects";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
// 引入状态徽标与色调函数
import { HookStatus, hookStatusTone } from "./hook-status";
// 引入图片视口
import { HookImageViewport } from "./hook-image";

// Hook 卡片：显示代理名、状态与图片，点击查看详情
export function HookCard(props: { hook: Accessor<HookItem>; connected: Accessor<boolean>; width: Accessor<number>; height: Accessor<number>;
  // 显示图片、活动状态与点击回调
  showImage: Accessor<boolean>; active: Accessor<boolean>; onPress: () => void; onRemoteFocus?: () => void;
  // 布局与图片加载回调
  onFrame?: (bounds: Bounds) => void; onImageLoad?: (hash: string, size: Size) => void }) {
  // 卡片较小时使用紧凑样式
  const small = () => props.width() < 260 || props.height() < 220;
  // 内边距随卡片尺寸收缩
  const padding = () => Math.min(small() ? 10 : 18, props.width() * 0.06, props.height() * 0.08);
  // 子元素间距同样随尺寸收缩
  const gap = () => Math.min(small() ? 6 : 10, props.height() * 0.04);
  // 根据行为状态得到色调
  const tone = () => hookStatusTone(props.hook().behavior);
  // 入场动效容器，order 决定入场先后
  return <CyberArrival order={2 + Math.min(4, props.hook().slot)} active={props.active}
    // 样式：撑满
    style={{ flex: 1, minWidth: 0, minHeight: 0 }} onLayout={props.onFrame}>
    <CyberPressable testID={`hook-card-${props.hook().slot}`} active={props.active} onPress={props.onPress}
      // 远程焦点回调
      onRemoteFocus={props.onRemoteFocus}
      // 读屏标签：查看代理详情
      accessibilityLabel={`查看 ${agentDisplayName(props.hook().agent)} 的详情`}
      // 悬停样式
      hoverStyle={{ borderColor: cyberAlpha(tone(), 0.48), shadowColor: tone(), shadowOpacity: 0.1, shadowRadius: 8 }}
      // 焦点样式
      focusStyle={{ borderColor: cyberAlpha(tone(), 0.85), shadowColor: tone(), shadowOpacity: 0.18, shadowRadius: 9 }}
      // 样式：相对定位与背景
      style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0, backgroundColor: color.surface,
        // 卡片圆角、边框、内边距与间距
        borderRadius: 18, borderWidth: 1, borderColor: cyberAlpha(tone(), 0.25), padding: padding(), gap: gap() }}>
      {/* 静态光层背景 */}
      <HookCardSurface tone={tone()} />
      {/* 标题行：代理名与状态徽标 */}
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: small() ? 5 : 10 }}>
        {/* 代理名，单行超出省略 */}
        <Text numberOfLines={1} ellipsizeMode="tail" style={{ flex: 1, minWidth: 0, color: color.ink,
          // 字号、行高与字重
          fontSize: small() ? 13 : 18, lineHeight: small() ? 18 : 24, fontWeight: 600 }}>{agentDisplayName(props.hook().agent)}</Text>
        {/* 状态徽标 */}
        <HookStatus behavior={props.hook().behavior} compact={small()} />
      </View>
      {/* 图片区域 */}
      <HookImageViewport hook={props.hook} connected={props.connected} showImage={props.showImage} active={props.active} onImageLoad={props.onImageLoad} />
    </CyberPressable>
  </CyberArrival>;
}

/** 静态光层不参与卡片测量，也不随快照启动额外动画。 */
// 卡片背景光层：渐变底色、角落辉光与渐变描边
export function HookCardSurface(props: { tone: string; quiet?: boolean }) {
  // 不接收事件、对无障碍隐藏、内缩 1px 以避开卡片边框
  return <Canvas pointerEvents="none" accessible={false} accessibilityElementsHidden
    // 对子孙元素隐藏读屏
    importantForAccessibility="no-hide-descendants"
    // 样式：铺满
    style={{ position: "absolute", top: 1, right: 1, bottom: 1, left: 1 }} draw={(cx, { width: w, height: h }) => {
      // 尺寸过小时不绘制
      if (w <= 2 || h <= 2) return;
      // quiet 模式降低光效强度
      const strength = props.quiet ? 0.45 : 1;
      // 圆角不超过半边长
      const radius = Math.min(17, w / 2, h / 2);
      // 保存状态并裁剪为圆角矩形
      cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, radius); cx.clip();
      // 底色渐变：左上较亮，向右下透明
      const surface = cx.createLinearGradient(0, 0, w * 0.4, h);
      // 起点颜色
      surface.addColorStop(0, cyberAlpha(color.surfaceContainer, props.quiet ? 0.5 : 0.85));
      // 终点完全透明
      surface.addColorStop(1, cyberAlpha(color.surface, 0));
      // 填充底色
      cx.fillStyle = surface; cx.fillRect(0, 0, w, h);
      // 左上角的径向辉光，使用状态色调
      const glow = cx.createRadialGradient(0, 0, 0, 0, 0, Math.max(w, h * 0.7));
      // 辉光中心
      glow.addColorStop(0, cyberAlpha(props.tone, 0.085 * strength));
      // 辉光边缘透明
      glow.addColorStop(1, cyberAlpha(props.tone, 0));
      // 填充辉光
      cx.fillStyle = glow; cx.fillRect(0, 0, w, h);
      // 描边渐变：白、状态色、紫依次过渡
      const edge = cx.createLinearGradient(0, 0, w, h * 0.45);
      // 起点偏白
      edge.addColorStop(0, cyberAlpha(color.ink, 0.13 * strength));
      // 中段偏状态色
      edge.addColorStop(0.45, cyberAlpha(props.tone, 0.06 * strength));
      // 终点偏紫
      edge.addColorStop(1, cyberAlpha(color.violet, 0.075 * strength));
      // 描一圈 1px 渐变边
      cx.strokeStyle = edge; cx.lineWidth = 1;
      // 路径位于像素中线以保持清晰
      cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, radius); cx.stroke();
      // 还原裁剪状态
      cx.restore();
    // 绘制回调结束
    }} />;
}
