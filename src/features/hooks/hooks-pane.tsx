// 引入滚动、文本、视图、响应式原语与平台信息
import { Scroll, Text, View, computed, effect, signal, untracked, type Accessor, type Bounds, type Size, platform } from "nsh";
import { Carousel, CarouselContent, CarouselItem } from "nsh/base";
// 引入布局类型
import type { HookLayout } from "../../shared/contracts/panel";
// 引入 Hooks 快照类型
import type { HookSnapshot } from "./model";
// 引入面板控制器类型
import type { PanelController } from "./controller";
// 引入分页函数
import { Rules } from "loki-metris-mobile-display/native";
// 引入遥控器是否活跃
import { remoteActive } from "../../shared/ui/remote-focus";
// 引入卡片与占位背景
import { HookCard, HookCardSurface } from "./hook-card";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

// Hooks 面板属性
export interface HooksPaneProps {
  // 面板宽度
  width: Accessor<number>;
  // 面板高度
  height: Accessor<number>;
  // 当前快照
  snapshot: Accessor<HookSnapshot | null | undefined>;
  // 是否已连接
  connected: Accessor<boolean>;
  // 资源告警文案
  assetWarning: Accessor<string | null>;
  // 布局：1x1 或 2x2
  layout: Accessor<HookLayout>;
  // 页码控制器
  controller: PanelController;
  // 测试用：上报面板边界
  onFrame?: (bounds: Bounds) => void;
  // 测试用：上报图片加载尺寸
  onImageLoad?: (hash: string, size: Size) => void;
}

