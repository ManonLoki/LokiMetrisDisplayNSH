import { Scroll, Text, View, signal, type Accessor, type Signal, type YNode, fontScale } from "nsh";
import { registerRemoteScroller } from "../../shared/ui/remote-focus";
import { ConnectionSettings, DeviceSettings, type ViewState } from "../connection";
import type { PanelConfig } from "../../shared/contracts/panel";
import { SettingsSection } from "../../shared/ui/settings-components";
import { cyberColors as color } from "../../shared/ui/theme";
import { PanelSettings } from "./panel-settings";

export interface SettingsPageProps {
  controller?: SettingsController;
  width: Accessor<number>;
  state: Accessor<ViewState>;
  config: Accessor<PanelConfig>;
  name: Accessor<string>;
  nameIssue: Accessor<string | null>;
  nameFeedback: Accessor<string | null>;
  panelError: Accessor<string | null>;
  savingName: Accessor<boolean>;
  onNameChange: (text: string) => void;
  onSaveName: () => void;
  onPanelChange: (next: PanelConfig) => void;
  onRetry: () => void;
  onSelect: (id: string) => void;
  /** Called after this page's original-source confirmation dialog. */
  onForget: (id: string) => void;
}

/** 由应用持有，切换导航时保留设备信息的展开状态。 */
export interface SettingsController { deviceInfoExpanded: Signal<boolean> }
export function createSettingsController(): SettingsController {
  return { deviceInfoExpanded: signal(false) };
}

/** 设置页只编排偏好、连接和身份；网络更新不重新挂载输入节点。 */
export function SettingsPage(props: SettingsPageProps) {
  const wide = () => props.width() >= 720 * Math.max(1, fontScale());
  // 输入仍有焦点时也把第一次点击交给保存按钮；空白点击仍可收起键盘。
  let scroll: YNode | undefined;
  const scrolled = registerRemoteScroller(() => scroll);
  return <Scroll testID="settings-scroll" ref={(node: YNode) => { scroll = node; }}
    onScroll={(e) => scrolled(e.offset.y, e.size.height)} onLayout={(bounds) => scrolled(null, bounds.size.height)}
    keyboardShouldPersistTaps="handled" style={{ flex: 1, minHeight: 0 }}
    contentStyle={{ paddingHorizontal: wide() ? 28 : 20, paddingVertical: 22, gap: 24 }}>
    <View style={{ gap: 4 }}>
      <Text style={{ color: color.ink, fontSize: 30, lineHeight: 38, letterSpacing: -0.8, fontWeight: 700 }}>设置</Text>
      <Text style={{ color: color.muted, fontSize: 13, lineHeight: 19 }}>面板显示与设备连接</Text>
    </View>
    <View testID={wide() ? "settings-two-columns" : "settings-one-column"}
      style={{ flexDirection: wide() ? "row" : "column", alignItems: "stretch", gap: 24 }}>
      <View style={{ flex: wide() ? 1 : undefined, minWidth: 0 }}>
        <SettingsSection title="面板显示" note="自动保存"><PanelSettings config={props.config} panelError={props.panelError} onPanelChange={props.onPanelChange} /></SettingsSection>
      </View>
      <View style={{ flex: wide() ? 1 : undefined, minWidth: 0, gap: 24 }}>
        <SettingsSection title="桌面连接"><ConnectionSettings state={props.state} onRetry={props.onRetry} onSelect={props.onSelect} onForget={props.onForget} /></SettingsSection>
        <SettingsSection title="本机"><DeviceSettings deviceId={() => props.state().deviceId} deviceName={() => props.state().deviceName}
          expanded={props.controller?.deviceInfoExpanded} name={props.name} nameIssue={props.nameIssue} nameFeedback={props.nameFeedback}
          savingName={props.savingName} onNameChange={props.onNameChange} onSaveName={props.onSaveName} /></SettingsSection>

      </View>
    </View>
  </Scroll>;
}

