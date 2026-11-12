import { computed, signal, untracked, View, type JSX, windowSize, lifecycle, platform } from "nsh";
import { CyberBackdrop, CyberMotionScope } from "../shared/ui/effects";
import { enableRemoteNavigation } from "../shared/ui/remote-focus";
import { CyberNavigation } from "./navigation";
import { PanelBody } from "../features/dashboard";
import { createPanelController } from "../features/hooks";
import { SettingsPage, createSettingsController } from "../features/settings";
import { cyberColors } from "../shared/ui/theme";
import type { ViewState } from "../features/connection";
import { Rules } from "loki-metris-mobile-display/native";
import type { PanelConfig } from "../shared/contracts/panel";
import { navPlacement } from "./navigation-layout";
import { createRuntime, type AppRuntime } from "./runtime";

export type { AppRuntime } from "./runtime";

/** 应用层只持有导航、输入草稿、可见状态和布局控制器。 */
// 创建应用：返回根组件函数
export function createApp(runtime: AppRuntime = {}): () => JSX.Element {
  const page = signal<"panel" | "settings">("panel");
  // 窗口宽度，初始值用非追踪读取
  const width = signal(untracked(() => windowSize().width));
  const height = signal(untracked(() => windowSize().height));
  const wideNavigation = computed(() => navPlacement(width()) === "side");
  // 内容区尺寸，先给出手机默认值，布局后更新
  const contentWidth = signal(390);
  // 内容区高度的初始值
  const contentHeight = signal(776);
  const panel = signal<PanelConfig>(Rules.defaultPanel());
  const view = signal<ViewState>(Rules.connectionEmptyView("", ""));
  const nameDraft = signal("");
  const nameFeedback = signal<string | null>(null);
  const nameIssue = signal<string | null>(null);
  const panelIssue = signal<string | null>(null);
  const savingName = signal(false);
  const appForeground = computed(() => {
    const state = lifecycle();
    // iOS 的场景短暂失焦时仍可见，例如控制中心覆盖时。
    const foreground = platform.os === "ios" ? state !== "background" : state === "active";
    return runtime.foreground?.() ?? foreground;
  });
  // Hooks 面板控制器
  const panelController = createPanelController();
  const settingsController = createSettingsController();
  // 只有移动端随前后台暂停连接，桌面窗口失焦或最小化仍保持连接；短暂失焦仍可见，只有进入后台才暂停。
  const connectionVisible = platform.os === "ios" || platform.os === "android"
    ? () => lifecycle() !== "background"
    : undefined;
  // 运行时读写界面信号
  const ports = createRuntime({
    // 界面状态与回调集合
    panel, view, nameDraft, nameFeedback, nameIssue, panelIssue, savingName,
  }, connectionVisible);
  const bg = cyberColors.canvas;

  function App() {
    // 启动运行时初始化与轮询
    ports.mount();
    // 电视通过方向键移动焦点，确认键执行当前目标提供的动作。
    enableRemoteNavigation(() => runtime.remoteNavigation?.() ?? (platform.idiom === "tv"));
    // 根容器：避开安全区，并把布局尺寸同步到信号
    return (
      <View
        testID="app-root"
        style={{ flex: 1, backgroundColor: bg, paddingTop: "env(safe-area-inset-top)", paddingRight: "env(safe-area-inset-right)",
          paddingBottom: "env(safe-area-inset-bottom)", paddingLeft: "env(safe-area-inset-left)" }}
        onLayout={({ bounds }) => {
          width.set(bounds.size.width);
          height.set(bounds.size.height);
        }}
      >
        {/* 动效作用域：仅前台时播放动画 */}
        <CyberMotionScope foreground={appForeground}>
          <View style={{ flex: 1 }}>
            {/* 背景特效 */}
            <CyberBackdrop testID="cyber-backdrop" />
            {/* 宽屏横排（侧栏 + 内容），窄屏竖排（内容 + 底栏） */}
            <View style={{ flex: 1, flexDirection: wideNavigation() ? "row" : "column" }}>
              {/* 宽屏时侧栏在内容之前 */}
              {() => wideNavigation() ? nav("side") : null}
              {/* 内容区：布局变化时记录尺寸 */}
              <View style={{ flex: 1, minWidth: 0, minHeight: 0 }} onLayout={({ bounds }) => {
                contentWidth.set(bounds.size.width);
                contentHeight.set(bounds.size.height);
              }}>
                {/* 按当前页面渲染展示台或设置页 */}
                {() => page() === "panel" ? <PanelBody width={contentWidth} height={contentHeight}
                  // 状态、配置与控制器
                  state={view} config={panel} controller={panelController}
                  onSettings={() => page.set("settings")}
                  // 帧与图片加载回调；否则显示设置页
                  onFrame={runtime.onFrame} onImageLoad={runtime.onImageLoad} /> : settingsBody()}
              </View>
              {/* 窄屏时底栏在内容之后 */}
              {() => wideNavigation() ? null : nav("bottom")}
            </View>
          </View>
        </CyberMotionScope>
      </View>
    );

  }

  // 创建指定位置的导航栏
  function nav(place: "side" | "bottom") {
    // 导航栏：位置、当前页与切换回调
    return <CyberNavigation placement={place} selected={page} onSelect={(next) => page.set(next)} />;
  }

  // 创建设置页，并连接各回调
  function settingsBody() {
    // 设置页：控制器、宽度与状态
    return <SettingsPage controller={settingsController} width={contentWidth} state={view} config={panel}
      // 名称、问题提示与面板错误
      name={nameDraft} nameIssue={nameIssue} nameFeedback={nameFeedback} panelError={panelIssue} savingName={savingName}
      // 名称与面板的编辑、保存回调
      onNameChange={ports.editName} onSaveName={ports.saveName} onPanelChange={ports.savePanel}
      // 重试、选择与忘记桌面回调
      onRetry={() => ports.command({ op: "refresh" })} onSelect={(id) => ports.command({ op: "select", clientId: id })}
      onForget={(id) => ports.command({ op: "forget", clientId: id })} />;
  }

  // 返回根组件工厂的结果
  return App;
}
