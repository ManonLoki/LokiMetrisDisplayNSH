// 引入滚动、文本、视图、响应式原语与平台信息
import { Scroll, Text, View, computed, effect, signal, untracked, type Accessor, type Bounds, type Size, type YNode, platform } from "nsh";
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
  // 页码与选择控制器
  controller: PanelController;
  // 当前选中槽位
  selectedSlot: Accessor<number | null>;
  // 选中某个槽位
  onSelect: (slot: number) => void;
  // 面板是否活跃
  active: Accessor<boolean>;
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
  // 期望的横向滚动位置，初值按当前页恢复
  const requestedOffset = signal(untracked(() => currentPage() * props.width()));
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
  // 上一次的宽度，用于检测宽度变化
  let lastWidth = untracked(props.width);
  // 翻页滚动视图的引用
  let pager: YNode | undefined;
  // 桌面平台需要用拖拽手势翻页
  const desktop = ["macos", "windows", "linux", "web"].includes(platform.os);
  // 布局、页数或宽度变化时修正页码与滚动位置
  effect(() => {
    // 读取布局、页数与宽度
    const layout = props.layout();
    // 页数
    const count = pageCount();
    // 面板宽度
    const width = props.width();
    // 以当前页码为起点
    let next = untracked(currentPage);
    // 宽度变化时需要重新定位滚动
    let reposition = width !== lastWidth;
    // 布局切换时回到第一页
    if (props.controller.layout != null && layout !== props.controller.layout) { next = 0; reposition = true; }
    // 页码越界时夹到最后一页
    if (next >= count) { next = Math.max(0, count - 1); reposition = true; }
    // 记录本次布局
    props.controller.layout = layout;
    // 记录本次宽度
    lastWidth = width;
    // 页码变化才写回
    if (next !== untracked(currentPage)) currentPage.set(next);
    // 需要时更新期望滚动位置
    if (reposition) requestedOffset.set(next * width);
  });
  // 面板根容器
  return <View testID="hooks-pane" onLayout={props.onFrame} style={{ flex: 1, minWidth: 0, minHeight: 0, gap: 8 }}>
    {/* 桌面端支持拖拽翻页的包裹层 */}
    <View style={{ flex: 1, minWidth: 0, minHeight: 0 }}
      // 桌面端的拖动手势：横向激活，纵向失败
      panActivation={desktop ? { activeOffsetX: 10, failOffsetY: 10 } : undefined}
      // 桌面端拖动结束的处理
      onPanEnd={desktop ? (event) => {
        // 位移不足或更偏垂直时忽略
        if (Math.abs(event.translationX) < 45 || Math.abs(event.translationY) > Math.abs(event.translationX)) return;
        // 按拖拽方向计算目标页并限制范围
        const next = Math.max(0, Math.min(pageCount() - 1, currentPage() + (event.translationX < 0 ? 1 : -1)));
        // 动画滚动到目标页
        pager?.scrollTo({ x: next * props.width(), y: 0, animated: true });
      // 否则不处理
      } : undefined}>
      {/* 横向分页滚动视图 */}
      <Scroll testID="hooks-pager" horizontal pagingEnabled scrollbar={false} bounces={false}
        // 保存滚动视图引用
        ref={(node) => { pager = node; }}
        // 样式：撑满
        style={{ flex: 1, minWidth: 0, minHeight: 0 }}
        // 内容样式：横向排列，高度为分页高度
        contentStyle={{ flexDirection: "row", height: pagerHeight() }}
        // 内容偏移：目标页的横向位置
        contentOffset={{ x: requestedOffset(), y: 0 }}
        // 滚动回调
        onScroll={(event) => {
          // 根据滚动偏移推算当前页
          const page = Math.max(0, Math.min(pages().length - 1, Math.round(event.offset.x / props.width())));
          // 页码变化才写回
          if (page !== currentPage()) currentPage.set(page);
        }}>
        {/* 逐页渲染 */}
        <Each of={pages()} key="index">{(cells, page) => <View style={{ width: props.width(), height: pagerHeight(), gap: 8 }}>
          {/* 每页按行渲染，每行一排卡片 */}
          <Each of={Array.from({ length: columns() }, (_, row) => row)} key="index">{(row) =>
            <View style={{ flex: 1, minHeight: 0, flexDirection: "row", gap: 8 }}>
              {/* 渲染这一行的卡片：有 Hook 则为卡片，否则为占位；详情打开时隐藏图片，仅当前页活跃，聚焦时滚到所在页 */}
              <Each of={cells().slice(row() * columns(), (row() + 1) * columns())}
                // 以槽位作键，空位用占位键
                key={(hook, column) => hook?.slot ?? `empty-${column}`}>{(hook, column) =>
                <If when={hook()} else={<HookPlaceholder snapshot={() => props.snapshot()}
                  // 卡片在页内的位置
                  position={page() * columns() * columns() + row() * columns() + column()} />}>
                  {(item) => <HookCard hook={item} connected={() => props.connected()} width={cardWidth} height={cardHeight}
                    // 非所选槽位才显示图片
                    showImage={() => props.selectedSlot() !== item().slot}
                    // 面板活动且在当前页才播放
                    active={() => props.active() && currentPage() === page()}
                    // 点击选中该槽位
                    onPress={() => props.onSelect(item().slot)}
                    // 远程焦点移动时滚动到所在页
                    onRemoteFocus={() => pager?.scrollTo({ x: page() * props.width(), y: 0, animated: true })}
                    // 图片加载回调
                    onImageLoad={props.onImageLoad} />}
                </If>}
              </Each>
            </View>}
          </Each>
        </View>}</Each>
      </Scroll>
      {/* 多页时显示页码提示 */}
      {pages().length > 1 ? <View pointerEvents="none" style={{ position: "absolute", bottom: 4, left: 0, right: 0, alignItems: "center" }}>
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
    {props.assetWarning() ? <Scroll style={{ maxHeight: 72 }} onLayout={(bounds) => {
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
