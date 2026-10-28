// 引入响应式原语、视图组件与窗口/安全区读取
import { computed, signal, untracked, View, type JSX, windowSize, safeArea } from "nsh";
// 引入背景特效与动效作用域
import { CyberBackdrop, CyberMotionScope } from "../shared/ui/effects";
// 引入电视遥控器焦点导航
import { enableRemoteNavigation } from "../shared/ui/remote-focus";
// 引入导航栏组件
import { CyberNavigation } from "./navigation";
// 引入展示台主体
import { PanelBody } from "../features/dashboard";
// 引入 Hooks 面板控制器
import { createPanelController } from "../features/hooks";
// 引入设置页及其控制器
import { SettingsPage, createSettingsController } from "../features/settings";
// 引入主题色板
import { cyberColors } from "../shared/ui/theme";
// 引入连接视图的空状态与类型
import type { ViewState } from "../features/connection";
// 引入默认面板配置
import { Rules } from "loki-metris-mobile-display/native";
// 引入面板配置类型
import type { PanelConfig } from "../shared/contracts/panel";
// 引入导航位置策略
import { navPlacement } from "./navigation-layout";
// 引入运行时工厂与类型
import { createRuntime, type AppRuntime } from "./runtime";

// 重新导出运行时类型供外部使用
export type { AppRuntime, LinkPort } from "./runtime";

/** 应用层只持有导航、输入草稿、可见状态和布局控制器。 */
// 创建应用：返回根组件函数
export function createApp(runtime: AppRuntime): () => JSX.Element {
  // 当前页面
  const page = signal<"panel" | "settings">("panel");
  // 窗口宽度，初始值用非追踪读取
  const width = signal(untracked(() => windowSize().width));
  // 窗口高度
  const height = signal(untracked(() => windowSize().height));
  // 是否使用侧边导航
  const wideNavigation = computed(() => navPlacement(width()) === "side");
  // 内容区尺寸，先给出手机默认值，布局后更新
  const contentWidth = signal(390);
  // 内容区高度的初始值
  const contentHeight = signal(776);
  // 面板配置
  const panel = signal<PanelConfig>(Rules.defaultPanel() as PanelConfig);
  // 连接视图
  const view = signal<ViewState>(Rules.connectionEmptyView("", "") as ViewState);
  // 设备名称草稿
  const nameDraft = signal("");
  // 名称保存反馈
  const nameFeedback = signal<string | null>(null);
  // 名称校验问题
  const nameIssue = signal<string | null>(null);
  // 面板设置错误
  const panelIssue = signal<string | null>(null);
  // 名称是否保存中
  const savingName = signal(false);
  // 应用是否在前台
  const appForeground = signal(false);
  // Hooks 面板控制器
  const panelController = createPanelController();
  // 设置页控制器
  const settingsController = createSettingsController();
  // 运行时：把信号交给它读写，视图变化时回调 closeMissingHook
  const ports = createRuntime(runtime, {
    // 界面状态与回调集合
    panel, view, nameDraft, nameFeedback, nameIssue, panelIssue, savingName, foreground: appForeground,
  }, closeMissingHook);
  // 画布背景色
  const bg = cyberColors.canvas;

  // 若选中的 Hook 已不在最新快照中，则退出详情
  function closeMissingHook(next: ViewState) {
    // 当前选中的 Hook 槽位
    const slot = panelController.selectedSlot();
    // 所选 Hook 已不存在时
    if (slot != null && (!next.snapshot?.hooksAvailable || !next.snapshot.hooks.some((hook) => hook.slot === slot))) {
      // 取消选中
      panelController.selectedSlot.set(null);
    }
  }

  // 选择桌面：先关闭详情，再下发选择命令
  function chooseDesktop(id: string) {
    // 取消选中
    panelController.selectedSlot.set(null);
    // 发出选择桌面命令
    ports.command({ op: "select", clientId: id });
  }

  // 忘记桌面：先关闭详情，再下发忘记命令
  function forgetDesktopNow(id: string) {
    // 取消选中
    panelController.selectedSlot.set(null);
    // 发出忘记桌面命令
    ports.command({ op: "forget", clientId: id });
  }

  // 根组件
  function App() {
    // 启动运行时初始化与轮询
    ports.mount();
    // 电视没有触摸：方向键在可按目标之间移动焦点，确认键执行。
    enableRemoteNavigation(() => runtime.remoteNavigation?.() === true);
    // 根容器：避开安全区，并把布局尺寸同步到信号
    return (
      <View
        // 测试标识
        testID="app-root"
        // 样式：撑满与背景
        style={{ flex: 1, backgroundColor: bg, paddingTop: safeArea().top, paddingRight: safeArea().right,
          // 安全区内边距
          paddingBottom: safeArea().bottom, paddingLeft: safeArea().left }}
        // 布局变化回调
        onLayout={(bounds) => {
          // 更新宽度
          width.set(bounds.size.width);
          // 更新高度
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
              <View style={{ flex: 1, minWidth: 0, minHeight: 0 }} onLayout={(bounds) => {
                // 更新内容区宽度
                contentWidth.set(bounds.size.width);
                // 更新内容区高度
                contentHeight.set(bounds.size.height);
              }}>
                {/* 按当前页面渲染展示台或设置页 */}
                {() => page() === "panel" ? <PanelBody width={contentWidth} height={contentHeight}
                  // 视口尺寸、状态、配置与控制器
                  viewportWidth={width} viewportHeight={height} state={view} config={panel} controller={panelController}
                  // 打开设置页
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
      onRetry={() => ports.command({ op: "refresh" })} onSelect={chooseDesktop} onForget={forgetDesktopNow} />;
  }

  // 返回根组件工厂的结果
  return App;
}
