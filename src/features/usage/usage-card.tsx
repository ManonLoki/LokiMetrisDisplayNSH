// 引入画布、滚动、文本、视图、响应式原语与字体缩放
import { Canvas, Scroll, Text, View, computed, effect, signal, type Accessor, type Bounds, type TextStyle, fontScale } from "nsh";
// 引入投影函数、颜色函数与类型
import { cardProjection, quotaColor, type DisplayRow, type QuotaProjection, type UsageSnapshot } from "./card-projection";
// 引入用量详细程度类型
import type { UsageDetail } from "../../shared/contracts/panel";
// 引入入场动效、数字动画与扫描光效
import { CyberArrival, CyberNumber, CyberSurfaceScan } from "../../shared/ui/effects";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "../../shared/ui/theme";

// 卡片背景色
const background = "#101E33";
// 前景文字色
const foreground = "#E9F2FF";
// 弱化文字色
const muted = "#9BB0C9";
// 额度窗口类型简写
type Window = DisplayRow["windows"][number];

// 用量卡片属性
export interface UsageCardProps {
  // 卡片宽度
  width: Accessor<number>;
  // 卡片高度
  height: Accessor<number>;
  // 当前快照
  snapshot: Accessor<UsageSnapshot | null>;
  // 精简或详细模式
  detail: Accessor<UsageDetail>;
  // 卡片是否处于活跃状态
  active?: Accessor<boolean>;
  // 测试用：上报布局边界
  onFrame?: (bounds: Bounds) => void;
}

/** Android UsageCard.kt:测量等宽字体的总额、独立的纵向滚动,
 * 以及从已认证快照投影出来的提供方窗口。 */
