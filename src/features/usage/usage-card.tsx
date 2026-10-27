import { Canvas, Scroll, Text, View, computed, effect, signal, type Accessor, type Bounds, type TextStyle, fontScale } from "nsh";
import { cardProjection, quotaColor, type DisplayRow, type QuotaProjection, type UsageSnapshot } from "./logic";
import type { UsageDetail } from "../../shared/contracts/panel";
import { CyberArrival, CyberNumber, CyberSurfaceScan } from "../../shared/ui/effects";
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

const background = "#101E33";
const foreground = "#E9F2FF";
const muted = "#9BB0C9";
type Window = DisplayRow["windows"][number];

export interface UsageCardProps {
  width: Accessor<number>;
  height: Accessor<number>;
  snapshot: Accessor<UsageSnapshot | null>;
  detail: Accessor<UsageDetail>;
  active?: Accessor<boolean>;
  onFrame?: (bounds: Bounds) => void;
}

/** Android UsageCard.kt: measured monospace total, independent vertical scroll,
 * and provider windows projected from the authenticated snapshot. */
export function UsageCard(props: UsageCardProps) {
  const compact = () => props.detail() === "compact";
  const scale = () => Math.max(0.85, Math.min(1.4, props.width() / 360, props.height() / 320));
  const padding = () => (compact() ? 16 : 12) * scale();
  const spacing = () => (compact() ? 8 : 5) * scale();
  const detailSize = () => 12 * scale();
  const contentWidth = () => Math.max(1, props.width() - padding() * 2);
  const contentHeight = () => Math.max(0, props.height() - padding() * 2);
  const projection = computed(() => cardProjection(props.snapshot()));
  const hasTokens = computed(() => projection().hasTokens);
  const hasExact = computed(() => projection().hasExact);
  const providers = () => projection().providers;
  const rows = () => projection().rows;
  const headerHeight = signal(0);
  const detailsHeight = signal(0);
  effect(() => { hasExact(); detailsHeight.set(0); });
  const summaryHeight = () => Math.max(0, contentHeight() - headerHeight() - spacing());
  const numberHeight = () => compact()
    ? Math.max(56, (providers().length === 0 ? summaryHeight() : Math.min(summaryHeight(), Math.max(contentHeight() * 0.45, 96))) - detailsHeight() - spacing())
    : Math.max(40, contentHeight() * 0.22);
  const text = () => projection().total;
  const numberSize = signal(48);
  const numberStyle = (): TextStyle => ({ color: color.cyan, fontFamily: "monospace", fontWeight: 600,
    fontSize: numberSize(), lineHeight: numberSize() * 1.12, letterSpacing: 0, textAlign: "center" });
  const barWidth = () => contentWidth() / fontScale() >= 440 ? 44 : contentWidth() / fontScale() >= 280 ? 28 : 18;
  const measureHeight = (target: typeof headerHeight, bounds: Bounds) => {
    if (Math.abs(target() - bounds.size.height) > 0.1) target.set(bounds.size.height);
  };
  const total = () => <View style={{ width: "100%", alignItems: "center", minHeight: numberSize() * 1.12 }}>
    <CyberNumber value={text()} active={props.active} style={{ width: "100%", alignItems: "center" }} render={(value, exiting) =>
      <Scroll horizontal scrollbar={false} style={{ maxWidth: "100%", flexShrink: 1 }} contentStyle={{ alignItems: "center" }}>
        <Text testID={exiting ? undefined : "usage-total"} selectable numberOfLines={1} style={numberStyle()}>{value}</Text>
      </Scroll>
    } />
  </View>;
  const exact = () => hasExact() ? <View onLayout={(bounds) => measureHeight(detailsHeight, bounds)}
    style={{ width: "100%", alignItems: "center" }}>
    <Scroll horizontal scrollbar={false} style={{ maxWidth: "100%", flexShrink: 1 }}>
      <Text testID="usage-exact" selectable numberOfLines={1} accessibilityLabel={projection().amountAccessibility ?? ""}
        style={{ color: muted, fontFamily: "monospace", fontSize: detailSize(), lineHeight: detailSize() * 1.4 }}>
        {projection().exact}
      </Text>
    </Scroll>
  </View> : null;
  const missing = () => <Text style={{ color: muted, fontSize: 14, lineHeight: 20 }}>
    {projection().missingText}
  </Text>;
  return <CyberArrival order={0} active={props.active} testID="usage-card" onLayout={props.onFrame}
    style={{ flex: 1, minWidth: 0, minHeight: 0, backgroundColor: background, borderRadius: 18,
      borderWidth: 1, borderColor: "transparent", overflow: "hidden" }}>
    <CyberSurfaceScan active={props.active} surfaceColor={background} testID="usage-surface-scan" />
    <Canvas pointerEvents="none" accessible={false} accessibilityElementsHidden
      style={{ position: "absolute", width: 1, height: 1, opacity: 0 }} draw={(cx) => {
        let low = compact() ? 20 : 18;
        let high = compact() ? 160 : 48;
        for (let step = 0; step < 10; step++) {
          const candidate = (low + high) / 2;
          cx.font = `600 ${candidate * fontScale()}px monospace`;
          if (cx.measureText(text()).width <= contentWidth() && candidate * 1.12 * fontScale() <= numberHeight()) low = candidate;
          else high = candidate;
        }
        if (Math.abs(numberSize() - low) > 0.05) numberSize.set(low);
      }} />
    <Scroll testID="usage-scroll" style={{ flex: 1, minHeight: 0, minWidth: 0 }}
      contentStyle={{ padding: padding(), gap: spacing(), minHeight: props.height() - 2 }}>
      <View onLayout={(bounds) => measureHeight(headerHeight, bounds)}
        style={{ flexDirection: "row", flexWrap: "wrap", justifyContent: "space-between", alignItems: "center", rowGap: 4 }}>
        <Text style={{ color: muted, fontSize: 13 * scale(), lineHeight: 18 * scale(), fontWeight: 500 }}>Token 用量</Text>
        {hasTokens() ? <Text style={{ color: muted, fontSize: detailSize(), lineHeight: detailSize() * 1.4 }}>{projection().date}</Text> : null}
      </View>
      {compact() ? <View testID="usage-summary" style={{ width: "100%", gap: spacing(), alignItems: "center",
        justifyContent: "center", minHeight: providers().length === 0 ? summaryHeight() : 0 }}>
        {hasTokens() ? <>{total()}{exact()}</> : missing()}
      </View> : <>
        {hasTokens() ? <>
          {total()}{exact()}
          <Divider />
          <View style={{ flexDirection: "row", gap: spacing(), alignItems: "center" }}>
            <Text style={{ color: muted, fontSize: detailSize(), lineHeight: detailSize() * 1.4 }}>设备时区</Text>
            <Scroll horizontal scrollbar={false} style={{ flex: 1, minWidth: 0 }} contentStyle={{ flexGrow: 1, justifyContent: "flex-end" }}>
              <Text testID="usage-timezone" numberOfLines={1} style={{ color: muted, fontSize: detailSize(), lineHeight: detailSize() * 1.4, textAlign: "right" }}>{projection().zone}</Text>
            </Scroll>
          </View>
        </> : missing()}
        <View testID="usage-details" style={{ gap: spacing() }}>
          {rows().length === 0 ? <Text style={{ color: muted, fontSize: 13 * scale(), lineHeight: 18 * scale() }}>暂无明细</Text> : null}
          {rows().map((row) => <UsageRow row={row} width={contentWidth()} scale={scale()} detailSize={detailSize()} barWidth={barWidth()} gap={spacing()} />)}
        </View>
        {providers().length ? <QuotaObservedAt unix={projection().quotaObservedAtUnix} /> : null}
      </>}
      {compact() && providers().length > 0 ? <View testID="usage-quotas" style={{ gap: 10 }}>
        <Divider />
        <Text style={{ color: muted, fontSize: 12, lineHeight: 16, fontWeight: 500 }}>额度剩余</Text>
        {providers().map((provider) => <ProviderQuota provider={provider} width={contentWidth()} />)}
        <QuotaObservedAt unix={projection().quotaObservedAtUnix} />
      </View> : null}
    </Scroll>
  </CyberArrival>;
}

