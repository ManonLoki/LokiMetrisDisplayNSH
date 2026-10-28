// 引入 nsh 的画布、视图、响应式原语与动画 API
import { Canvas, View, cleanup, context, effect, signal, split, untracked, type Accessor, type JSX, type StyleProp, type ViewProps, type ViewStyle, type YNode, Animated, createAnimatedValue, type CompositeAnimation, type Easing, reduceMotion } from "nsh";
// 引入原生按钮
import { Button, type ButtonProps } from "nsh/base";
// 引入透明度函数、色板与动效参数
import { cyberAlpha, cyberColors as color, cyberMotion as timing } from "./theme";
// 引入遥控焦点框与目标登记
import { RemoteRing, registerRemoteTarget } from "./remote-focus";
// Windows 前台的全窗口重绘兼容处理
import { useWindowsRepaint } from "./windows-repaint";

// 参数既可传值也可传读取器
export type CyberValue<T> = T | Accessor<T>;
// 统一读取：函数则调用，否则返回原值
const read = <T,>(value: CyberValue<T>): T => typeof value === "function" ? (value as Accessor<T>)() : value;
// 动效许可的上下文，默认不允许
const Motion = context<Accessor<boolean>>(() => false);
// 缓动曲线：快出慢入
const fastOutSlowIn: Easing = [0.4, 0, 0.2, 1];
// 铺满父容器的绝对定位样式
const fill: ViewStyle = { position: "absolute", top: 0, right: 0, bottom: 0, left: 0 };
// 装饰元素公共属性：不接收事件、对无障碍隐藏
const decoration = { pointerEvents: "none" as const, accessible: false,
  // 读屏属性：对辅助技术隐藏
  accessibilityElementsHidden: true, importantForAccessibility: "no-hide-descendants" as const };

// 动效作用域属性
export interface CyberMotionScopeProps {
  // 是否启用动效
  enabled?: CyberValue<boolean>;
  // 是否在前台，缺省视为后台
  foreground?: CyberValue<boolean>;
  // 子节点
  children?: JSX.Element;
}

/** 没有明确的前台信息时,所有特效都保持其静态的
 * 最终外观。页面选择/详情呈现可以停用该作用域。 */
// 动效作用域：启用、前台且未开启“减少动态效果”时才允许动画
export function CyberMotionScope(props: CyberMotionScopeProps) {
  // 向后代提供许可读取器
  return <Motion.Provider value={() => read(props.enabled ?? true)
    // 前景可见且未减少动效才显示
    && read(props.foreground ?? false) && !reduceMotion()}>{props.children}</Motion.Provider>;
}

// 动效属性：active 控制单个元素是否活跃
export interface MotionProps { active?: CyberValue<boolean> }
/** 装饰特效与原生动画图片共用。隐藏的页面、
 * 非活动窗口以及系统的减弱动态偏好共用同一道闸门。 */
// 读取作用域并叠加自身 active 与系统减少动效设置
export function useCyberMotion(props: MotionProps = {}): Accessor<boolean> {
  // 取得作用域许可
  const scope = Motion();
  // 三者同时满足才允许动画
  return () => scope() && read(props.active ?? true) && !reduceMotion();
}

// 创建 0 到 1 的循环相位值
function phase(props: MotionProps, duration: number) {
  // 动效许可
  const allowed = useCyberMotion(props);
  // 动画值
  const value = createAnimatedValue(0);
  // 许可变化时重启循环
  effect(() => {
    // 先复位
    value.set(0);
    // 不允许则停在静态外观
    if (!allowed()) return;
    // 线性无限循环
    const loop = Animated.loop(Animated.timing(value, { toValue: 1, duration, easing: "linear" }));
    // 启动循环
    void loop.start();
    // 清理时停止并复位
    return () => { loop.stop(); value.set(0); };
  });
  // 返回相位值
  return value;
}

// 装饰元素属性
export interface CyberDecorationProps extends MotionProps {
  // 自定义样式
  style?: StyleProp<ViewStyle>;
  // 测试标识
  testID?: string;
}

/** Android 背景:两个径向光晕,以及源码中 24dp、0.5dp 的网格。
 * 视觉外观保持静态；Windows 前台额外驱动不可见的重绘兼容处理。 */