// 用量卡片
export function UsageCard(props: UsageCardProps) {
  // 是否精简模式
  const compact = () => props.detail() === "compact";
  // 随卡片尺寸缩放的系数，限制在 0.85 到 1.4
  const scale = () => Math.max(0.85, Math.min(1.4, props.width() / 360, props.height() / 320));
  // 内边距，精简模式更大
  const padding = () => (compact() ? 16 : 12) * scale();
  // 元素间距
  const spacing = () => (compact() ? 8 : 5) * scale();
  // 明细文字字号
  const detailSize = () => 12 * scale();
  // 内容区宽度
  const contentWidth = () => Math.max(1, props.width() - padding() * 2);
  // 内容区高度
  const contentHeight = () => Math.max(0, props.height() - padding() * 2);
  // 由快照计算出的投影
  const projection = computed(() => cardProjection(props.snapshot()));
  // 是否有 Token 数据
  const hasTokens = computed(() => projection().hasTokens);
  // 是否有精确值
  const hasExact = computed(() => projection().hasExact);
  // 供应商额度列表
  const providers = () => projection().providers;
  // 明细行
  const rows = () => projection().rows;
  // 标题行实测高度
  const headerHeight = signal(0);
  // 精确值行实测高度
  const detailsHeight = signal(0);
  // 精确值出现或消失时重置其高度，等待重新测量
  effect(() => { hasExact(); detailsHeight.set(0); });
  // 摘要区可用高度
  const summaryHeight = () => Math.max(0, contentHeight() - headerHeight() - spacing());
  // 大数字可用高度：精简模式随剩余空间，详细模式取内容高度的 22%
  const numberHeight = () => compact()
    // 高度下限
    ? Math.max(56, (providers().length === 0 ? summaryHeight() : Math.min(summaryHeight(), Math.max(contentHeight() * 0.45, 96))) - detailsHeight() - spacing())
    // 否则按内容高度的比例
    : Math.max(40, contentHeight() * 0.22);
  // 总量文本
  const text = () => projection().total;
  // 大数字字号，由下方画布二分得出
  const numberSize = signal(48);
  // 大数字样式
  const numberStyle = (): TextStyle => ({ color: color.cyan, fontFamily: "monospace", fontWeight: 600,
    // 字号、行高与字距
    fontSize: numberSize(), lineHeight: numberSize() * 1.12, letterSpacing: 0, textAlign: "center" });
  // 额度条宽度随内容宽度分三档
  const barWidth = () => contentWidth() / fontScale() >= 440 ? 44 : contentWidth() / fontScale() >= 280 ? 28 : 18;
  // 高度变化超过阈值才更新，避免布局抖动
  const measureHeight = (target: typeof headerHeight, bounds: Bounds) => {
    // 高度变化超过阈值才更新
    if (Math.abs(target() - bounds.size.height) > 0.1) target.set(bounds.size.height);
  };
  // 总量大数字：数字动画加横向可滚动文本
  const total = () => <View style={{ width: "100%", alignItems: "center", minHeight: numberSize() * 1.12 }}>
    {/* 数值变化带动画 */}
    <CyberNumber value={text()} active={props.active} style={{ width: "100%", alignItems: "center" }} render={(value, exiting) =>
      <Scroll horizontal scrollbar={false} style={{ maxWidth: "100%", flexShrink: 1 }} contentStyle={{ alignItems: "center" }}>
        {/* 退出动画中的副本不带 testID */}
        <Text testID={exiting ? undefined : "usage-total"} selectable numberOfLines={1} style={numberStyle()}>{value}</Text>
      </Scroll>
    } />
  </View>;
  // 精确值行：有值时显示并上报高度
  const exact = () => hasExact() ? <View onLayout={(bounds) => measureHeight(detailsHeight, bounds)}
    // 样式：宽度占满并居中
    style={{ width: "100%", alignItems: "center" }}>
    {/* 横向可滚动，避免长文本被截断 */}
    <Scroll horizontal scrollbar={false} style={{ maxWidth: "100%", flexShrink: 1 }}>
      {/* 精确值文本 */}
      <Text testID="usage-exact" selectable numberOfLines={1} accessibilityLabel={projection().amountAccessibility ?? ""}
        // 样式：等宽字体与详情字号
        style={{ color: muted, fontFamily: "monospace", fontSize: detailSize(), lineHeight: detailSize() * 1.4 }}>
        {projection().exact}
      </Text>
    </Scroll>
  </View> : null;
  // 数据缺失时的提示文本
  const missing = () => <Text style={{ color: muted, fontSize: 14, lineHeight: 20 }}>
    {projection().missingText}
  </Text>;
  // 卡片入场容器
  return <CyberArrival order={0} active={props.active} testID="usage-card" onLayout={props.onFrame}
    // 样式：撑满、背景色与圆角
    style={{ flex: 1, minWidth: 0, minHeight: 0, backgroundColor: background, borderRadius: 18,
      // 描边与裁剪
      borderWidth: 1, borderColor: "transparent", overflow: "hidden" }}>
    {/* 表面扫描光效 */}
    <CyberSurfaceScan active={props.active} surfaceColor={background} testID="usage-surface-scan" />
    {/* 隐形画布：借助 measureText 二分求出能放下的最大字号 */}
    <Canvas pointerEvents="none" accessible={false} accessibilityElementsHidden
      // 样式：隐藏的占位
      style={{ position: "absolute", width: 1, height: 1, opacity: 0 }} draw={(cx) => {
        // 二分下界
        let low = compact() ? 20 : 18;
        // 二分上界
        let high = compact() ? 160 : 48;
        // 迭代 10 次足够收敛
        for (let step = 0; step < 10; step++) {
          // 候选字号
          const candidate = (low + high) / 2;
          // 以候选字号设置字体后测量
          cx.font = `600 ${candidate * fontScale()}px monospace`;
          // 宽高都放得下则提高下界，否则降低上界
          if (cx.measureText(text()).width <= contentWidth() && candidate * 1.12 * fontScale() <= numberHeight()) low = candidate;
          // 放不下，降低上界
          else high = candidate;
        }
        // 变化超过阈值才更新字号
        if (Math.abs(numberSize() - low) > 0.05) numberSize.set(low);
      // 绘制回调结束
      }} />
    {/* 垂直滚动的内容区 */}
    <Scroll testID="usage-scroll" style={{ flex: 1, minHeight: 0, minWidth: 0 }}
      // 内容区样式：内边距、间距与最小高度
      contentStyle={{ padding: padding(), gap: spacing(), minHeight: props.height() - 2 }}>
      {/* 标题行：标题与日期，并测量高度 */}
      <View onLayout={(bounds) => measureHeight(headerHeight, bounds)}
        // 样式：横向换行排列，两端对齐
        style={{ flexDirection: "row", flexWrap: "wrap", justifyContent: "space-between", alignItems: "center", rowGap: 4 }}>
        {/* 标题 */}
        <Text style={{ color: muted, fontSize: 13 * scale(), lineHeight: 18 * scale(), fontWeight: 500 }}>Token 用量</Text>
        {/* 有数据时显示日期 */}
        {hasTokens() ? <Text style={{ color: muted, fontSize: detailSize(), lineHeight: detailSize() * 1.4 }}>{projection().date}</Text> : null}
      </View>
      {/* 精简模式：居中显示总量与精确值 */}
      {compact() ? <View testID="usage-summary" style={{ width: "100%", gap: spacing(), alignItems: "center",
        // 纵向居中与最小高度
        justifyContent: "center", minHeight: providers().length === 0 ? summaryHeight() : 0 }}>
        {/* 有数据显示总量，否则显示缺失提示 */}
        {hasTokens() ? <>{total()}{exact()}</> : missing()}
      </View> : <>
        {/* 详细模式：总量、精确值、分隔线与时区 */}
        {hasTokens() ? <>
          {total()}{exact()}
          {/* 分隔线 */}
          <Divider />
          {/* 设备时区行 */}
          <View style={{ flexDirection: "row", gap: spacing(), alignItems: "center" }}>
            {/* 标签 */}
            <Text style={{ color: muted, fontSize: detailSize(), lineHeight: detailSize() * 1.4 }}>设备时区</Text>
            {/* 时区文本右对齐且可横向滚动 */}
            <Scroll horizontal scrollbar={false} style={{ flex: 1, minWidth: 0 }} contentStyle={{ flexGrow: 1, justifyContent: "flex-end" }}>
              <Text testID="usage-timezone" numberOfLines={1} style={{ color: muted, fontSize: detailSize(), lineHeight: detailSize() * 1.4, textAlign: "right" }}>{projection().zone}</Text>
            </Scroll>
          </View>
        </> : missing()}
        {/* 各代理明细 */}
        <View testID="usage-details" style={{ gap: spacing() }}>
          {/* 无明细时的提示 */}
          {rows().length === 0 ? <Text style={{ color: muted, fontSize: 13 * scale(), lineHeight: 18 * scale() }}>暂无明细</Text> : null}
          {/* 逐行渲染代理明细 */}
          {rows().map((row) => <UsageRow row={row} width={contentWidth()} scale={scale()} detailSize={detailSize()} barWidth={barWidth()} gap={spacing()} />)}
        </View>
        {/* 有额度数据时显示更新时间 */}
        {providers().length ? <QuotaObservedAt unix={projection().quotaObservedAtUnix} /> : null}
      {/* 片段结束 */}
      </>}
      {/* 精简模式下在底部列出额度剩余 */}
      {compact() && providers().length > 0 ? <View testID="usage-quotas" style={{ gap: 10 }}>
        {/* 分隔线 */}
        <Divider />
        {/* 小标题 */}
        <Text style={{ color: muted, fontSize: 12, lineHeight: 16, fontWeight: 500 }}>额度剩余</Text>
        {/* 逐个供应商渲染额度 */}
        {providers().map((provider) => <ProviderQuota provider={provider} width={contentWidth()} />)}
        {/* 额度更新时间 */}
        <QuotaObservedAt unix={projection().quotaObservedAtUnix} />
      </View> : null}
    </Scroll>
  </CyberArrival>;
}