function Divider() {
  return <View style={{ height: 1, width: "100%", backgroundColor: cyberAlpha(muted, 0.2) }} />;
}

function ProviderTitle(props: { name: string; plan: string | null; size: number; lineHeight: number; weight: number }) {
  return <Scroll horizontal scrollbar={false} style={{ flex: 1, minWidth: 0, height: Math.max(props.lineHeight, 16) * fontScale() }}
    contentStyle={{ flexDirection: "row", gap: 6, alignItems: "flex-end" }}>
    <Text numberOfLines={1} style={{ color: foreground, fontSize: props.size, lineHeight: props.lineHeight, fontWeight: props.weight }}>{props.name}</Text>
    {props.plan ? <View style={{ borderRadius: 4, backgroundColor: cyberAlpha(color.cyan, 0.08), borderWidth: 0.5,
      borderColor: cyberAlpha(color.cyan, 0.3), paddingHorizontal: 5, paddingVertical: 1 }}>
      <Text numberOfLines={1} style={{ color: color.cyan, fontSize: 10, lineHeight: 14, fontWeight: 500 }}>{props.plan}</Text>
    </View> : null}
  </Scroll>;
}

function UsageRow(props: { row: DisplayRow; width: number; scale: number; detailSize: number; barWidth: number; gap: number }) {
  return <View testID={`usage-row-${props.row.name}`} style={{ gap: 2 * props.scale }}>
    <View style={{ flexDirection: "row", gap: props.gap, alignItems: "center" }}>
      <ProviderTitle name={props.row.name} plan={props.row.plan} size={18 * props.scale} lineHeight={25 * props.scale} weight={700} />
      {props.row.compact != null ? <Scroll horizontal scrollbar={false} style={{ maxWidth: props.width * 0.55, flexShrink: 1 }}>
        <Text selectable numberOfLines={1} style={{ color: foreground, fontFamily: "monospace", fontSize: 18 * props.scale,
          lineHeight: 18 * props.scale * 1.35 }}>{props.row.compact}</Text>
      </Scroll> : null}
    </View>
    {props.row.windows.length > 0 || props.row.exact != null ? <Scroll horizontal scrollbar={false} style={{ minWidth: 0, width: "100%" }}
      contentStyle={{ minWidth: props.width, flexDirection: "row", gap: 6, alignItems: "center" }}>
      {props.row.windows.map((window) => <View accessibilityLabel={`${props.row.name}，${window.kindLabel} 额度剩余 ${window.percent}`}
        style={{ flexDirection: "row", gap: 3, alignItems: "center" }}>
        <Text numberOfLines={1} style={{ color: muted, fontSize: props.detailSize, lineHeight: props.detailSize * 1.4, fontWeight: 500 }}>{window.kindLabel}</Text>
        <Text numberOfLines={1} style={{ color: quotaColor(window.level), fontFamily: "monospace", fontSize: props.detailSize,
          lineHeight: props.detailSize * 1.4, fontWeight: 500 }}>{window.percent}</Text>
        <BalanceBar window={window} width={props.barWidth} />
      </View>)}
      <View style={{ flex: 1 }} />
      {props.row.exact != null ? <Text selectable numberOfLines={1} accessibilityLabel={`Token ${props.row.tokenText}`}
        style={{ color: muted, fontFamily: "monospace", fontSize: props.detailSize, lineHeight: props.detailSize * 1.4 }}>{props.row.exact}</Text> : null}
    </Scroll> : null}
  </View>;
}

