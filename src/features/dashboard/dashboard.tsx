import { Scroll, Text, View, computed, effect, untracked, type Bounds, type Size } from "nsh";
import type { ViewState } from "../connection";
import type { PanelConfig } from "../../shared/contracts/panel";
import { HooksPane, HookDetail, createPanelController, type PanelController } from "../hooks";
import { UsageCard } from "../usage";
import { CyberArrival, CyberPressable } from "../../shared/ui/effects";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
import type { Accessor } from "nsh";

export type PanelValue<T> = T | Accessor<T>;
const read = <T,>(value: PanelValue<T>): T => typeof value === "function" ? (value as Accessor<T>)() : value;

export interface PanelBodyProps {
  /** Available area after the navigation rail / bar. */
  width: PanelValue<number>;
  height: PanelValue<number>;
  /** Full window dimensions: Android's dialog is independent of the rail. */
  viewportWidth?: PanelValue<number>;
  viewportHeight?: PanelValue<number>;
  state: PanelValue<ViewState>;
  config: PanelValue<PanelConfig>;
  controller?: PanelController;
  onSettings: () => void;
  onFrame?: (id: "usage" | "hook", bounds: Bounds) => void;
  onImageLoad?: (hash: string, size: Size) => void;
}

/** 仪表盘只组合用量与 Hooks；快照更新通过 accessor 传递，不重置页码和详情。 */
export function PanelBody(props: PanelBodyProps) {
  const width = () => Math.max(1, read(props.width));
  const height = () => Math.max(1, read(props.height));
  const state = () => read(props.state);
  const config = () => read(props.config);
  const showUsage = computed(() => config().showUsage);
  const showHooks = computed(() => config().showHooks);
  const horizontal = () => width() > height();
  const padding = () => width() >= 650 ? 28 : 20;
  const both = () => config().showUsage && config().showHooks;
  const paneWidth = () => Math.max(1, (width() - padding() * 2 - (both() && horizontal() ? 12 : 0)) / (both() && horizontal() ? 2 : 1));
  const paneHeight = () => Math.max(1, (height() - 44 - (both() && !horizontal() ? 12 : 0)) / (both() && !horizontal() ? 2 : 1));
  const controller = props.controller ?? createPanelController();
  const selectedSlot = controller.selectedSlot;
  const active = () => selectedSlot() == null;
  const selectedHook = computed(() => config().showHooks && state().snapshot?.hooksAvailable
    ? state().snapshot!.hooks.find((hook) => hook.slot === selectedSlot()) ?? null : null);
  const hasSelection = computed(() => selectedHook() != null);
  effect(() => {
    const selected = selectedHook();
    if (untracked(selectedSlot) != null && selected == null) selectedSlot.set(null);
  });
  return <View testID={horizontal() ? "panel-split-row" : "panel-split-column"}
    style={{ flex: 1, minWidth: 0, minHeight: 0, flexDirection: horizontal() ? "row" : "column",
      paddingHorizontal: padding(), paddingVertical: 22, gap: 12 }}>
    {showUsage() ? <View testID="usage-pane" style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
      <UsageCard width={paneWidth} height={paneHeight} snapshot={() => state().snapshot} detail={() => config().usageDetail} active={active}
        onFrame={(bounds) => props.onFrame?.("usage", bounds)} />
    </View> : null}
    {showHooks() ? <HooksPane width={paneWidth} height={paneHeight} snapshot={() => state().snapshot}
      connected={() => state().connected} assetWarning={() => state().assetWarning ?? null} layout={() => config().hookLayout}
      controller={controller}
      selectedSlot={selectedSlot} onSelect={(slot) => selectedSlot.set(slot)} active={active}
      onFrame={(bounds) => props.onFrame?.("hook", bounds)} onImageLoad={props.onImageLoad} /> : null}
    {!showUsage() && !showHooks() ? <CyberArrival order={2} testID="panel-empty"
      style={{ flex: 1, minWidth: 0, minHeight: 0, backgroundColor: color.surface, borderRadius: 18,
        borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.14), padding: 18 }}
      hoverStyle={{ borderColor: cyberAlpha(color.cyan, 0.4), shadowColor: color.cyan, shadowOpacity: 0.1, shadowRadius: 7 }}>
      <Scroll contentStyle={{ gap: 14 }} style={{ flex: 1 }}>
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 24, fontWeight: 500 }}>暂无卡片</Text>
        <CyberPressable testID="go-settings" onPress={props.onSettings} style={{ alignSelf: "flex-start", minHeight: 40, paddingHorizontal: 12, justifyContent: "center", borderRadius: 20 }}>
          <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>前往设置</Text>
        </CyberPressable>
      </Scroll>
    </CyberArrival> : null}
    {hasSelection() ? <HookDetail hook={selectedHook} connected={() => state().connected}
      width={() => Math.max(1, read(props.viewportWidth ?? props.width))}
      height={() => Math.max(1, read(props.viewportHeight ?? props.height))}
      onDismiss={() => selectedSlot.set(null)} onImageLoad={props.onImageLoad} /> : null}
  </View>;
}