// 水平分隔线
function Divider() {
  // 分隔线
  return <View style={{ height: 1, width: "100%", backgroundColor: cyberAlpha(muted, 0.2) }} />;
}

// 供应商标题：名称与可选的套餐徽标
function ProviderTitle(props: { name: string; plan: string | null; size: number; lineHeight: number; weight: number }) {
  // 横向滚动容器，高度随字体缩放
  return <Scroll horizontal scrollbar={false} style={{ flex: 1, minWidth: 0, height: Math.max(props.lineHeight, 16) * fontScale() }}
    // 内容区样式：横向排列，底部对齐
    contentStyle={{ flexDirection: "row", gap: 6, alignItems: "flex-end" }}>
    {/* 供应商名称 */}
    <Text numberOfLines={1} style={{ color: foreground, fontSize: props.size, lineHeight: props.lineHeight, fontWeight: props.weight }}>{props.name}</Text>
    {/* 有套餐时显示徽标 */}
    {props.plan ? <View style={{ borderRadius: 4, backgroundColor: cyberAlpha(color.cyan, 0.08), borderWidth: 0.5,
      // 边框颜色与内边距
      borderColor: cyberAlpha(color.cyan, 0.3), paddingHorizontal: 5, paddingVertical: 1 }}>
      <Text numberOfLines={1} style={{ color: color.cyan, fontSize: 10, lineHeight: 14, fontWeight: 500 }}>{props.plan}</Text>
    </View> : null}
  </Scroll>;
}

