// 引入滚动、文本、视图与类型
import { Scroll, Text, View, untracked, type Accessor, type Size, type ViewStyle } from "nsh";
// 引入对话框
import { Dialog } from "nsh/base";
// 引入 Hook 条目类型
import type { HookItem } from "../../shared/contracts/snapshot";
// 引入代理显示名
import { Rules } from "loki-metris-mobile-display/native";
// 引入可按压容器
import { CyberPressable } from "../../shared/ui/effects";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
// 引入状态徽标
import { HookStatus } from "./hook-status";
// 引入图片视口
import { HookImageViewport } from "./hook-image";

// Hook 详情对话框：显示正文与大图
export function HookDetail(props: { hook: Accessor<HookItem | null>; connected: Accessor<boolean>; width: Accessor<number>; height: Accessor<number>;
  // 关闭与图片加载回调
  onDismiss: () => void; onImageLoad?: (hash: string, size: Size) => void }) {
  // 记住首次的 Hook，关闭动画期间数据消失也能继续渲染
  const initialHook = untracked(props.hook)!;
  // 优先使用最新数据，缺失时回退到初始值
  const hook = () => props.hook() ?? initialHook;
  // 对话框高度为窗口 92% 再减去边距
  const detailHeight = () => Math.max(1, props.height() * 0.92 - 32);
  // 对话框宽度最多 900
  const detailWidth = () => Math.max(1, Math.min(900, props.width() - 32));
  // 对话框居中放置的样式
  const surfaceStyle = (): ViewStyle => ({ position: "absolute", left: (props.width() - detailWidth()) / 2, top: (props.height() - detailHeight()) / 2,
      // 宽高与最小高度
      width: detailWidth(), height: detailHeight(), minHeight: 0,
      // 圆角、边框与边框颜色
      borderRadius: 20, borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.3), backgroundColor: color.surface,
      // 内边距、间距与裁剪
      padding: 16, gap: 12, overflow: "hidden" });
  // 对话框内容
  const body = <View testID="hook-detail" style={{ flex: 1, minWidth: 0, minHeight: 0, gap: 12 }}>
      {/* 标题行：标题与关闭按钮 */}
      <View style={{ flexDirection: "row", gap: 10, alignItems: "center" }}>
        {/* 标题 */}
        <Text numberOfLines={1} style={{ flex: 1, color: color.ink, fontSize: 16, lineHeight: 24, fontWeight: 500 }}>Hook 详情</Text>
        {/* 关闭按钮 */}
        <CyberPressable testID="hook-detail-close" onPress={props.onDismiss}
          // 样式：最小高度、内边距与圆角
          style={{ minHeight: 40, paddingHorizontal: 12, justifyContent: "center", borderRadius: 20 }}>
          <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>关闭</Text>
        </CyberPressable>
      </View>
      {/* 可滚动内容区 */}
      <Scroll style={{ flex: 1, minWidth: 0, minHeight: 0 }} contentStyle={{ gap: 12 }}>
        {/* 代理名 */}
        <Text style={{ color: color.ink, fontSize: 22, lineHeight: 28, fontWeight: 600 }}>{Rules.agentDisplayName(hook().agent)}</Text>
        {/* 状态徽标 */}
        <HookStatus behavior={hook().behavior} />
        {/* 有正文则显示，否则提示暂无正文 */}
        {hook().content.length > 0 ? <Text testID="hook-detail-content" selectable
          // 内容文本样式
          style={{ color: color.ink, fontSize: 16, lineHeight: 25 }}>{hook().content}</Text>
          // 无内容时的文本样式
          : <Text style={{ color: color.muted, fontSize: 14, lineHeight: 20 }}>暂无正文</Text>}
        {/* 正文被截断时的提示 */}
        {hook().contentTruncated ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 16 }}>正文已截断</Text> : null}
        {/* 详情中的大图 */}
        <HookImageViewport hook={hook} connected={props.connected} height={detailHeight() * 0.7} detail onImageLoad={props.onImageLoad} />
      </Scroll>
    </View>;
  // 点遮罩、按 Esc 或状态变为关闭都会退出详情
  return <Dialog testID="hook-detail-dialog" animation="fade" closeOnBackdropPress closeOnEscape
    // 取消回调与展开状态变化回调
    onCancel={props.onDismiss} onOpenChange={(open) => { if (!open) props.onDismiss(); }}
    // 背景遮罩的测试标识与样式
    backdropTestID="hook-detail-backdrop" backdropStyle={{ backgroundColor: "#00000099" }} style={surfaceStyle()}>
    {body}
  </Dialog>;
}