function ProviderQuota(props: { provider: QuotaProjection; width: number }) {
  const inlineTitle = () => props.width >= 260 && fontScale() < 1.3;
  const windowsWidth = () => inlineTitle() ? (props.width - 12) * 2 / 2.8 : props.width;
  const windows = () => props.provider.windows;
  return <View testID={`quota-${props.provider.provider}`} style={{ flexDirection: inlineTitle() ? "row" : "column", gap: inlineTitle() ? 12 : 5 }}>
    <View style={inlineTitle() ? { flex: 0.8, minWidth: 0, alignSelf: "flex-start", height: 20 * fontScale() }
      : { width: "100%", height: 20 * fontScale() }}>
      <ProviderTitle name={props.provider.name} plan={props.provider.plan} size={14} lineHeight={20} weight={500} />
    </View>
    <View style={{ flex: inlineTitle() ? 2 : undefined, width: inlineTitle() ? undefined : "100%", minWidth: 0,
      flexDirection: windowsWidth() < 240 * fontScale() ? "column" : "row", gap: windowsWidth() < 240 * fontScale() ? 10 : 12 }}>
      {windows().map((window) => <View accessibilityLabel={`${props.provider.name}，${window.kindLabel} 额度剩余 ${window.percent}`}
        style={{ flex: windowsWidth() < 240 * fontScale() ? undefined : 1, gap: 4, minWidth: 0 }}>
        <View style={{ flexDirection: "row", flexWrap: "wrap", alignItems: "center", justifyContent: "flex-end", gap: 4 }}>
          <Text style={{ flex: 1, color: muted, fontSize: 11, lineHeight: 15.4, fontWeight: 500 }}>{window.kindLabel}</Text>
          <Text style={{ color: quotaColor(window.level), fontFamily: "monospace", fontSize: 11, lineHeight: 15.4, fontWeight: 500 }}>{window.percent}</Text>
        </View>
        <BalanceBar window={window} />
      </View>)}
    </View>
  </View>;
}

function BalanceBar(props: { window: Window; width?: number }) {
  return <View style={{ width: props.width ?? "100%", height: 3, borderRadius: 1.5, overflow: "hidden", backgroundColor: cyberAlpha(muted, 0.15) }}>
    {props.window.remainingTenths > 0 ? <View style={{ height: 3, borderRadius: 1.5,
      width: `${props.window.remainingTenths / 10}%`, backgroundColor: quotaColor(props.window.level) }} /> : null}
  </View>;
}

function QuotaObservedAt(props: { unix: number }) {
  return <Text testID="quota-observed-at" style={{ color: muted, fontSize: 11, lineHeight: 16 }}>
    {`额度更新 ${quotaTimestamp(props.unix)}`}
  </Text>;
}

function quotaTimestamp(unix: number): string {
  const date = new Date(unix * 1000);
  return date.toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
}