// 背景：深色底、青紫两处辉光与网格线
export function CyberBackdrop(props: CyberDecorationProps & { foreground?: CyberValue<boolean> }) {
  const repaint = useWindowsRepaint(() => read(props.foreground ?? false));
  // 画布铺满，叠加调用方样式
  return <Canvas {...decoration} testID={props.testID} style={[fill, props.style]} draw={(cx, size) => {
    // 取得画布尺寸
    const { width: w, height: h } = size;
    // 先铺底色
    cx.fillStyle = color.canvas; cx.fillRect(0, 0, w, h);
    // 尺寸无效则不继续
    if (w <= 0 || h <= 0) return;
    // 左上角青色径向辉光
    const cyan = cx.createRadialGradient(w * 0.02, h * 0.06, 0, w * 0.02, h * 0.06, w * 0.85);
    // 辉光渐变：中心到边缘透明
    cyan.addColorStop(0, cyberAlpha(color.cyan, 0.11)); cyan.addColorStop(1, cyberAlpha(color.cyan, 0));
    // 绘制青色辉光
    cx.fillStyle = cyan; cx.fillRect(0, 0, w, h);
    // 右侧偏下的紫色径向辉光
    const violet = cx.createRadialGradient(w, h * 0.62, 0, w, h * 0.62, w * 0.82);
    // 辉光渐变
    violet.addColorStop(0, cyberAlpha(color.violet, 0.09)); violet.addColorStop(1, cyberAlpha(color.violet, 0));
    // 绘制紫色辉光
    cx.fillStyle = violet; cx.fillRect(0, 0, w, h);
    // 开始绘制网格线
    cx.beginPath(); cx.lineWidth = timing.gridStroke; cx.strokeStyle = cyberAlpha(color.cyan, 0.035);
    // 竖线
    for (let x = 0; x < w; x += timing.gridStep) { cx.moveTo(x, 0); cx.lineTo(x, h); }
    // 横线
    for (let y = 0; y < h; y += timing.gridStep) { cx.moveTo(0, y); cx.lineTo(w, y); }
    // 一次性描出所有网格线
    cx.stroke();
    // 画布外的原生动画参数让整个背景参与重绘，像素外观不变。
    // 放在最后，使前面的静态背景仍可由 NSH 缓存为绘制命令。
    cx.fillStyle = color.canvas; cx.fillRect(-2, -2, repaint, 1);
  // 绘制回调结束
  }} />;
}

// 表面扫描光效属性
export interface CyberSurfaceScanProps extends CyberDecorationProps { surfaceColor?: string }

/** 固定圆角框中旋转原生渐变图层，内层遮住中心，仅露出绕边光带。
 * 与玻璃扫光共用时钟；不可见或减少动态效果时保留静态光边。 */