// 详细模式下的代理明细行
function UsageRow(props: { row: DisplayRow; width: number; scale: number; detailSize: number; barWidth: number; gap: number }) {
  // 行容器
  return <View testID={`usage-row-${props.row.name}`} style={{ gap: 2 * props.scale }}>
    {/* 标题行：供应商标题与紧凑用量 */}
    <View style={{ flexDirection: "row", gap: props.gap, alignItems: "center" }}>
      {/* 供应商标题 */}
      <ProviderTitle name={props.row.name} plan={props.row.plan} size={18 * props.scale} lineHeight={25 * props.scale} weight={700} />
      {/* 有紧凑用量时显示，宽度不超过 55% */}
      {props.row.compact != null ? <Scroll horizontal scrollbar={false} style={{ maxWidth: props.width * 0.55, flexShrink: 1 }}>
        <Text selectable numberOfLines={1} style={{ color: foreground, fontFamily: "monospace", fontSize: 18 * props.scale,
          // 行高
          lineHeight: 18 * props.scale * 1.35 }}>{props.row.compact}</Text>
      </Scroll> : null}
    </View>
    {/* 有额度窗口或精确值时显示第二行 */}
    {props.row.windows.length > 0 || props.row.exact != null ? <Scroll horizontal scrollbar={false} style={{ minWidth: 0, width: "100%" }}
      // 内容区样式：最小宽度，横向居中排列
      contentStyle={{ minWidth: props.width, flexDirection: "row", gap: 6, alignItems: "center" }}>
      {/* 逐个额度窗口：标签、百分比与额度条 */}
      {props.row.windows.map((window) => <View accessibilityLabel={`${props.row.name}，${window.kindLabel} 额度剩余 ${window.percent}`}
        // 样式：横向排列，间距 3
        style={{ flexDirection: "row", gap: 3, alignItems: "center" }}>
        {/* 窗口标签 */}
        <Text numberOfLines={1} style={{ color: muted, fontSize: props.detailSize, lineHeight: props.detailSize * 1.4, fontWeight: 500 }}>{window.kindLabel}</Text>
        {/* 剩余百分比，颜色表示等级 */}
        <Text numberOfLines={1} style={{ color: quotaColor(window.level), fontFamily: "monospace", fontSize: props.detailSize,
          // 行高与字重
          lineHeight: props.detailSize * 1.4, fontWeight: 500 }}>{window.percent}</Text>
        {/* 剩余额度条 */}
        <BalanceBar window={window} width={props.barWidth} />
      </View>)}
      {/* 弹性占位，把精确值推到右侧 */}
      <View style={{ flex: 1 }} />
      {/* 精确值 */}
      {props.row.exact != null ? <Text selectable numberOfLines={1} accessibilityLabel={`Token ${props.row.tokenText}`}
        // 样式：等宽字体与详情字号
        style={{ color: muted, fontFamily: "monospace", fontSize: props.detailSize, lineHeight: props.detailSize * 1.4 }}>{props.row.exact}</Text> : null}
    </Scroll> : null}
  </View>;
}

