// 引入画布、文本、视图、计算信号与减少动效开关
import { Canvas, Text, View, computed, type Accessor, reduceMotion } from "nsh";
// 引入单选与开关组件
import { Radio, RadioGroup, Switch } from "nsh/base";
// 引入面板配置类型
import type { PanelConfig } from "../../shared/contracts/panel";
// 引入设置页通用组件
import { SettingsCard, SettingsDivider, SettingsIcon, row, decoration } from "../../shared/ui/settings-components";
// 引入透明度、色板与动效参数
import { cyberAlpha, cyberColors as color, cyberMotion } from "../../shared/ui/theme";
// 引入遥控器焦点目标
import { RemoteTarget } from "../../shared/ui/remote-focus";

// 图标种类
type Symbol = "usage" | "hooks";
// 缩略图种类：用量两种、布局两种
type Miniature = "compact" | "detailed" | "1x1" | "2x2";

// 带图标和说明的开关行
function SettingsToggle(props: { title: string; subtitle: string; symbol: Symbol;
  // 选中状态、变更回调与测试标识
  checked: Accessor<boolean>; onChange: (value: boolean) => void; testID: string }) {
  // 遥控器确认键等同于切换开关
  return <RemoteTarget id={props.testID} activate={() => props.onChange(!props.checked())} radius={16}>
    <Switch testID={props.testID} accessibilityLabel={props.title} checked={props.checked()} onChange={props.onChange}
    // 样式：反向横排，垂直居中
    style={{ minHeight: 52, flexDirection: "row-reverse", alignItems: "center", gap: 12 }}
    // 轨道样式
    trackStyle={{ width: 52, height: 32, flexShrink: 0, borderRadius: 16,
      // 内边距与对齐随选中状态变化
      padding: props.checked() ? 4 : 8, alignItems: props.checked() ? "flex-end" : "flex-start", justifyContent: "center",
      // 背景色随选中状态变化
      backgroundColor: props.checked() ? color.cyan : "#233247" }}
    // 滑块样式：选中时更大
    thumbStyle={{ width: props.checked() ? 24 : 16, height: props.checked() ? 24 : 16, borderRadius: 12,
      // 背景色随选中状态变化
      backgroundColor: props.checked() ? "#0A2530" : color.muted }}>
    {/* 图标与文字 */}
    <View style={{ ...row, flex: 1, minWidth: 0 }}>
      {/* 图标 */}
      <SettingsIcon symbol={props.symbol} />
      {/* 标题与副标题 */}
      <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
        {/* 标题 */}
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 22, fontWeight: 600 }}>{props.title}</Text>
        {/* 副标题 */}
        <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16 }}>{props.subtitle}</Text>
      </View>
    </View>
  </Switch></RemoteTarget>;
}

// 面板显示设置：用量与 Hooks 的开关及其子选项
export function PanelSettings(props: { config: Accessor<PanelConfig>; panelError: Accessor<string | null>; onPanelChange: (next: PanelConfig) => void }) {
  // 是否显示用量
  const showUsage = computed(() => props.config().showUsage);
  // 是否显示 Hooks
  const showHooks = computed(() => props.config().showHooks);
  // 卡片主体
  return <SettingsCard testID="settings-panel-card">
    {/* 用量设置组 */}
    <View style={{ gap: 12 }}>
      {/* 用量开关 */}
      <SettingsToggle title="Token 用量" subtitle="总量与 Agent 消耗" symbol="usage" testID="settings-show-usage"
        // 选中状态与变更回调
        checked={() => props.config().showUsage} onChange={(showUsage) => props.onPanelChange({ ...props.config(), showUsage })} />
      {/* 开启后可选精简或详细 */}
      {showUsage() ? <RadioGroup style={{ flexDirection: "row", gap: 10 }}>
        {/* 精简选项 */}
        <SettingsChoice title="精简" kind="compact" selected={() => props.config().usageDetail === "compact"}
          // 紧凑用量
          testID="settings-usage-compact" onSelect={() => props.onPanelChange({ ...props.config(), usageDetail: "compact" })} />
        {/* 详细选项 */}
        <SettingsChoice title="详细" kind="detailed" selected={() => props.config().usageDetail === "detailed"}
          // 详细用量
          testID="settings-usage-detailed" onSelect={() => props.onPanelChange({ ...props.config(), usageDetail: "detailed" })} />
      </RadioGroup> : null}
      {/* 当前模式的说明 */}
      {showUsage() ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>
        {props.config().usageDetail === "compact" ? "突出总 Token 用量" : "显示总量与各 Agent 明细"}
      </Text> : null}
    </View>
    {/* 分隔线 */}
    <SettingsDivider />
    {/* Hooks 设置组 */}
    <View style={{ gap: 12 }}>
      {/* Hooks 开关 */}
      <SettingsToggle title="Hooks" subtitle="图文与实时状态" symbol="hooks" testID="settings-show-hooks"
        // 选中状态与变更回调
        checked={() => props.config().showHooks} onChange={(showHooks) => props.onPanelChange({ ...props.config(), showHooks })} />
      {/* 开启后可选每页一张或四张 */}
      {showHooks() ? <RadioGroup style={{ flexDirection: "row", gap: 10 }}>
        {/* 每页一张 */}
        <SettingsChoice title="1 × 1" kind="1x1" selected={() => props.config().hookLayout === "1x1"}
          // Hook 单列布局
          testID="settings-hooks-1x1" onSelect={() => props.onPanelChange({ ...props.config(), hookLayout: "1x1" })} />
        {/* 每页四张 */}
        <SettingsChoice title="2 × 2" kind="2x2" selected={() => props.config().hookLayout === "2x2"}
          // Hook 双列布局
          testID="settings-hooks-2x2" onSelect={() => props.onPanelChange({ ...props.config(), hookLayout: "2x2" })} />
      </RadioGroup> : null}
      {/* 当前布局的说明 */}
      {showHooks() ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>
        {props.config().hookLayout === "1x1" ? "每页一张，完整展示图片" : "每页四张，左右翻页查看更多"}
      </Text> : null}
    </View>
    {/* 两项都关闭时提示如何开启 */}
    {!props.config().showUsage && !props.config().showHooks ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>
      {/* 提示文字 */}
      开启一项内容，即可在展示台中显示。
    </Text> : null}
    {/* 保存失败时显示错误 */}
    {props.panelError() ? <Text testID="settings-panel-error" style={{ color: color.error, fontSize: 12, lineHeight: 16 }}>{props.panelError()}</Text> : null}
  </SettingsCard>;
}