// 表面扫描光效：旋转渐变露出绕边光带，另有玻璃扫光
export function CyberSurfaceScan(props: CyberSurfaceScanProps) {
  // 周期相位
  const progress = phase(props, timing.edgeDuration);
  // 实测宽高
  const width = signal(0), height = signal(0);
  // 旋转图层的直径，取对角线加余量
  const diameter = () => Math.hypot(width(), height()) + 4;
  // 扫光带宽度
  const sheenWidth = () => width() * 0.65;
  // 绕边光带的锥形渐变
  const orbit = `conic-gradient(from 0deg, ${cyberAlpha(color.cyan, 0.16)} 0%, ${cyberAlpha(color.cyan, 0.16)} 36%, ${cyberAlpha(color.cyan, 0.96)} 46%, ${cyberAlpha(color.violet, 0.85)} 53%, ${cyberAlpha(color.cyan, 0.16)} 65%, ${cyberAlpha(color.cyan, 0.16)} 100%)`;
  // 圆角裁剪容器
  return <View {...decoration} testID={props.testID}
    // 样式：填充、圆角与裁剪
    style={[fill, { borderRadius: timing.edgeRadius, overflow: "hidden" }, props.style]}
    // 尺寸变化超过阈值才更新
    onLayout={(bounds) => {
      // 宽度变化超过阈值才更新
      if (Math.abs(width() - bounds.size.width) > 0.1) width.set(bounds.size.width);
      // 高度变化超过阈值才更新
      if (Math.abs(height() - bounds.size.height) > 0.1) height.set(bounds.size.height);
    }}>
    {/* View 的旋转单位为度；渐变图层保留在原生端，不逐帧重画角向色块。 */}
    {/* 旋转的渐变图层 */}
    <View style={{ position: "absolute", width: diameter(), height: diameter(),
      // 位置：居中
      left: (width() - diameter()) / 2, top: (height() - diameter()) / 2,
      // 背景渐变与旋转
      backgroundImage: orbit, transform: [{ rotate: progress.mul(360) }] }} />
    {/* 内层遮罩，盖住中心只露出边缘光带 */}
    <View style={{ position: "absolute", top: 1.6, right: 1.6, bottom: 1.6, left: 1.6,
      // 圆角与背景色
      borderRadius: timing.edgeRadius - 1.6, backgroundColor: props.surfaceColor ?? color.surface }} />
    {/* 静态辉光与顶部标记线 */}
    <Canvas {...decoration} style={fill} draw={(cx, { width: w, height: h }) => {
      // 太小不绘制
      if (w <= 6 || h <= 6) return;
      // 圆角半径
      const r = Math.min(timing.edgeRadius, w / 2, h / 2);
      // 裁剪到圆角矩形
      cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, r); cx.clip();
      // 左上青色辉光
      const cyan = cx.createRadialGradient(w * 0.08, 0, 0, w * 0.08, 0, Math.max(w, h * 0.65));
      // 青色渐变起点
      cyan.addColorStop(0, cyberAlpha(color.cyan, 0.075));
      // 青色渐变终点（透明）
      cyan.addColorStop(1, cyberAlpha(color.cyan, 0));
      // 绘制
      cx.fillStyle = cyan; cx.fillRect(0, 0, w, h);
      // 右下紫色辉光
      const violet = cx.createRadialGradient(w, h, 0, w, h, Math.max(w, h * 0.7));
      // 紫色渐变起点
      violet.addColorStop(0, cyberAlpha(color.violet, 0.055));
      // 紫色渐变终点（透明）
      violet.addColorStop(1, cyberAlpha(color.violet, 0));
      // 绘制
      cx.fillStyle = violet; cx.fillRect(0, 0, w, h);
      // 还原裁剪
      cx.restore();
      // 顶部居中的细标记线，青到紫渐变
      const marker = cx.createLinearGradient(w / 2 - 24, 0, w / 2 + 24, 0);
      // 标记渐变起点（透明青）
      marker.addColorStop(0, cyberAlpha(color.cyan, 0));
      // 标记渐变中点（较亮的青）
      marker.addColorStop(0.5, cyberAlpha(color.cyan, 0.48));
      // 标记渐变终点（透明紫）
      marker.addColorStop(1, cyberAlpha(color.violet, 0));
      // 绘制
      cx.fillStyle = marker; cx.fillRect(w / 2 - 24, 1.6, 48, 1);
    // 绘制回调结束
    }} />
    {/* 玻璃扫光的裁剪层 */}
    <View style={{ position: "absolute", top: 2, right: 2, bottom: 2, left: 2,
      // 圆角与裁剪
      borderRadius: timing.edgeRadius - 2, overflow: "hidden" }}>
      {/* 斜向光带，随相位从左滑到右 */}
      <View style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: sheenWidth(),
        // 扫光渐变
        backgroundImage: `linear-gradient(110deg, ${cyberAlpha(color.ink, 0)} 0%, ${cyberAlpha(color.cyan, 0.025)} 46%, ${cyberAlpha(color.ink, 0.045)} 50%, ${cyberAlpha(color.cyan, 0.025)} 56%, ${cyberAlpha(color.ink, 0)} 100%)`,
        // 横向位移
        transform: [{ translateX: progress.mul(width() + sheenWidth() * 2).sub(sheenWidth()) }] }} />
    </View>
  </View>;
}

// 连接指示点属性
export interface CyberConnectionDotProps extends CyberDecorationProps { connected: CyberValue<boolean> }
// 连接指示点：已连接时带呼吸圈
export function CyberConnectionDot(props: CyberConnectionDotProps) {
  // 仅在连接且活跃时才有相位动画
  const progress = phase({ active: () => read(props.connected) && read(props.active ?? true) }, timing.connectionDuration);
  // 画布装饰
  return <Canvas {...decoration} testID={props.testID} style={[{ width: timing.connectionSize, height: timing.connectionSize }, props.style]}
    // 绘制回调
    draw={(cx, { width: w, height: h }) => {
      // 读取连接状态
      const connected = read(props.connected);
      // 已连接青色，未连接灰色
      const tint = connected ? color.cyan : color.disconnected;
      // 已连接：绘制逐渐扩大并变淡的外圈
      if (connected) {
        // 外圈透明度与半径随相位变化
        cx.beginPath(); cx.globalAlpha = progress.mul(-0.4).add(0.4); cx.fillStyle = tint;
        // 画扩散圆
        cx.arc(w / 2, h / 2, progress.mul(1.4).add(3), 0, Math.PI * 2); cx.fill(); cx.globalAlpha = 1;
      }
      // 绘制中心实心点
      cx.beginPath(); cx.fillStyle = tint; cx.arc(w / 2, h / 2, timing.connectionRadius, 0, Math.PI * 2); cx.fill();
    // 绘制回调结束
    }} />;
}