// 精简模式下单个供应商的额度块
function ProviderQuota(props: { provider: QuotaProjection; width: number }) {
  // 宽度足够且字体不大时标题与额度同行
  const inlineTitle = () => props.width >= 260 && fontScale() < 1.3;
  // 额度窗口区域可用宽度
  const windowsWidth = () => inlineTitle() ? (props.width - 12) * 2 / 2.8 : props.width;
  // 窗口列表
  const windows = () => props.provider.windows;
  // 块容器：同行或上下排布
  return <View testID={`quota-${props.provider.provider}`} style={{ flexDirection: inlineTitle() ? "row" : "column", gap: inlineTitle() ? 12 : 5 }}>
    {/* 标题容器 */}
    <View style={inlineTitle() ? { flex: 0.8, minWidth: 0, alignSelf: "flex-start", height: 20 * fontScale() }
      // 否则使用固定高度的样式
      : { width: "100%", height: 20 * fontScale() }}>
      {/* 供应商标题 */}
      <ProviderTitle name={props.provider.name} plan={props.provider.plan} size={14} lineHeight={20} weight={500} />
    </View>
    {/* 窗口容器：过窄时纵向排列 */}
    <View style={{ flex: inlineTitle() ? 2 : undefined, width: inlineTitle() ? undefined : "100%", minWidth: 0,
      // 窗口较窄时竖排，否则横排
      flexDirection: windowsWidth() < 240 * fontScale() ? "column" : "row", gap: windowsWidth() < 240 * fontScale() ? 10 : 12 }}>
      {/* 逐个窗口：标签、百分比与额度条 */}
      {windows().map((window) => <View accessibilityLabel={`${props.provider.name}，${window.kindLabel} 额度剩余 ${window.percent}`}
        // 样式：窗口较窄时不再均分宽度
        style={{ flex: windowsWidth() < 240 * fontScale() ? undefined : 1, gap: 4, minWidth: 0 }}>
        {/* 标签与百分比一行 */}
        <View style={{ flexDirection: "row", flexWrap: "wrap", alignItems: "center", justifyContent: "flex-end", gap: 4 }}>
          {/* 窗口标签 */}
          <Text style={{ flex: 1, color: muted, fontSize: 11, lineHeight: 15.4, fontWeight: 500 }}>{window.kindLabel}</Text>
          {/* 剩余百分比 */}
          <Text style={{ color: quotaColor(window.level), fontFamily: "monospace", fontSize: 11, lineHeight: 15.4, fontWeight: 500 }}>{window.percent}</Text>
        </View>
        {/* 剩余额度条 */}
        <BalanceBar window={window} />
      </View>)}
    </View>
  </View>;
}

// 额度条：底槽加按剩余比例填充的色条
function BalanceBar(props: { window: Window; width?: number }) {
  // 底槽，默认撑满
  return <View style={{ width: props.width ?? "100%", height: 3, borderRadius: 1.5, overflow: "hidden", backgroundColor: cyberAlpha(muted, 0.15) }}>
    {/* 剩余大于 0 才画填充 */}
    {props.window.remainingTenths > 0 ? <View style={{ height: 3, borderRadius: 1.5,
      // 宽度按剩余比例
      width: `${props.window.remainingTenths / 10}%`, backgroundColor: quotaColor(props.window.level) }} /> : null}
  </View>;
}

// 额度观测时间文案
function QuotaObservedAt(props: { unix: number }) {
  // 额度观测时间文本
  return <Text testID="quota-observed-at" style={{ color: muted, fontSize: 11, lineHeight: 16 }}>
    {`额度更新 ${quotaTimestamp(props.unix)}`}
  </Text>;
}

// 把 Unix 秒格式化为本地短日期时间
function quotaTimestamp(unix: number): string {
  // 秒转毫秒后构造日期
  const date = new Date(unix * 1000);
  // 按本地短格式输出日期时间
  return date.toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
}
