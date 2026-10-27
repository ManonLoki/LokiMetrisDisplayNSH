import { Canvas, Text, View, computed, type Accessor, reduceMotion } from "nsh";
import { Radio, RadioGroup, Switch } from "nsh/base";
import type { PanelConfig } from "../../shared/contracts/panel";
import { SettingsCard, SettingsDivider, SettingsIcon, row, decoration } from "../../shared/ui/settings-components";
import { cyberAlpha, cyberColors as color, cyberMotion } from "../../shared/ui/theme";
import { RemoteTarget } from "../../shared/ui/remote-focus";

type Symbol = "usage" | "hooks";
type Miniature = "compact" | "detailed" | "1x1" | "2x2";

function SettingsToggle(props: { title: string; subtitle: string; symbol: Symbol;
  checked: Accessor<boolean>; onChange: (value: boolean) => void; testID: string }) {
  return <RemoteTarget id={props.testID} activate={() => props.onChange(!props.checked())} radius={16}>
    <Switch testID={props.testID} accessibilityLabel={props.title} checked={props.checked()} onChange={props.onChange}
    style={{ minHeight: 52, flexDirection: "row-reverse", alignItems: "center", gap: 12 }}
    trackStyle={{ width: 52, height: 32, flexShrink: 0, borderRadius: 16,
      padding: props.checked() ? 4 : 8, alignItems: props.checked() ? "flex-end" : "flex-start", justifyContent: "center",
      backgroundColor: props.checked() ? color.cyan : "#233247" }}
    thumbStyle={{ width: props.checked() ? 24 : 16, height: props.checked() ? 24 : 16, borderRadius: 12,
      backgroundColor: props.checked() ? "#0A2530" : color.muted }}>
    <View style={{ ...row, flex: 1, minWidth: 0 }}>
      <SettingsIcon symbol={props.symbol} />
      <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
        <Text style={{ color: color.ink, fontSize: 16, lineHeight: 22, fontWeight: 600 }}>{props.title}</Text>
        <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16 }}>{props.subtitle}</Text>
      </View>
    </View>
  </Switch></RemoteTarget>;
}

export function PanelSettings(props: { config: Accessor<PanelConfig>; panelError: Accessor<string | null>; onPanelChange: (next: PanelConfig) => void }) {
  const showUsage = computed(() => props.config().showUsage);
  const showHooks = computed(() => props.config().showHooks);
  return <SettingsCard testID="settings-panel-card">
    <View style={{ gap: 12 }}>
      <SettingsToggle title="Token 用量" subtitle="总量与 Agent 消耗" symbol="usage" testID="settings-show-usage"
        checked={() => props.config().showUsage} onChange={(showUsage) => props.onPanelChange({ ...props.config(), showUsage })} />
      {showUsage() ? <RadioGroup style={{ flexDirection: "row", gap: 10 }}>
        <SettingsChoice title="精简" kind="compact" selected={() => props.config().usageDetail === "compact"}
          testID="settings-usage-compact" onSelect={() => props.onPanelChange({ ...props.config(), usageDetail: "compact" })} />
        <SettingsChoice title="详细" kind="detailed" selected={() => props.config().usageDetail === "detailed"}
          testID="settings-usage-detailed" onSelect={() => props.onPanelChange({ ...props.config(), usageDetail: "detailed" })} />
      </RadioGroup> : null}
      {showUsage() ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>
        {props.config().usageDetail === "compact" ? "突出总 Token 用量" : "显示总量与各 Agent 明细"}
      </Text> : null}
    </View>
    <SettingsDivider />
    <View style={{ gap: 12 }}>
      <SettingsToggle title="Hooks" subtitle="图文与实时状态" symbol="hooks" testID="settings-show-hooks"
        checked={() => props.config().showHooks} onChange={(showHooks) => props.onPanelChange({ ...props.config(), showHooks })} />
      {showHooks() ? <RadioGroup style={{ flexDirection: "row", gap: 10 }}>
        <SettingsChoice title="1 × 1" kind="1x1" selected={() => props.config().hookLayout === "1x1"}
          testID="settings-hooks-1x1" onSelect={() => props.onPanelChange({ ...props.config(), hookLayout: "1x1" })} />
        <SettingsChoice title="2 × 2" kind="2x2" selected={() => props.config().hookLayout === "2x2"}
          testID="settings-hooks-2x2" onSelect={() => props.onPanelChange({ ...props.config(), hookLayout: "2x2" })} />
      </RadioGroup> : null}
      {showHooks() ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>
        {props.config().hookLayout === "1x1" ? "每页一张，完整展示图片" : "每页四张，左右翻页查看更多"}
      </Text> : null}
    </View>
    {!props.config().showUsage && !props.config().showHooks ? <Text style={{ color: color.muted, fontSize: 12, lineHeight: 18 }}>
      开启一项内容，即可在展示台中显示。
    </Text> : null}
    {props.panelError() ? <Text testID="settings-panel-error" style={{ color: color.error, fontSize: 12, lineHeight: 16 }}>{props.panelError()}</Text> : null}
  </SettingsCard>;
}

