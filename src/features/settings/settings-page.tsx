// 引入滚动、文本、视图、信号与字体缩放
import { Scroll, Text, View, signal, type Accessor, type Signal, type YNode, fontScale } from "nsh";
// 引入遥控器滚动注册
import { registerRemoteScroller } from "../../shared/ui/remote-focus";
// 引入连接与设备设置卡片
import { ConnectionSettings, DeviceSettings, type ViewState } from "../connection";
// 引入面板配置类型
import type { PanelConfig } from "../../shared/contracts/panel";
// 引入设置分区
import { SettingsSection } from "../../shared/ui/settings-components";
// 引入色板
import { cyberColors as color } from "../../shared/ui/theme";
// 引入面板显示设置
import { PanelSettings } from "./panel-settings";

// 设置页属性
export interface SettingsPageProps {
  // 页面控制器，用于保留展开状态
  controller?: SettingsController;
  // 内容区宽度
  width: Accessor<number>;
  // 连接视图
  state: Accessor<ViewState>;
  // 面板配置
  config: Accessor<PanelConfig>;
  // 名称草稿
  name: Accessor<string>;
  // 名称校验问题
  nameIssue: Accessor<string | null>;
  // 名称保存反馈
  nameFeedback: Accessor<string | null>;
  // 面板保存错误
  panelError: Accessor<string | null>;
  // 名称保存中
  savingName: Accessor<boolean>;
  // 名称改变
  onNameChange: (text: string) => void;
  // 保存名称
  onSaveName: () => void;
  // 面板配置改变
  onPanelChange: (next: PanelConfig) => void;
  // 重试连接
  onRetry: () => void;
  // 选择桌面
  onSelect: (id: string) => void;
  /** 在本页的原版来源确认对话框之后调用。 */
  // 确认忘记桌面
  onForget: (id: string) => void;
}

/** 由应用持有，切换导航时保留设备信息的展开状态。 */
// 设置页控制器
export interface SettingsController { deviceInfoExpanded: Signal<boolean> }
// 创建控制器，设备信息默认收起
export function createSettingsController(): SettingsController {
  // 折叠状态信号
  return { deviceInfoExpanded: signal(false) };
}

/** 设置页只编排偏好、连接和身份；网络更新不重新挂载输入节点。 */
// 设置页
export function SettingsPage(props: SettingsPageProps) {
  // 宽屏判断：随字体缩放放大阈值，决定是否分两列
  const wide = () => props.width() >= 720 * Math.max(1, fontScale());
  // 输入仍有焦点时也把第一次点击交给保存按钮；空白点击仍可收起键盘。
  // 滚动视图引用
  let scroll: YNode | undefined;
  // 注册遥控器滚动，得到滚动变化回调
  const scrolled = registerRemoteScroller(() => scroll);
  // 滚动容器：滚动与布局变化通知遥控器逻辑，点击输入框外不吞掉按钮点击
  // 滚动容器：滚动与布局变化通知遥控器逻辑，保留键盘下的点击
  return <Scroll testID="settings-scroll" ref={(node: YNode) => { scroll = node; }}
    // 滚动与布局回调
    onScroll={(e) => scrolled(e.offset.y, e.size.height)} onLayout={(bounds) => scrolled(null, bounds.size.height)}
    // 点击时保持键盘
    keyboardShouldPersistTaps="handled" style={{ flex: 1, minHeight: 0 }}
    // 内容样式：内边距与间距
    contentStyle={{ paddingHorizontal: wide() ? 28 : 20, paddingVertical: 22, gap: 24 }}>
    {/* 页面标题区 */}
    {/* 页面标题区 */}
    <View style={{ gap: 4 }}>
      {/* 标题 */}
      {/* 标题 */}
      <Text style={{ color: color.ink, fontSize: 30, lineHeight: 38, letterSpacing: -0.8, fontWeight: 700 }}>设置</Text>
      {/* 副标题 */}
      {/* 副标题 */}
      <Text style={{ color: color.muted, fontSize: 13, lineHeight: 19 }}>面板显示与设备连接</Text>
    </View>
    {/* 宽屏两列，窄屏单列 */}
    {/* 宽屏两列，窄屏单列 */}
    <View testID={wide() ? "settings-two-columns" : "settings-one-column"}
      // 样式：宽屏横排
      style={{ flexDirection: wide() ? "row" : "column", alignItems: "stretch", gap: 24 }}>
      {/* 左列：面板显示 */}
      {/* 左列：面板显示 */}
      <View style={{ flex: wide() ? 1 : undefined, minWidth: 0 }}>
        {/* 面板显示分区，修改后自动保存 */}
        {/* 面板显示分区，修改后自动保存 */}
        <SettingsSection title="面板显示" note="自动保存"><PanelSettings config={props.config} panelError={props.panelError} onPanelChange={props.onPanelChange} /></SettingsSection>
      </View>
      {/* 右列：连接与本机 */}
      {/* 右列：连接与本机 */}
      <View style={{ flex: wide() ? 1 : undefined, minWidth: 0, gap: 24 }}>
        {/* 桌面连接分区 */}
        {/* 桌面连接分区 */}
        <SettingsSection title="桌面连接"><ConnectionSettings state={props.state} onRetry={props.onRetry} onSelect={props.onSelect} onForget={props.onForget} /></SettingsSection>
        {/* 本机设备分区 */}
        {/* 本机设备分区 */}
        <SettingsSection title="本机"><DeviceSettings deviceId={() => props.state().deviceId} deviceName={() => props.state().deviceName}
          // 展开状态、名称与问题提示
          expanded={props.controller?.deviceInfoExpanded} name={props.name} nameIssue={props.nameIssue} nameFeedback={props.nameFeedback}
          // 保存状态与回调
          savingName={props.savingName} onNameChange={props.onNameChange} onSaveName={props.onSaveName} /></SettingsSection>

      </View>
    </View>
  </Scroll>;
}

