import { Text, View, computed, type Accessor } from "nsh";
import { AlertDialog } from "nsh/base";
import type { Host, ViewState } from "./model";
import { validDeviceId } from "./identity";
import { CyberConnectionDot, CyberPressable } from "../../shared/ui/effects";
import { SettingsCard, SettingsDivider, SettingsIcon, row, outlineButton, textButton } from "../../shared/ui/settings-components";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

/** 连接列表不能读取用量、Hooks、偏好或系统服务状态。 */
export type ConnectionState = Pick<ViewState, "deviceId" | "hosts" | "connected" | "selectedId" | "status" | "error" | "pairingCode">;
export interface ConnectionSettingsProps {
  state: Accessor<ConnectionState>;
  onRetry: () => void;
  onSelect: (id: string) => void;
  onForget: (id: string) => void;
}

export function ConnectionSettings(props: ConnectionSettingsProps) {
  const connected = computed(() => props.state().connected);
  const error = computed(() => props.state().error);
  const pairingCode = computed(() => props.state().pairingCode);
  const hosts = computed(() => props.state().hosts);
  const forget = async (id: string) => {
    const result = await AlertDialog.show({ title: "忘记此桌面？",
      description: "将删除本机配对凭据。再次连接需要重新配对；桌面端的记录需在桌面设备管理中移除。",
      actions: [{ label: "取消", id: "cancel", role: "cancel" }, { label: "忘记", id: "forget", role: "destructive" }],
      closeOnEscape: true, closeOnBackdropPress: true });
    if (result === "forget") props.onForget(id);
  };
  return <SettingsCard order={1} testID="settings-connection-card">
    <View style={row}>
      <SettingsIcon symbol="desktop" />
      <View style={{ flex: 1, minWidth: 0, gap: 5 }}>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 7 }}>
          <CyberConnectionDot connected={connected} />
          <Text testID="devices-heading" style={{ color: color.ink, fontSize: 16, lineHeight: 22, fontWeight: 600 }}>
            {connected() ? "已连接桌面" : pairingCode() != null ? "等待完成配对" : "自动发现桌面"}
          </Text>
        </View>
        <Text testID="devices-status" style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>{props.state().status}</Text>
      </View>
    </View>
    {error() ? <>
      <Text style={{ color: color.error, fontSize: 12, lineHeight: 16 }}>{error()}</Text>
      <CyberPressable testID="devices-retry" disabled={!validDeviceId(props.state().deviceId)} onPress={props.onRetry}
        style={{ ...outlineButton, alignSelf: "flex-start" }} disabledStyle={{ opacity: 0.38 }}>
        <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>重试连接</Text>
      </CyberPressable>
    </> : null}
    {pairingCode() != null ? <View style={{ backgroundColor: cyberAlpha(color.primaryContainer, 0.55), borderRadius: 14,
      width: "100%", padding: 16, gap: 8 }}>
      <Text style={{ color: color.cyan, fontSize: 12, lineHeight: 18 }}>在桌面输入配对码</Text>
      <Text testID="devices-pair-code" selectable style={{ color: color.ink, fontSize: 38, letterSpacing: 8, fontWeight: 600, fontFamily: "monospace" }}>
        {pairingCode()}
      </Text>
      <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16 }}>60 秒内有效 · 保持应用在前台</Text>
    </View> : null}
    {hosts().length === 0 ? <>
      <SettingsDivider />
      <View style={{ gap: 6 }}>
        <Text style={{ color: color.ink, fontSize: 13, lineHeight: 19, fontWeight: 500 }}>还未发现桌面</Text>
        <Text style={{ color: color.muted, fontSize: 12, lineHeight: 19 }}>在同一 Wi-Fi 下打开桌面端，并允许本地网络访问。</Text>
      </View>
    </> : null}
    <Each of={hosts()} key={(host) => host.id}>{(host) => <HostRow host={host()} state={props.state}
      onSelect={props.onSelect} onForget={() => void forget(host().id)} />}</Each>
  </SettingsCard>;
}

function HostRow(props: { host: Host; state: Accessor<ConnectionState>; onSelect: (id: string) => void; onForget: () => void }) {
  const connected = computed(() => props.state().connected && props.state().selectedId === props.host.id);
  return <>
    <SettingsDivider />
    <View style={{ gap: 8 }}>
      <View style={row}>
        <View style={{ flex: 1, minWidth: 0, gap: 5 }}>
          <Text style={{ color: color.ink, fontSize: 15, fontWeight: 600 }}>{props.host.name}</Text>
          <Text style={{ color: connected() ? color.cyan : color.muted, fontSize: 12, lineHeight: 18 }}>
            {connected() ? "已连接 · 正在同步" : props.host.paired && props.host.available ? "已配对 · 可连接" : props.host.paired ? "已配对 · 暂未发现" : "可配对"}
          </Text>
        </View>
        {!connected() ? <CyberPressable testID={`host-connect-${props.host.id}`} disabled={!props.host.available}
          onPress={() => props.onSelect(props.host.id)} style={outlineButton} disabledStyle={{ opacity: 0.38 }}>
          <Text style={{ color: color.cyan, fontSize: 13, lineHeight: 20, fontWeight: 500 }}>{props.host.paired ? "连接" : "配对"}</Text>
        </CyberPressable> : null}
      </View>
      {props.host.paired ? <CyberPressable testID={`host-forget-${props.host.id}`} onPress={props.onForget} style={textButton}>
        <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>忘记此桌面</Text>
      </CyberPressable> : null}
    </View>
  </>;
}
