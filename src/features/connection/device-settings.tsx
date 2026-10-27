import { Text, View, computed, signal, type Accessor, type Signal } from "nsh";
import { Input } from "nsh/base";
import { nameError, validDeviceId } from "./identity";
import { CyberPressable } from "../../shared/ui/effects";
import { SettingsCard, SettingsDivider, SettingsIcon, row, textButton } from "../../shared/ui/settings-components";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

/** 设备身份编辑仅接收所需字段，保存命令由应用装配层提交。 */
export interface DeviceSettingsProps {
  deviceId: Accessor<string>;
  deviceName: Accessor<string>;
  expanded?: Signal<boolean>;
  name: Accessor<string>;
  nameIssue: Accessor<string | null>;
  nameFeedback: Accessor<string | null>;
  savingName: Accessor<boolean>;
  onNameChange: (name: string) => void;
  onSaveName: () => void;
}

function DeviceNameInput(props: Pick<DeviceSettingsProps, "name" | "nameIssue" | "onNameChange">) {
  // No name()/state() read in the component constructor: input prop bindings
  // update in place, including while an IME composition or caret is active.
  const issue = () => props.nameIssue() ?? nameError(props.name());
  return <View style={{ width: "100%", gap: 4 }}><Input testID="settings-device-name" accessibilityLabel="设备名称" value={props.name()}
    autoCapitalize="none" autoCorrect={false} autoComplete="off" multiline={false}
    onChange={props.onNameChange} style={{ width: "100%", minHeight: 56, paddingHorizontal: 16, paddingVertical: 16,
      color: color.ink, fontSize: 14, lineHeight: 20, selectionColor: color.cyan,
      backgroundColor: "transparent", borderWidth: 1, borderRadius: 12,
      borderColor: issue() ? color.error : color.outline }}
    focusStyle={{ borderWidth: 2, borderColor: issue() ? color.error : color.cyan }} />
    {issue() ? <Text testID="settings-name-error" style={{ color: color.error, fontSize: 12, lineHeight: 16,
      paddingHorizontal: 16 }}>{issue()}</Text> : null}
  </View>;
}

export function DeviceSettings(props: DeviceSettingsProps) {
  const expanded = props.expanded ?? signal(false);
  const issue = () => props.nameIssue() ?? nameError(props.name());
  const dirty = computed(() => props.name() !== props.deviceName());
  const canonical = computed(() => validDeviceId(props.deviceId()));
  const disabled = () => issue() != null || !canonical() || props.savingName();
  return <SettingsCard order={2} testID="settings-device-card">
    <View style={row}>
      <SettingsIcon symbol="device" />
      <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 22, fontWeight: 600 }}>设备名称</Text>
        <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>在桌面端识别这台设备</Text>
      </View>
    </View>
    <DeviceNameInput name={props.name} nameIssue={props.nameIssue} onNameChange={props.onNameChange} />
    {dirty() ? <CyberPressable testID="settings-save-name" disabled={disabled()}
      onPress={props.onSaveName} style={{ width: "100%", minHeight: 40, borderRadius: 12, paddingHorizontal: 24,
        paddingVertical: 10, backgroundColor: color.cyan, justifyContent: "center", alignItems: "center" }}
      disabledStyle={{ backgroundColor: cyberAlpha(color.ink, 0.12) }}>
      <Text style={{ color: disabled() ? cyberAlpha(color.ink, 0.38) : color.onPrimary, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>保存名称</Text>
    </CyberPressable> : null}
    {props.nameFeedback() ? <Text testID="settings-name-feedback" style={{ color: color.ink, fontSize: 12, lineHeight: 16 }}>{props.nameFeedback()}</Text> : null}
    <SettingsDivider />
    <CyberPressable testID="settings-device-info" onPress={() => expanded.set(!expanded())}
      style={{ ...textButton, width: "100%", flexDirection: "row", alignItems: "center" }}>
      <Text style={{ flex: 1, color: color.muted, fontSize: 13, lineHeight: 19 }}>设备信息</Text>
      <Text style={{ color: color.cyan, fontSize: 12 }}>{expanded() ? "收起 −" : "展开 +"}</Text>
    </CyberPressable>
    {expanded() ? <>
      <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16, fontWeight: 500 }}>设备 ID</Text>
      <Text testID="settings-device-id" selectable style={{ color: color.ink, fontSize: 12, lineHeight: 16, fontFamily: "monospace" }}>
        {props.deviceId() || "身份暂不可用"}
      </Text>
      <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>LokiMetris</Text>
    </> : null}
  </SettingsCard>;
}
