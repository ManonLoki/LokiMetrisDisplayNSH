import { Scroll, Text, View, computed, effect, signal, untracked, type Accessor, type Bounds, type Size, type YNode, platform } from "nsh";
import type { HookLayout } from "../../shared/contracts/panel";
import type { HookSnapshot } from "./model";
import type { PanelController } from "./controller";
import { hookPages } from "./paging";
import { remoteActive } from "../../shared/ui/remote-focus";
import { HookCard, HookCardSurface } from "./hook-card";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

export interface HooksPaneProps {
  width: Accessor<number>;
  height: Accessor<number>;
  snapshot: Accessor<HookSnapshot | null>;
  connected: Accessor<boolean>;
  assetWarning: Accessor<string | null>;
  layout: Accessor<HookLayout>;
  controller: PanelController;
  selectedSlot: Accessor<number | null>;
  onSelect: (slot: number) => void;
  active: Accessor<boolean>;
  onFrame?: (bounds: Bounds) => void;
  onImageLoad?: (hash: string, size: Size) => void;
}

export function HooksPane(props: HooksPaneProps) {
  // enabledAgents filters usage and quotas only; the source does not filter Hooks.
  const hooks = computed(() => props.snapshot()?.hooksAvailable ? props.snapshot()!.hooks : []);
  const pages = computed(() => hookPages(hooks(), props.layout()));
  const currentPage = props.controller.hookPage;
  const pageCount = computed(() => pages().length);
  const requestedOffset = signal(untracked(() => currentPage() * props.width()));
  const warningHeight = signal(0);
  const columns = () => props.layout() === "2x2" ? 2 : 1;
  const pagerHeight = () => Math.max(1, props.height() - (props.assetWarning() ? Math.min(72, warningHeight()) + 8 : 0));
  const cardWidth = () => Math.max(1, (props.width() - (columns() - 1) * 8) / columns());
  const cardHeight = () => Math.max(1, (pagerHeight() - (columns() - 1) * 8) / columns());
  let lastWidth = untracked(props.width);
  let pager: YNode | undefined;
  const desktop = ["macos", "windows", "linux", "web"].includes(platform.os);
  effect(() => {
    const layout = props.layout();
    const count = pageCount();
    const width = props.width();
    let next = untracked(currentPage);
    let reposition = width !== lastWidth;
    if (props.controller.layout != null && layout !== props.controller.layout) { next = 0; reposition = true; }
    if (next >= count) { next = Math.max(0, count - 1); reposition = true; }
    props.controller.layout = layout;
    lastWidth = width;
    if (next !== untracked(currentPage)) currentPage.set(next);
    if (reposition) requestedOffset.set(next * width);
  });
  return <View testID="hooks-pane" onLayout={props.onFrame} style={{ flex: 1, minWidth: 0, minHeight: 0, gap: 8 }}>
    <View style={{ flex: 1, minWidth: 0, minHeight: 0 }}
      panActivation={desktop ? { activeOffsetX: 10, failOffsetY: 10 } : undefined}
      onPanEnd={desktop ? (event) => {
        if (Math.abs(event.translationX) < 45 || Math.abs(event.translationY) > Math.abs(event.translationX)) return;
        const next = Math.max(0, Math.min(pageCount() - 1, currentPage() + (event.translationX < 0 ? 1 : -1)));
        pager?.scrollTo({ x: next * props.width(), y: 0, animated: true });
      } : undefined}>
      <Scroll testID="hooks-pager" horizontal pagingEnabled scrollbar={false} bounces={false}
        ref={(node) => { pager = node; }}
        style={{ flex: 1, minWidth: 0, minHeight: 0 }}
        contentStyle={{ flexDirection: "row", height: pagerHeight() }}
        contentOffset={{ x: requestedOffset(), y: 0 }}
        onScroll={(event) => {
          const page = Math.max(0, Math.min(pages().length - 1, Math.round(event.offset.x / props.width())));
          if (page !== currentPage()) currentPage.set(page);
        }}>
        <Each of={pages()} key="index">{(cells, page) => <View style={{ width: props.width(), height: pagerHeight(), gap: 8 }}>
          <Each of={Array.from({ length: columns() }, (_, row) => row)} key="index">{(row) =>
            <View style={{ flex: 1, minHeight: 0, flexDirection: "row", gap: 8 }}>
              <Each of={cells().slice(row() * columns(), (row() + 1) * columns())}
                key={(hook, column) => hook?.slot ?? `empty-${column}`}>{(hook, column) =>
                <If when={hook()} else={<HookPlaceholder snapshot={() => props.snapshot()}
                  position={page() * columns() * columns() + row() * columns() + column()} />}>
                  {(item) => <HookCard hook={item} connected={() => props.connected()} width={cardWidth} height={cardHeight}
                    showImage={() => props.selectedSlot() !== item().slot}
                    active={() => props.active() && currentPage() === page()}
                    onPress={() => props.onSelect(item().slot)}
                    onRemoteFocus={() => pager?.scrollTo({ x: page() * props.width(), y: 0, animated: true })}
                    onImageLoad={props.onImageLoad} />}
                </If>}
              </Each>
            </View>}
          </Each>
        </View>}</Each>
      </Scroll>
      {pages().length > 1 ? <View pointerEvents="none" style={{ position: "absolute", bottom: 4, left: 0, right: 0, alignItems: "center" }}>
        <View style={{ backgroundColor: cyberAlpha(color.canvas, 0.94), borderRadius: 8,
          borderWidth: 0.5, borderColor: cyberAlpha(color.cyan, 0.14), paddingHorizontal: 8, paddingVertical: 3 }}>
          <Text testID="hooks-page" style={{ color: color.muted, fontSize: 11, lineHeight: 16, fontWeight: 500 }}>
            {`${currentPage() + 1} / ${pages().length} · ${remoteActive() ? "方向键翻页" : "左右滑动"}`}
          </Text>
        </View>
      </View> : null}
    </View>
    {props.assetWarning() ? <Scroll style={{ maxHeight: 72 }} onLayout={(bounds) => {
      if (warningHeight() !== bounds.size.height) warningHeight.set(bounds.size.height);
    }}>
      <Text style={{ color: color.error, fontSize: 12, lineHeight: 16 }}>{props.assetWarning()}</Text>
    </Scroll> : null}
  </View>;
}

function HookPlaceholder(props: { snapshot: Accessor<HookSnapshot | null>; position: number }) {
  return <View testID={`hook-placeholder-${props.position}`} style={{ flex: 1, minWidth: 0, minHeight: 0,
    position: "relative", borderRadius: 18, borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.12), backgroundColor: cyberAlpha(color.surface, 0.7),
    padding: 8, justifyContent: "center", alignItems: "center" }}>
    <HookCardSurface tone={color.cyan} quiet />
    <Text style={{ color: color.muted, fontSize: 12, lineHeight: 16, textAlign: "center" }}>
      {props.snapshot() == null ? "等待同步" : !props.snapshot()!.hooksAvailable ? "Hooks 暂不可用" : "暂无活动"}
    </Text>
  </View>;
}