// 单选卡片：缩略图加标题
function SettingsChoice(props: { title: string; kind: Miniature; selected: Accessor<boolean>; testID: string; onSelect: () => void }) {
  // Radio 持有选中语义。原生状态样式复现 cyberPressScale,
  // 无需在 radio 外再嵌套第二个可聚焦的按钮。
  // 遥控器确认键等同于选择
  return <RemoteTarget id={props.testID} activate={props.onSelect} radius={12} style={{ flex: 1, minWidth: 0 }}>
    <Radio testID={props.testID} checked={props.selected()} onChange={() => props.onSelect()}
    // 样式：撑满，内边距与间距
    style={{ flex: 1, minWidth: 0, paddingHorizontal: 10, paddingVertical: 12, gap: 8, alignItems: "center",
      // 圆角、边框与边框颜色
      borderRadius: 12, borderWidth: 1, borderColor: props.selected() ? cyberAlpha(color.cyan, 0.65) : cyberAlpha(color.ink, 0.09),
      // 背景色随选中状态变化
      backgroundColor: props.selected() ? cyberAlpha(color.cyan, 0.08) : cyberAlpha(color.canvas, 0.4),
      // 过渡属性与时长
      transitionProperty: "transform", transitionDuration: reduceMotion() ? 0 : cyberMotion.pressDuration,
      // 过渡缓动函数
      transitionTimingFunction: "cubic-bezier(0.4,0,0.2,1)" }}
    // 激活样式：减少动效时不缩放
    activeStyle={{ transform: [{ scale: reduceMotion() ? 1 : cyberMotion.pressScale }] }}>
    {/* 缩略图 */}
    <SettingsMiniature kind={props.kind} selected={props.selected} />
    {/* 标题，选中时加粗并高亮 */}
    <Text style={{ fontSize: 13, lineHeight: 19, fontWeight: props.selected() ? 600 : 500,
      // 文字颜色随选中状态变化
      color: props.selected() ? color.cyan : color.muted }}>{props.title}</Text>
  </Radio></RemoteTarget>;
}

// 用画布绘制选项缩略图
function SettingsMiniature(props: { kind: Miniature; selected: Accessor<boolean> }) {
  // 装饰性画布，不参与无障碍
  return <Canvas {...decoration} style={{ width: 44, height: 26 }} draw={(cx, { width: w, height: h }) => {
    // 选中用高亮色，否则用弱化色
    const tint = props.selected() ? color.cyan : color.muted;
    // 默认线宽与圆角端点
    cx.lineWidth = 1; cx.lineCap = "round";
    // 用量缩略图：外框加一条粗线表示总量
    if (props.kind === "compact" || props.kind === "detailed") {
      // 画外框，再画表示总量的粗线
      cx.strokeStyle = cyberAlpha(tint, 0.35); cx.beginPath(); cx.roundRect(0, 0, w, h, 2); cx.stroke();
      // 画图标线条
      cx.strokeStyle = tint; cx.lineWidth = 3; cx.beginPath(); cx.moveTo(w * 0.24, h * 0.38); cx.lineTo(w * 0.76, h * 0.38); cx.stroke();
      // 精简：下方一条细线
      if (props.kind === "compact") {
        // 细线
        cx.strokeStyle = cyberAlpha(tint, 0.45); cx.lineWidth = 1; cx.beginPath(); cx.moveTo(w * 0.34, h * 0.68); cx.lineTo(w * 0.66, h * 0.68); cx.stroke();
      // 否则分支
      } else {
        // 详细：下方三个小块表示明细
        cx.fillStyle = cyberAlpha(tint, 0.35);
        // 逐个绘制
        for (let i = 0; i < 3; i++) { cx.beginPath(); cx.roundRect(w * (0.15 + i * 0.25), h * 0.62, w * 0.2, h * 0.17, 1); cx.fill(); }
      }
    // 否则分支
    } else {
      // 布局缩略图：每边块数与间距
      const columns = props.kind === "1x1" ? 1 : 2, gap = 3;
      // 单块宽高
      const width = (w - gap * (columns - 1)) / columns, height = (h - gap * (columns - 1)) / columns;
      // 逐块绘制填充与描边
      for (let x = 0; x < columns; x++) for (let y = 0; y < columns; y++) {
        // 圆角矩形路径
        cx.beginPath(); cx.roundRect(x * (width + gap), y * (height + gap), width, height, 2);
        // 淡填充加描边
        cx.fillStyle = cyberAlpha(tint, 0.15); cx.fill(); cx.strokeStyle = cyberAlpha(tint, 0.8); cx.stroke();
      }
    }
  // 绘制回调结束
  }} />;
}
