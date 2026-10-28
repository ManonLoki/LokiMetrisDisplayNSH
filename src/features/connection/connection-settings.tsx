// 引入文本、视图、计算信号与读取器类型
import { Text, View, computed, type Accessor } from "nsh";
// 引入确认对话框
import { AlertDialog } from "nsh/base";
// 引入主机与视图状态类型
import type { Host, ViewState } from "./view-state";
// 引入设备 ID 校验
import { validDeviceId } from "./identity";
// 引入连接指示点与可按压容器
import { CyberConnectionDot, CyberPressable } from "../../shared/ui/effects";
// 引入设置页通用组件与样式
import { SettingsCard, SettingsDivider, SettingsIcon, row, outlineButton, textButton } from "../../shared/ui/settings-components";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

/** 连接列表不能读取用量、Hooks、偏好或系统服务状态。 */
// 连接状态类型：取视图状态中的连接相关字段
export type ConnectionState = Pick<ViewState, "deviceId" | "hosts" | "connected" | "selectedId" | "status" | "error" | "pairingCode">;
// 连接设置卡片的属性
export interface ConnectionSettingsProps {
  // 连接状态读取器
  state: Accessor<ConnectionState>;
  // 点击重试
  onRetry: () => void;
  // 选择某台桌面
  onSelect: (id: string) => void;
  // 确认忘记某台桌面
  onForget: (id: string) => void;
}

// 连接设置卡片：展示连接状态、配对码与桌面列表
export function ConnectionSettings(props: ConnectionSettingsProps) {
  // 各字段做成计算信号，仅在自身变化时更新
  const connected = computed(() => props.state().connected);
  // 当前错误
  const error = computed(() => props.state().error);
  // 当前配对码
  const pairingCode = computed(() => props.state().pairingCode);
  // 主机列表
  const hosts = computed(() => props.state().hosts);
  // 忘记前先弹确认框，用户点击“忘记”才真正执行
  const forget = async (id: string) => {
    // 弹窗确认是否忘记
    const result = await AlertDialog.show({ title: "忘记此桌面？",
      // 说明：将删除本机配对凭据
      description: "将删除本机配对凭据。再次连接需要重新配对；桌面端的记录需在桌面设备管理中移除。",
      // 按钮：取消与忘记
      actions: [{ label: "取消", id: "cancel", role: "cancel" }, { label: "忘记", id: "forget", role: "destructive" }],
      // 按 Esc 或点击背景可关闭
      closeOnEscape: true, closeOnBackdropPress: true });
    // 选择“忘记”则回调
    if (result === "forget") props.onForget(id);
  };
  // 卡片主体
  return <SettingsCard order={1} testID="settings-connection-card">
    {/* 顶部：图标、标题与状态 */}
    <View style={row}>
      <SettingsIcon symbol="desktop" />
      <View style={{ flex: 1, minWidth: 0, gap: 5 }}>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 7 }}>
          <CyberConnectionDot connected={connected} />
          <Text testID="devices-heading" style={{ color: color.ink, fontSize: 16, lineHeight: 22, fontWeight: 600 }}>
            {connected() ? "已连接桌面" : pairingCode() != null ? "等待完成配对" : "自动发现桌面"}
          </Text>
        </View>
        {/* 设备状态文字 */}
        <Text testID="devices-status" style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>{props.state().status}</Text>
      </View>
    </View>
    {/* 有错误时显示错误文案与重试按钮；设备 ID 非法时禁用重试 */}
    {error() ? <>
      <Text style={{ color: color.error, fontSize: 12, lineHeight: 16 }}>{error()}</Text>
      <CyberPressable testID="devices-retry" disabled={!validDeviceId(props.state().deviceId)} onPress={props.onRetry}
        // 按钮样式：描边并靠左
        style={{ ...outlineButton, alignSelf: "flex-start" }} disabledStyle={{ opacity: 0.38 }}>
        <Text style={{ color: color.cyan, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>重试连接</Text>
      </CyberPressable>
    </> : null}
    {/* 配对流程中显示配对码 */}
    {pairingCode() != null ? <View style={{ backgroundColor: cyberAlpha(color.primaryContainer, 0.55), borderRadius: 14,
      // 宽度占满、内边距与间距
      width: "100%", padding: 16, gap: 8 }}>
      <Text style={{ color: color.cyan, fontSize: 12, lineHeight: 18 }}>在桌面输入配对码</Text>
      <Text testID="devices-pair-code" selectable style={{ color: color.ink, fontSize: 38, letterSpacing: 8, fontWeight: 600, fontFamily: "monospace" }}>
        {pairingCode()}
      </Text>
      <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16 }}>60 秒内有效 · 保持应用在前台</Text>
    </View> : null}
    {/* 没有发现桌面时给出提示 */}
    {hosts().length === 0 ? <>
      <SettingsDivider />
      <View style={{ gap: 6 }}>
        <Text style={{ color: color.ink, fontSize: 13, lineHeight: 19, fontWeight: 500 }}>还未发现桌面</Text>
        <Text style={{ color: color.muted, fontSize: 12, lineHeight: 19 }}>在同一 Wi-Fi 下打开桌面端，并允许本地网络访问。</Text>
      </View>
    </> : null}
    {/* 逐台渲染桌面行，以 id 作为 key */}
    <Each of={hosts()} key={(host) => host.id}>{(host) => <HostRow host={host()} state={props.state}
      // 选择与忘记回调
      onSelect={props.onSelect} onForget={() => void forget(host().id)} />}</Each>
  </SettingsCard>;
}

// 单个桌面行：显示名称、状态以及连接/忘记按钮
function HostRow(props: { host: Host; state: Accessor<ConnectionState>; onSelect: (id: string) => void; onForget: () => void }) {
  // 是否为当前已连接的桌面
  const connected = computed(() => props.state().connected && props.state().selectedId === props.host.id);
  // 片段
  return <>
    <SettingsDivider />
    <View style={{ gap: 8 }}>
      <View style={row}>
        <View style={{ flex: 1, minWidth: 0, gap: 5 }}>
          <Text style={{ color: color.ink, fontSize: 15, fontWeight: 600 }}>{props.host.name}</Text>
          <Text style={{ color: connected() ? color.cyan : color.muted, fontSize: 12, lineHeight: 18 }}>
            {/* 状态文案：已连接、已配对或未配对 */}
            {connected() ? "已连接 · 正在同步" : props.host.paired && props.host.available ? "已配对 · 可连接" : props.host.paired ? "已配对 · 暂未发现" : "可配对"}
          </Text>
        </View>
        {/* 未连接时显示连接或配对按钮；桌面不可用时禁用 */}
        {!connected() ? <CyberPressable testID={`host-connect-${props.host.id}`} disabled={!props.host.available}
          // 点击选择该桌面
          onPress={() => props.onSelect(props.host.id)} style={outlineButton} disabledStyle={{ opacity: 0.38 }}>
          <Text style={{ color: color.cyan, fontSize: 13, lineHeight: 20, fontWeight: 500 }}>{props.host.paired ? "连接" : "配对"}</Text>
        </CyberPressable> : null}
      </View>
      {/* 已配对的桌面可以忘记 */}
      {props.host.paired ? <CyberPressable testID={`host-forget-${props.host.id}`} onPress={props.onForget} style={textButton}>
        <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>忘记此桌面</Text>
      </CyberPressable> : null}
    </View>
  </>;
}