// 入场动效属性，order 决定错峰
export interface CyberArrivalProps extends ViewProps, MotionProps { order?: number }
// 入场容器：淡入、上移并放大到位
export function CyberArrival(props: CyberArrivalProps) {
  // 动效许可
  const allowed = useCyberMotion(props);
  // 入场进度，不允许动效时直接为 1
  const progress = createAnimatedValue(untracked(() => allowed() ? 0 : 1));
  // 是否已完成过入场，避免重复播放
  let completed = false;
  // 许可变化时播放入场
  effect(() => {
    // 不允许或已播放过则直接到位
    if (!allowed() || completed) { progress.set(1); return; }
    // 重置进度
    progress.set(0);
    // 入场动画，延迟按顺序错峰（最多 6 档）
    const enter = Animated.timing(progress, { toValue: 1, duration: timing.arrivalDuration,
      // 延迟：按序号错开
      delay: Math.max(0, Math.min(6, props.order ?? 0)) * timing.arrivalStagger, easing: fastOutSlowIn });
    // 完成后记录
    void enter.start(({ finished }) => { if (finished) completed = true; });
    // 清理时停止
    return () => enter.stop();
  });
  // 拆出自有属性，其余透传
  const [, rest] = split(props, ["order", "active", "style", "children"] as const);
  // 透明度、位移与缩放都由进度推导
  return <View {...rest} style={[props.style, { opacity: progress, transform: [
    // 从下方偏移处移动到原位
    { translateY: progress.mul(-timing.arrivalOffset).add(timing.arrivalOffset) },
    // 从略小的缩放放大到 1
    { scale: progress.mul(1 - timing.arrivalScale).add(timing.arrivalScale) },
  // 数组结束
  ] }]}>{props.children}</View>;
}

// 可按压属性
export interface CyberPressableProps extends ButtonProps, MotionProps {
  /** 获得遥控焦点时调用（电视）。 */
  // 远程焦点回调
  onRemoteFocus?: () => void;
}
/** 保留原生 Button 的语义、键盘激活、悬停/聚焦和禁用
 * 样式;原生手势取消时同样会触发抬起事件。 */
// 可按压容器：保留原生按钮语义，附加按压缩放与遥控支持
export function CyberPressable(props: CyberPressableProps) {
  // 动效许可
  const allowed = useCyberMotion(props);
  // 缩放动画值
  const scale = createAnimatedValue(1);
  // 当前按压动画
  let animation: CompositeAnimation | undefined;
  // 按下缩小、松开恢复；不允许动效或禁用时保持原大小
  const animate = (pressed: boolean) => {
    // 先停止上一段动画
    animation?.stop();
    // 不允许动画或已禁用则复位缩放
    if (!allowed() || props.disabled) { scale.set(1); return; }
    // 缩放到按下比例或复位
    animation = Animated.timing(scale, { toValue: pressed ? timing.pressScale : 1,
      // 时长与缓动
      duration: timing.pressDuration, easing: fastOutSlowIn });
    // 启动动画
    void animation.start();
  };
  // 许可丢失或禁用时立刻复位
  effect(() => { if (!allowed() || props.disabled) { animation?.stop(); scale.set(1); } });
  // 销毁时停止动画
  cleanup(() => animation?.stop());
  // 节点引用，供遥控导航测量
  let node: YNode | undefined;
  // 以 testID 作为遥控目标标识
  const id = props.testID;
  // 有标识才登记为遥控目标
  if (id) registerRemoteTarget(id, { node: () => node, activate: () => props.onPress?.(),
    // 禁用与焦点状态
    disabled: () => props.disabled === true, focused: () => props.onRemoteFocus?.() });
  // 读取圆角，供焦点框使用
  const style = props.style;
  // 取样式里的圆角
  const radius = style && !Array.isArray(style) && typeof style === "object" ? (style as ViewStyle).borderRadius : undefined;
  // 拆出自有属性，其余透传给 Button
  const [, rest] = split(props, ["active", "style", "children", "onPressDown", "onPressUp", "ref", "onRemoteFocus"] as const);
  // 按压缩放通过 transform 呈现
  return <Button {...rest} ref={(value: YNode) => { node = value; }} style={[props.style, { transform: [{ scale }] }]}
    // 按下开始缩小动画
    onPressDown={() => { animate(true); props.onPressDown?.(); }}
    // 松开恢复动画
    onPressUp={() => { animate(false); props.onPressUp?.(); }}>
    {/* 子内容 */}
    {props.children}
    {/* 遥控焦点框 */}
    <RemoteRing id={id} radius={typeof radius === "number" ? radius : 13} />
  </Button>;
}