// Hooks 面板：分页展示 Hook 卡片
export function HooksPane(props: HooksPaneProps) {
  // enabledAgents 只过滤用量和配额;源码不会过滤 Hooks。
  // 不可用时视为空列表（agent 过滤只作用于用量，不作用于 Hooks）
  const hooks = computed(() => props.snapshot()?.hooksAvailable ? props.snapshot()!.hooks : []);
  // 按布局分页
  const pages = computed(() => Rules.hookPages(hooks().length, props.layout())
    .map((cells) => cells.map((index) => index == null ? null : hooks()[index])));
  // 当前页码，保存在控制器中以便切换页面后保留
  const currentPage = props.controller.hookPage;
  // 页数
  const pageCount = computed(() => pages().length);
  // 告警区域实际高度
  const warningHeight = signal(0);
  // 2x2 为两列，其余一列
  const columns = () => props.layout() === "2x2" ? 2 : 1;
  // 翻页区高度：扣除告警区与间距
  const pagerHeight = () => Math.max(1, props.height() - (props.assetWarning() ? Math.min(72, warningHeight()) + 8 : 0));
  // 卡片宽度：扣除列间距后均分
  const cardWidth = () => Math.max(1, (props.width() - (columns() - 1) * 8) / columns());
  // 卡片高度：扣除行间距后均分
  const cardHeight = () => Math.max(1, (pagerHeight() - (columns() - 1) * 8) / columns());
  // 桌面平台需要用拖拽手势翻页
  const desktop = ["macos", "windows", "linux", "web"].includes(platform.os);
  // 布局改变时回到第一页，页数减少时保留最近的一页
  effect(() => {
    // 读取布局和页数
    const layout = props.layout();
    // 页数
    const count = pageCount();
    // 以当前页码为起点
    let next = untracked(currentPage);
    // 布局切换时回到第一页
    if (props.controller.layout != null && layout !== props.controller.layout) next = 0;
    // 页码越界时夹到最后一页
    if (next >= count) next = Math.max(0, count - 1);
    // 记录本次布局
    props.controller.layout = layout;
    // 页码变化才写回
    if (next !== untracked(currentPage)) currentPage.set(next);
  });
  // 面板根容器
  return <View testID="hooks-pane" onLayout={({ bounds }) => props.onFrame?.(bounds)} style={{ flex: 1, minWidth: 0, minHeight: 0, gap: 8 }}>
    {/* 桌面端支持拖拽翻页的包裹层 */}
    <View style={{ flex: 1, minWidth: 0, minHeight: 0 }}
      // 桌面端的拖动手势：横向激活，纵向失败
      panActivation={desktop ? { activeOffsetX: 10, failOffsetY: 10 } : undefined}
      // 桌面端拖动结束的处理
      onPanEnd={desktop ? (event) => {
        // 位移不足或更偏垂直时忽略
        if (Math.abs(event.translation.x) < 45 || Math.abs(event.translation.y) > Math.abs(event.translation.x)) return;
        // 按拖拽方向计算目标页并限制范围
        const next = Math.max(0, Math.min(pageCount() - 1, currentPage() + (event.translation.x < 0 ? 1 : -1)));
        currentPage.set(next);
      // 否则不处理
      } : undefined}>
      <Carousel index={currentPage()} onChange={({ index }) => currentPage.set(index)}
        accessibilityLabel="Hooks" style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
        <CarouselContent testID="hooks-pager" style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
          {/* 逐页渲染 */}
          <Each of={pages()} key="index">{(cells, page) => <CarouselItem style={{ width: props.width(), height: pagerHeight(), gap: 8 }}>
            {/* 每页按行渲染，每行一排卡片 */}
            <Each of={Array.from({ length: columns() }, (_, row) => row)} key="index">{(row) =>
              <View style={{ flex: 1, minHeight: 0, flexDirection: "row", gap: 8 }}>
                {/* 渲染这一行的卡片：仅当前页播放图片动画，遥控焦点移动时翻到所在页 */}
                <Each of={cells().slice(row() * columns(), (row() + 1) * columns())}
                  // 以槽位作键，空位用占位键
                  key={(hook, column) => hook?.slot ?? `empty-${column}`}>{(hook, column) =>
                  <If when={hook()} else={<HookPlaceholder snapshot={() => props.snapshot()}
                    // 卡片在页内的位置
                    position={page() * columns() * columns() + row() * columns() + column()} />}>
                    {(item) => <HookCard hook={item} connected={() => props.connected()} width={cardWidth} height={cardHeight}
                      // 只在当前页播放
                      active={() => currentPage() === page()}
                      // 远程焦点移动时滚动到所在页
                      onRemoteFocus={() => currentPage.set(page())}
                      // 图片加载回调
                      onImageLoad={props.onImageLoad} />}
                  </If>}
                </Each>
              </View>}
            </Each>
          </CarouselItem>}</Each>
        </CarouselContent>
      </Carousel>
      {/* 多页时显示页码提示 */}
      {pages().length > 1 ? <View style={{ pointerEvents: "none", position: "absolute", bottom: 4, left: 0, right: 0, alignItems: "center" }}>
        {/* 页码提示背景 */}
        <View style={{ backgroundColor: cyberAlpha(color.canvas, 0.94), borderRadius: 8,
          // 边框与内边距
          borderWidth: 0.5, borderColor: cyberAlpha(color.cyan, 0.14), paddingHorizontal: 8, paddingVertical: 3 }}>
          {/* 页码文案，遥控器模式提示方向键翻页 */}
          <Text testID="hooks-page" style={{ color: color.muted, fontSize: 11, lineHeight: 16, fontWeight: 500 }}>
            {`${currentPage() + 1} / ${pages().length} · ${remoteActive() ? "方向键翻页" : "左右滑动"}`}
          </Text>
        </View>
      </View> : null}
    </View>
    {/* 有资源告警时显示可滚动的错误文案 */}
    {props.assetWarning() ? <Scroll style={{ maxHeight: 72 }} onLayout={({ bounds }) => {
      // 高度变化才更新
      if (warningHeight() !== bounds.size.height) warningHeight.set(bounds.size.height);
    }}>
      <Text style={{ color: color.error, fontSize: 12, lineHeight: 16 }}>{props.assetWarning()}</Text>
    </Scroll> : null}
  </View>;
}

// 空位占位：显示等待同步、不可用或暂无活动
function HookPlaceholder(props: { snapshot: Accessor<HookSnapshot | null | undefined>; position: number }) {
  // 占位卡片容器
  return <View testID={`hook-placeholder-${props.position}`} style={{ flex: 1, minWidth: 0, minHeight: 0,
    // 相对定位、圆角与边框
    position: "relative", borderRadius: 18, borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.12), backgroundColor: cyberAlpha(color.surface, 0.7),
    // 内边距与居中对齐
    padding: 8, justifyContent: "center", alignItems: "center" }}>
    {/* 安静模式的背景光层 */}
    <HookCardSurface tone={color.cyan} quiet />
    {/* 按快照状态选择提示文案 */}
    <Text style={{ color: color.muted, fontSize: 12, lineHeight: 16, textAlign: "center" }}>
      {props.snapshot() == null ? "等待同步" : !props.snapshot()!.hooksAvailable ? "Hooks 暂不可用" : "暂无活动"}
    </Text>
  </View>;
}
