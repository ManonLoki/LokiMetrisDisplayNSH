// 引入文本、视图与响应式原语
import { Text, View, computed, signal, type Accessor, type Signal } from "nsh";
// 引入文本输入框
import { Input } from "nsh/base";
// 引入名称与 ID 校验
import { nameError, validDeviceId } from "./identity";
// 引入可按压容器
import { CyberPressable } from "../../shared/ui/effects";
// 引入设置页通用组件
import { SettingsCard, SettingsDivider, SettingsIcon, row, textButton } from "../../shared/ui/settings-components";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

/** 设备身份编辑仅接收所需字段，保存命令由应用装配层提交。 */
// 设备设置属性
export interface DeviceSettingsProps {
  // 当前设备 ID
  deviceId: Accessor<string>;
  // 已保存的设备名称
  deviceName: Accessor<string>;
  // 设备信息是否展开，可由外部注入
  expanded?: Signal<boolean>;
  // 输入框中的名称草稿
  name: Accessor<string>;
  // 来自 Rust 的名称校验问题
  nameIssue: Accessor<string | null>;
  // 保存后的反馈文案
  nameFeedback: Accessor<string | null>;
  // 是否保存中
  savingName: Accessor<boolean>;
  // 名称改变
  onNameChange: (name: string) => void;
  // 点击保存
  onSaveName: () => void;
}

// 名称输入框，附带错误提示
function DeviceNameInput(props: Pick<DeviceSettingsProps, "name" | "nameIssue" | "onNameChange">) {
  // 组件构造函数中不读取 name()/state():输入属性绑定
  // 会原地更新,包括输入法组合或光标处于活动状态时。
  // 优先使用 Rust 给出的问题，否则按当前输入即时校验
  const issue = () => props.nameIssue() ?? nameError(props.name());
  // 输入框有问题时边框变为错误色
  return <View style={{ width: "100%", gap: 4 }}><Input testID="settings-device-name" accessibilityLabel="设备名称" value={props.name()}
    // 输入框属性：不自动大写和更正
    autoCapitalize="none" autoCorrect={false} autoComplete="off" multiline={false}
    // 变更回调与样式
    onChange={props.onNameChange} style={{ width: "100%", minHeight: 56, paddingHorizontal: 16, paddingVertical: 16,
      // 文字颜色、字号与选区颜色
      color: color.ink, fontSize: 14, lineHeight: 20, selectionColor: color.cyan,
      // 背景、边框与圆角
      backgroundColor: "transparent", borderWidth: 1, borderRadius: 12,
      // 边框颜色随问题变化
      borderColor: issue() ? color.error : color.outline }}
    // 聚焦样式：按是否有问题着色
    focusStyle={{ borderWidth: 2, borderColor: issue() ? color.error : color.cyan }} />
    {/* 有问题时在输入框下方显示 */}
    {issue() ? <Text testID="settings-name-error" style={{ color: color.error, fontSize: 12, lineHeight: 16,
      // 水平内边距
      paddingHorizontal: 16 }}>{issue()}</Text> : null}
  </View>;
}

// 设备设置卡片：编辑名称并查看设备信息
export function DeviceSettings(props: DeviceSettingsProps) {
  // 展开状态：未注入时使用内部信号
  const expanded = props.expanded ?? signal(false);
  // 当前名称问题
  const issue = () => props.nameIssue() ?? nameError(props.name());
  // 草稿是否与已保存名称不同
  const dirty = computed(() => props.name() !== props.deviceName());
  // 设备 ID 是否规范
  const canonical = computed(() => validDeviceId(props.deviceId()));
  // 有问题、ID 非法或保存中时禁用保存
  const disabled = () => issue() != null || !canonical() || props.savingName();
  // 卡片主体
  return <SettingsCard order={2} testID="settings-device-card">
    <View style={row}>
      <SettingsIcon symbol="device" />
      <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 22, fontWeight: 600 }}>设备名称</Text>
        <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>在桌面端识别这台设备</Text>
      </View>
    </View>
    {/* 名称输入框 */}
    <DeviceNameInput name={props.name} nameIssue={props.nameIssue} onNameChange={props.onNameChange} />
    {/* 草稿有改动时才显示保存按钮 */}
    {dirty() ? <CyberPressable testID="settings-save-name" disabled={disabled()}
      // 保存按钮与样式
      onPress={props.onSaveName} style={{ width: "100%", minHeight: 40, borderRadius: 12, paddingHorizontal: 24,
        // 垂直内边距与背景色
        paddingVertical: 10, backgroundColor: color.cyan, justifyContent: "center", alignItems: "center" }}
      // 禁用样式
      disabledStyle={{ backgroundColor: cyberAlpha(color.ink, 0.12) }}>
      <Text style={{ color: disabled() ? cyberAlpha(color.ink, 0.38) : color.onPrimary, fontSize: 14, lineHeight: 20, fontWeight: 500 }}>保存名称</Text>
    </CyberPressable> : null}
    {/* 保存反馈 */}
    {props.nameFeedback() ? <Text testID="settings-name-feedback" style={{ color: color.ink, fontSize: 12, lineHeight: 16 }}>{props.nameFeedback()}</Text> : null}
    {/* 分隔线 */}
    <SettingsDivider />
    {/* 展开/收起设备信息 */}
    <CyberPressable testID="settings-device-info" onPress={() => expanded.set(!expanded())}
      // 文字按钮样式
      style={{ ...textButton, width: "100%", flexDirection: "row", alignItems: "center" }}>
      <Text style={{ flex: 1, color: color.muted, fontSize: 13, lineHeight: 19 }}>设备信息</Text>
      <Text style={{ color: color.cyan, fontSize: 12 }}>{expanded() ? "收起 −" : "展开 +"}</Text>
    </CyberPressable>
    {/* 展开后显示设备 ID 与产品名 */}
    {expanded() ? <>
      <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16, fontWeight: 500 }}>设备 ID</Text>
      <Text testID="settings-device-id" selectable style={{ color: color.ink, fontSize: 12, lineHeight: 16, fontFamily: "monospace" }}>
        {props.deviceId() || "身份暂不可用"}
      </Text>
      <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>LokiMetris</Text>
    </> : null}
  </SettingsCard>;
}