function SettingsChoice(props: { title: string; kind: Miniature; selected: Accessor<boolean>; testID: string; onSelect: () => void }) {
  // Radio owns checked semantics. Native state styles reproduce cyberPressScale
  // without nesting a second focusable button around the radio.
  return <RemoteTarget id={props.testID} activate={props.onSelect} radius={12} style={{ flex: 1, minWidth: 0 }}>
    <Radio testID={props.testID} checked={props.selected()} onChange={() => props.onSelect()}
    style={{ flex: 1, minWidth: 0, paddingHorizontal: 10, paddingVertical: 12, gap: 8, alignItems: "center",
      borderRadius: 12, borderWidth: 1, borderColor: props.selected() ? cyberAlpha(color.cyan, 0.65) : cyberAlpha(color.ink, 0.09),
      backgroundColor: props.selected() ? cyberAlpha(color.cyan, 0.08) : cyberAlpha(color.canvas, 0.4),
      transitionProperty: "transform", transitionDuration: reduceMotion() ? 0 : cyberMotion.pressDuration,
      transitionTimingFunction: "cubic-bezier(0.4,0,0.2,1)" }}
    activeStyle={{ transform: [{ scale: reduceMotion() ? 1 : cyberMotion.pressScale }] }}>
    <SettingsMiniature kind={props.kind} selected={props.selected} />
    <Text style={{ fontSize: 13, lineHeight: 19, fontWeight: props.selected() ? 600 : 500,
      color: props.selected() ? color.cyan : color.muted }}>{props.title}</Text>
  </Radio></RemoteTarget>;
}

function SettingsMiniature(props: { kind: Miniature; selected: Accessor<boolean> }) {
  return <Canvas {...decoration} style={{ width: 44, height: 26 }} draw={(cx, { width: w, height: h }) => {
    const tint = props.selected() ? color.cyan : color.muted;
    cx.lineWidth = 1; cx.lineCap = "round";
    if (props.kind === "compact" || props.kind === "detailed") {
      cx.strokeStyle = cyberAlpha(tint, 0.35); cx.beginPath(); cx.roundRect(0, 0, w, h, 2); cx.stroke();
      cx.strokeStyle = tint; cx.lineWidth = 3; cx.beginPath(); cx.moveTo(w * 0.24, h * 0.38); cx.lineTo(w * 0.76, h * 0.38); cx.stroke();
      if (props.kind === "compact") {
        cx.strokeStyle = cyberAlpha(tint, 0.45); cx.lineWidth = 1; cx.beginPath(); cx.moveTo(w * 0.34, h * 0.68); cx.lineTo(w * 0.66, h * 0.68); cx.stroke();
      } else {
        cx.fillStyle = cyberAlpha(tint, 0.35);
        for (let i = 0; i < 3; i++) { cx.beginPath(); cx.roundRect(w * (0.15 + i * 0.25), h * 0.62, w * 0.2, h * 0.17, 1); cx.fill(); }
      }
    } else {
      const columns = props.kind === "1x1" ? 1 : 2, gap = 3;
      const width = (w - gap * (columns - 1)) / columns, height = (h - gap * (columns - 1)) / columns;
      for (let x = 0; x < columns; x++) for (let y = 0; y < columns; y++) {
        cx.beginPath(); cx.roundRect(x * (width + gap), y * (height + gap), width, height, 2);
        cx.fillStyle = cyberAlpha(tint, 0.15); cx.fill(); cx.strokeStyle = cyberAlpha(tint, 0.8); cx.stroke();
      }
    }
  }} />;
}
