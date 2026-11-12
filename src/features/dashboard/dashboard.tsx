import { Scroll, Text, View, computed, type Bounds, type Size } from "nsh";
import type { ViewState } from "../connection";
import type { PanelConfig } from "../../shared/contracts/panel";
import { HooksPane, createPanelController, type PanelController } from "../hooks";
import { UsageCard } from "../usage";
import { CyberArrival, CyberPressable } from "../../shared/ui/effects";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";
import type { Accessor } from "nsh";

// 面板参数既可传值也可传读取器
export type PanelValue<T> = T | Accessor<T>;
// 统一读取：函数则调用，否则直接返回
const read = <T,>(value: PanelValue<T>): T => typeof value === "function" ? (value as Accessor<T>)() : value;

export interface PanelBodyProps {
  /** 导航栏/导航轨之后的可用区域。 */
  // 可用宽度（扣除导航之后）
  width: PanelValue<number>;
  height: PanelValue<number>;
  state: PanelValue<ViewState>;
  config: PanelValue<PanelConfig>;
  // Hooks 页码控制器，可由外部注入
  controller?: PanelController;
  onSettings: () => void;
  // 测试用：上报卡片布局边界
  onFrame?: (id: "usage" | "hook", bounds: Bounds) => void;
  // 测试用：上报图片加载尺寸
  onImageLoad?: (hash: string, size: Size) => void;
}

/** 仪表盘只组合用量与 Hooks；快照更新通过 accessor 传递，保留页码。 */
// 展示台：按配置组合用量卡片与 Hooks 面板
export function PanelBody(props: PanelBodyProps) {
  // 宽高至少为 1，避免除零
  const width = () => Math.max(1, read(props.width));
  const height = () => Math.max(1, read(props.height));
  const state = () => read(props.state);
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
  // 容器：横向或纵向分割
  return <View testID={horizontal() ? "panel-split-row" : "panel-split-column"}
    // 样式：撑满，按方向横排或竖排
    style={{ flex: 1, minWidth: 0, minHeight: 0, flexDirection: horizontal() ? "row" : "column",
      // 水平与垂直内边距及间距
      paddingHorizontal: padding(), paddingVertical: 22, gap: 12 }}>
    {/* 用量卡片区域 */}
    {showUsage() ? <View testID="usage-pane" style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
      <UsageCard width={paneWidth} height={paneHeight} snapshot={() => state().snapshot} detail={() => config().usageDetail}
        onFrame={(bounds) => props.onFrame?.("usage", bounds)} />
    </View> : null}
    {/* Hooks 面板区域 */}
    {showHooks() ? <HooksPane width={paneWidth} height={paneHeight} snapshot={() => state().snapshot}
      // 连接状态与图片警告
      connected={() => state().connected} assetWarning={() => state().assetWarning ?? null} layout={() => config().hookLayout}
      controller={controller}
      // 上报 Hook 区布局与图片加载
      onFrame={(bounds) => props.onFrame?.("hook", bounds)} onImageLoad={props.onImageLoad} /> : null}
    {/* 两个面板都关闭时显示空状态与跳转设置的入口 */}
    {!showUsage() && !showHooks() ? <CyberArrival order={2} testID="panel-empty"
      style={{ flex: 1, minWidth: 0, minHeight: 0, backgroundColor: color.surface, borderRadius: 18,
        borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.14), padding: 18 }}
      hoverStyle={{ borderColor: cyberAlpha(color.cyan, 0.4), boxShadow: `0 0 7px ${cyberAlpha(color.cyan, 0.1)}` }}>
      <Scroll contentStyle={{ gap: 14 }} style={{ flex: 1 }}>
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 24, fontWeight: 500 }}>暂无卡片</Text>
        <CyberPressable testID="go-settings" onPress={props.onSettings} style={{ alignSelf: "flex-start", minHeight: 40, paddingHorizontal: 12, justifyContent: "center", borderRadius: 20 }}>
          <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>前往设置</Text>
        </CyberPressable>
      </Scroll>
    </CyberArrival> : null}
  </View>;
}
