import { Scroll, Text, View, untracked, type Accessor, type Size, type ViewStyle } from "nsh";
import { Dialog } from "nsh/base";
import type { HookItem } from "../../shared/contracts/snapshot";
import { agentDisplayName } from "../../shared/domain/agents";
import { CyberPressable } from "../../shared/ui/effects";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
import { HookStatus } from "./hook-status";
import { HookImageViewport } from "./hook-image";

export function HookDetail(props: { hook: Accessor<HookItem | null>; connected: Accessor<boolean>; width: Accessor<number>; height: Accessor<number>;
  onDismiss: () => void; onImageLoad?: (hash: string, size: Size) => void }) {
  const initialHook = untracked(props.hook)!;
  const hook = () => props.hook() ?? initialHook;
  const detailHeight = () => Math.max(1, props.height() * 0.92 - 32);
  const detailWidth = () => Math.max(1, Math.min(900, props.width() - 32));
  const surfaceStyle = (): ViewStyle => ({ position: "absolute", left: (props.width() - detailWidth()) / 2, top: (props.height() - detailHeight()) / 2,
      width: detailWidth(), height: detailHeight(), minHeight: 0,
      borderRadius: 20, borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.3), backgroundColor: color.surface,
      padding: 16, gap: 12, overflow: "hidden" });
  const body = <View testID="hook-detail" style={{ flex: 1, minWidth: 0, minHeight: 0, gap: 12 }}>
      <View style={{ flexDirection: "row", gap: 10, alignItems: "center" }}>
        <Text numberOfLines={1} style={{ flex: 1, color: color.ink, fontSize: 16, lineHeight: 24, fontWeight: 500 }}>Hook 详情</Text>
        <CyberPressable testID="hook-detail-close" onPress={props.onDismiss}
          style={{ minHeight: 40, paddingHorizontal: 12, justifyContent: "center", borderRadius: 20 }}>
          <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>关闭</Text>
        </CyberPressable>
      </View>
      <Scroll style={{ flex: 1, minWidth: 0, minHeight: 0 }} contentStyle={{ gap: 12 }}>
        <Text style={{ color: color.ink, fontSize: 22, lineHeight: 28, fontWeight: 600 }}>{agentDisplayName(hook().agent)}</Text>
        <HookStatus behavior={hook().behavior} />
        {hook().content.length > 0 ? <Text testID="hook-detail-content" selectable
          style={{ color: color.ink, fontSize: 16, lineHeight: 25 }}>{hook().content}</Text>
          : <Text style={{ color: color.muted, fontSize: 14, lineHeight: 20 }}>暂无正文</Text>}
        {hook().contentTruncated ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 16 }}>正文已截断</Text> : null}
        <HookImageViewport hook={hook} connected={props.connected} height={detailHeight() * 0.7} detail onImageLoad={props.onImageLoad} />
      </Scroll>
    </View>;
  return <Dialog testID="hook-detail-dialog" animation="fade" closeOnBackdropPress closeOnEscape
    onCancel={props.onDismiss} onOpenChange={(open) => { if (!open) props.onDismiss(); }}
    backdropTestID="hook-detail-backdrop" backdropStyle={{ backgroundColor: "#00000099" }} style={surfaceStyle()}>
    {body}
  </Dialog>;
}
