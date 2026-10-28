// 引入滚动、文本、视图与响应式原语
import { Scroll, Text, View, computed, effect, untracked, type Bounds, type Size } from "nsh";
// 引入连接视图类型
import type { ViewState } from "../connection";
// 引入面板配置类型
import type { PanelConfig } from "../../shared/contracts/panel";
// 引入 Hooks 面板、详情与控制器
import { HooksPane, HookDetail, createPanelController, type PanelController } from "../hooks";
// 引入用量卡片
import { UsageCard } from "../usage";
// 引入入场动效容器与可按压容器
import { CyberArrival, CyberPressable } from "../../shared/ui/effects";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
// 引入信号读取器类型
import type { Accessor } from "nsh";

// 面板参数既可传值也可传读取器
export type PanelValue<T> = T | Accessor<T>;
// 统一读取：函数则调用，否则直接返回
const read = <T,>(value: PanelValue<T>): T => typeof value === "function" ? (value as Accessor<T>)() : value;

// 展示台主体的属性
export interface PanelBodyProps {
  /** 导航栏/导航轨之后的可用区域。 */
  // 可用宽度（扣除导航之后）
  width: PanelValue<number>;
  // 面板高度
  height: PanelValue<number>;
  /** 完整窗口尺寸:Android 的对话框独立于导航轨。 */
  // 整个窗口宽度，详情对话框不受导航栏影响
  viewportWidth?: PanelValue<number>;
  // 视口高度（可选）
  viewportHeight?: PanelValue<number>;
  // 连接视图
  state: PanelValue<ViewState>;
  // 面板配置
  config: PanelValue<PanelConfig>;
  // Hooks 页码与选择控制器，可由外部注入
  controller?: PanelController;
  // 点击前往设置
  onSettings: () => void;
  // 测试用：上报卡片布局边界
  onFrame?: (id: "usage" | "hook", bounds: Bounds) => void;
  // 测试用：上报图片加载尺寸
  onImageLoad?: (hash: string, size: Size) => void;
}

/** 仪表盘只组合用量与 Hooks；快照更新通过 accessor 传递，不重置页码和详情。 */
// 展示台：按配置组合用量卡片与 Hooks 面板
export function PanelBody(props: PanelBodyProps) {
  // 宽高至少为 1，避免除零
  const width = () => Math.max(1, read(props.width));
  // 高度至少为 1
  const height = () => Math.max(1, read(props.height));
  // 读取视图与配置
  const state = () => read(props.state);
  // 当前面板配置
  const config = () => read(props.config);
  // 是否显示用量卡片与 Hooks 面板
  const showUsage = computed(() => config().showUsage);
  // 是否显示 Hook 区
  const showHooks = computed(() => config().showHooks);
  // 宽大于高时横向并排
  const horizontal = () => width() > height();
  // 宽屏使用更大的内边距
  const padding = () => width() >= 650 ? 28 : 20;
  // 两个面板是否同时显示
  const both = () => config().showUsage && config().showHooks;
  // 单个面板宽度：横向并排时均分并扣除间距
  const paneWidth = () => Math.max(1, (width() - padding() * 2 - (both() && horizontal() ? 12 : 0)) / (both() && horizontal() ? 2 : 1));
  // 单个面板高度：纵向堆叠时均分，44 为页面固定占用
  const paneHeight = () => Math.max(1, (height() - 44 - (both() && !horizontal() ? 12 : 0)) / (both() && !horizontal() ? 2 : 1));
  // 使用注入的控制器，否则创建新的
  const controller = props.controller ?? createPanelController();
  // 当前选中的 Hook 槽位
  const selectedSlot = controller.selectedSlot;
  // 没有打开详情时，页面处于活跃状态
  const active = () => selectedSlot() == null;
  // 选中的 Hook：需要显示 Hooks 且快照中存在该槽位
  const selectedHook = computed(() => config().showHooks && state().snapshot?.hooksAvailable
    // 查找所选槽位的 Hook
    ? state().snapshot!.hooks.find((hook) => hook.slot === selectedSlot()) ?? null : null);
  // 是否存在选中的 Hook
  const hasSelection = computed(() => selectedHook() != null);
  // 选中的 Hook 消失后自动退出详情
  effect(() => {
    // 读取当前选中的 Hook
    const selected = selectedHook();
    // 槽位仍被选中但 Hook 已不存在时清空
    if (untracked(selectedSlot) != null && selected == null) selectedSlot.set(null);
  });
  // 容器：横向或纵向分割
  return <View testID={horizontal() ? "panel-split-row" : "panel-split-column"}
    // 样式：撑满，按方向横排或竖排
    style={{ flex: 1, minWidth: 0, minHeight: 0, flexDirection: horizontal() ? "row" : "column",
      // 水平与垂直内边距及间距
      paddingHorizontal: padding(), paddingVertical: 22, gap: 12 }}>
    {/* 用量卡片区域 */}
    {showUsage() ? <View testID="usage-pane" style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
      <UsageCard width={paneWidth} height={paneHeight} snapshot={() => state().snapshot} detail={() => config().usageDetail} active={active}
        // 上报用量区布局
        onFrame={(bounds) => props.onFrame?.("usage", bounds)} />
    </View> : null}
    {/* Hooks 面板区域 */}
    {showHooks() ? <HooksPane width={paneWidth} height={paneHeight} snapshot={() => state().snapshot}
      // 连接状态与图片警告
      connected={() => state().connected} assetWarning={() => state().assetWarning ?? null} layout={() => config().hookLayout}
      // 控制器
      controller={controller}
      // 所选槽位与选择回调
      selectedSlot={selectedSlot} onSelect={(slot) => selectedSlot.set(slot)} active={active}
      // 上报 Hook 区布局与图片加载
      onFrame={(bounds) => props.onFrame?.("hook", bounds)} onImageLoad={props.onImageLoad} /> : null}
    {/* 两个面板都关闭时显示空状态与跳转设置的入口 */}
    {!showUsage() && !showHooks() ? <CyberArrival order={2} testID="panel-empty"
      // 样式：撑满与背景
      style={{ flex: 1, minWidth: 0, minHeight: 0, backgroundColor: color.surface, borderRadius: 18,
        // 边框与内边距
        borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.14), padding: 18 }}
      // 悬停样式
      hoverStyle={{ borderColor: cyberAlpha(color.cyan, 0.4), shadowColor: color.cyan, shadowOpacity: 0.1, shadowRadius: 7 }}>
      <Scroll contentStyle={{ gap: 14 }} style={{ flex: 1 }}>
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 24, fontWeight: 500 }}>暂无卡片</Text>
        <CyberPressable testID="go-settings" onPress={props.onSettings} style={{ alignSelf: "flex-start", minHeight: 40, paddingHorizontal: 12, justifyContent: "center", borderRadius: 20 }}>
          <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>前往设置</Text>
        </CyberPressable>
      </Scroll>
    </CyberArrival> : null}
    {/* 选中 Hook 时弹出详情 */}
    {hasSelection() ? <HookDetail hook={selectedHook} connected={() => state().connected}
      // 视口宽度
      width={() => Math.max(1, read(props.viewportWidth ?? props.width))}
      // 视口高度
      height={() => Math.max(1, read(props.viewportHeight ?? props.height))}
      // 关闭回调与图片加载回调
      onDismiss={() => selectedSlot.set(null)} onImageLoad={props.onImageLoad} /> : null}
  </View>;
}