// 数字动画属性
export interface CyberNumberProps extends MotionProps {
  // 当前显示的文本
  value: string;
  /** 退出中的副本只是装饰:从中省略实时数字的 testID。 */
  // 渲染函数：exiting 为 true 时是退出中的副本
  render: (value: string, exiting: boolean) => JSX.Element;
  // 样式
  style?: StyleProp<ViewStyle>;
  // 测试标识
  testID?: string;
}

/** 源 AnimatedContent 的时序:240ms 淡入 / 300ms +height/3 进入,
 * 150ms 淡出 / 220ms -height/3 退出。只有实时副本可被无障碍访问。 */
// 数字切换动画：新值滑入淡入，旧值滑出淡出
export function CyberNumber(props: CyberNumberProps) {
  // 动效许可
  const allowed = useCyberMotion(props);
  // 当前显示的值
  const shown = signal(untracked(() => props.value));
  // 正在退出的旧值
  const outgoing = signal<string | null>(null);
  // 实测高度，作为位移基准
  const height = signal(0);
  // 新值的透明度与位移
  const inOpacity = createAnimatedValue(1), inY = createAnimatedValue(0);
  // 旧值的透明度与位移
  const outOpacity = createAnimatedValue(0), outY = createAnimatedValue(0);
  // 当前动画
  let animation: CompositeAnimation | undefined;
  // 代数：忽略过期的完成回调
  let generation = 0;
  // 立即收敛到静态终态
  const settle = () => {
    // 代次加一并停止动画，清除旧内容
    generation += 1; animation?.stop(); outgoing.set(null);
    // 重置新旧内容的透明度与位移
    inOpacity.set(1); inY.set(0); outOpacity.set(0); outY.set(0);
  };
  // 值或许可变化时触发
  effect(() => {
    // 读取新值与许可
    const value = props.value, enabled = allowed();
    // 上一次显示的值
    const previous = untracked(shown);
    // 值未变：仅在动效关闭时收敛
    if (value === previous) { if (!enabled) settle(); return; }
    // 先收敛，再切换到新值
    settle(); shown.set(value);
    // 动效关闭则不播放
    if (!enabled) return;
    // 设置初始状态：新值透明待入，旧值显示待出
    outgoing.set(previous); inOpacity.set(0); inY.set(1); outOpacity.set(1);
    // 记录本次代数
    const run = generation;
    // 四段动画并行
    animation = Animated.parallel([
      // 新值淡入
      Animated.timing(inOpacity, { toValue: 1, duration: timing.numberFadeIn, easing: fastOutSlowIn }),
      // 新值滑入
      Animated.timing(inY, { toValue: 0, duration: timing.numberSlideIn, easing: fastOutSlowIn }),
      // 旧值淡出
      Animated.timing(outOpacity, { toValue: 0, duration: timing.numberFadeOut, easing: fastOutSlowIn }),
      // 旧值滑出
      Animated.timing(outY, { toValue: -1, duration: timing.numberSlideOut, easing: fastOutSlowIn }),
    ]);
    // 完成且未过期则移除旧值
    void animation.start(({ finished }) => { if (finished && run === generation) outgoing.set(null); });
  });
  // 销毁时使回调失效并停止动画
  cleanup(() => { generation += 1; animation?.stop(); });
  // 容器
  return <View testID={props.testID} style={[{ position: "relative" }, props.style]}>
    {/* 新值层，同时测量高度 */}
    <View onLayout={(bounds) => height.set(bounds.size.height)}
      // 样式：透明度与纵向位移
      style={{ opacity: inOpacity, transform: [{ translateY: inY.mul(height() / 3) }] }}>
      {/* 渲染新值（可被无障碍读取） */}
      {() => props.render(shown(), false)}
    </View>
    {/* 退出中的旧值覆盖在上方，仅为装饰 */}
    {() => outgoing() == null ? null : <View {...decoration}
      // 样式：绝对定位与透明度
      style={{ position: "absolute", top: 0, left: 0, right: 0, opacity: outOpacity,
        // 纵向位移
        transform: [{ translateY: outY.mul(height() / 3) }] }}>{props.render(outgoing()!, true)}</View>}
  </View>;
}
