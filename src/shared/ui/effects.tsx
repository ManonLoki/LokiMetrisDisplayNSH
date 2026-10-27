import { Canvas, View, cleanup, context, effect, signal, split, untracked, type Accessor, type JSX, type StyleProp, type ViewProps, type ViewStyle, type YNode, Animated, createAnimatedValue, type CompositeAnimation, type Easing, reduceMotion } from "nsh";
import { Button, type ButtonProps } from "nsh/base";
import { cyberAlpha, cyberColors as color, cyberMotion as timing } from "./theme";
import { RemoteRing, registerRemoteTarget } from "./remote-focus";

export type CyberValue<T> = T | Accessor<T>;
const read = <T,>(value: CyberValue<T>): T => typeof value === "function" ? (value as Accessor<T>)() : value;
const foreground = signal(false);
const Motion = context<Accessor<boolean>>(() => false);
const fastOutSlowIn: Easing = [0.4, 0, 0.2, 1];
const fill: ViewStyle = { position: "absolute", top: 0, right: 0, bottom: 0, left: 0 };
const decoration = { pointerEvents: "none" as const, accessible: false,
  accessibilityElementsHidden: true, importantForAccessibility: "no-hide-descendants" as const };

/** Called by a real host lifecycle source, never inferred from network status.
 * NSH currently exposes reduceMotion but no public AppState/scenePhase API. */
export function setCyberForeground(active: boolean): void { foreground.set(active); }

export interface CyberMotionScopeProps {
  enabled?: CyberValue<boolean>;
  foreground?: CyberValue<boolean>;
  children?: JSX.Element;
}

/** Without explicit foreground knowledge all effects remain at their static
 * final appearance. Page selection/detail presentation can disable the scope. */
export function CyberMotionScope(props: CyberMotionScopeProps) {
  return <Motion.Provider value={() => read(props.enabled ?? true)
    && read(props.foreground ?? foreground) && !reduceMotion()}>{props.children}</Motion.Provider>;
}

export interface MotionProps { active?: CyberValue<boolean> }
/** Shared by decorative effects and native animated images. Hidden pages,
 * inactive windows and the system reduced-motion preference use one gate. */
export function useCyberMotion(props: MotionProps = {}): Accessor<boolean> {
  const scope = Motion();
  return () => scope() && read(props.active ?? true) && !reduceMotion();
}

function phase(props: MotionProps, duration: number) {
  const allowed = useCyberMotion(props);
  const value = createAnimatedValue(0);
  effect(() => {
    value.set(0);
    if (!allowed()) return;
    const loop = Animated.loop(Animated.timing(value, { toValue: 1, duration, easing: "linear" }));
    void loop.start();
    return () => { loop.stop(); value.set(0); };
  });
  return value;
}

export interface CyberDecorationProps extends MotionProps {
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

/** Android backdrop: two radial glows and the source 24dp, 0.5dp grid.
 * It is deliberately static, as the Android source backdrop is static. */
export function CyberBackdrop(props: CyberDecorationProps) {
  return <Canvas {...decoration} testID={props.testID} style={[fill, props.style]} draw={(cx, size) => {
    const { width: w, height: h } = size;
    cx.fillStyle = color.canvas; cx.fillRect(0, 0, w, h);
    if (w <= 0 || h <= 0) return;
    const cyan = cx.createRadialGradient(w * 0.02, h * 0.06, 0, w * 0.02, h * 0.06, w * 0.85);
    cyan.addColorStop(0, cyberAlpha(color.cyan, 0.11)); cyan.addColorStop(1, cyberAlpha(color.cyan, 0));
    cx.fillStyle = cyan; cx.fillRect(0, 0, w, h);
    const violet = cx.createRadialGradient(w, h * 0.62, 0, w, h * 0.62, w * 0.82);
    violet.addColorStop(0, cyberAlpha(color.violet, 0.09)); violet.addColorStop(1, cyberAlpha(color.violet, 0));
    cx.fillStyle = violet; cx.fillRect(0, 0, w, h);
    cx.beginPath(); cx.lineWidth = timing.gridStroke; cx.strokeStyle = cyberAlpha(color.cyan, 0.035);
    for (let x = 0; x < w; x += timing.gridStep) { cx.moveTo(x, 0); cx.lineTo(x, h); }
    for (let y = 0; y < h; y += timing.gridStep) { cx.moveTo(0, y); cx.lineTo(w, y); }
    cx.stroke();
  }} />;
}

export interface CyberSurfaceScanProps extends CyberDecorationProps { surfaceColor?: string }

/** 固定圆角框中旋转原生渐变图层，内层遮住中心，仅露出绕边光带。
 * 与玻璃扫光共用时钟；不可见或减少动态效果时保留静态光边。 */
export function CyberSurfaceScan(props: CyberSurfaceScanProps) {
  const progress = phase(props, timing.edgeDuration);
  const width = signal(0), height = signal(0);
  const diameter = () => Math.hypot(width(), height()) + 4;
  const sheenWidth = () => width() * 0.65;
  const orbit = `conic-gradient(from 0deg, ${cyberAlpha(color.cyan, 0.16)} 0%, ${cyberAlpha(color.cyan, 0.16)} 36%, ${cyberAlpha(color.cyan, 0.96)} 46%, ${cyberAlpha(color.violet, 0.85)} 53%, ${cyberAlpha(color.cyan, 0.16)} 65%, ${cyberAlpha(color.cyan, 0.16)} 100%)`;
  return <View {...decoration} testID={props.testID}
    style={[fill, { borderRadius: timing.edgeRadius, overflow: "hidden" }, props.style]}
    onLayout={(bounds) => {
      if (Math.abs(width() - bounds.size.width) > 0.1) width.set(bounds.size.width);
      if (Math.abs(height() - bounds.size.height) > 0.1) height.set(bounds.size.height);
    }}>
    {/* View 的旋转单位为度；渐变图层保留在原生端，不逐帧重画角向色块。 */}
    <View style={{ position: "absolute", width: diameter(), height: diameter(),
      left: (width() - diameter()) / 2, top: (height() - diameter()) / 2,
      backgroundImage: orbit, transform: [{ rotate: progress.mul(360) }] }} />
    <View style={{ position: "absolute", top: 1.6, right: 1.6, bottom: 1.6, left: 1.6,
      borderRadius: timing.edgeRadius - 1.6, backgroundColor: props.surfaceColor ?? color.surface }} />
    <Canvas {...decoration} style={fill} draw={(cx, { width: w, height: h }) => {
      if (w <= 6 || h <= 6) return;
      const r = Math.min(timing.edgeRadius, w / 2, h / 2);
      cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, r); cx.clip();
      const cyan = cx.createRadialGradient(w * 0.08, 0, 0, w * 0.08, 0, Math.max(w, h * 0.65));
      cyan.addColorStop(0, cyberAlpha(color.cyan, 0.075));
      cyan.addColorStop(1, cyberAlpha(color.cyan, 0));
      cx.fillStyle = cyan; cx.fillRect(0, 0, w, h);
      const violet = cx.createRadialGradient(w, h, 0, w, h, Math.max(w, h * 0.7));
      violet.addColorStop(0, cyberAlpha(color.violet, 0.055));
      violet.addColorStop(1, cyberAlpha(color.violet, 0));
      cx.fillStyle = violet; cx.fillRect(0, 0, w, h);
      cx.restore();
      const marker = cx.createLinearGradient(w / 2 - 24, 0, w / 2 + 24, 0);
      marker.addColorStop(0, cyberAlpha(color.cyan, 0));
      marker.addColorStop(0.5, cyberAlpha(color.cyan, 0.48));
      marker.addColorStop(1, cyberAlpha(color.violet, 0));
      cx.fillStyle = marker; cx.fillRect(w / 2 - 24, 1.6, 48, 1);
    }} />
    <View style={{ position: "absolute", top: 2, right: 2, bottom: 2, left: 2,
      borderRadius: timing.edgeRadius - 2, overflow: "hidden" }}>
      <View style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: sheenWidth(),
        backgroundImage: `linear-gradient(110deg, ${cyberAlpha(color.ink, 0)} 0%, ${cyberAlpha(color.cyan, 0.025)} 46%, ${cyberAlpha(color.ink, 0.045)} 50%, ${cyberAlpha(color.cyan, 0.025)} 56%, ${cyberAlpha(color.ink, 0)} 100%)`,
        transform: [{ translateX: progress.mul(width() + sheenWidth() * 2).sub(sheenWidth()) }] }} />
    </View>
  </View>;
}

export interface CyberConnectionDotProps extends CyberDecorationProps { connected: CyberValue<boolean> }
export function CyberConnectionDot(props: CyberConnectionDotProps) {
  const progress = phase({ active: () => read(props.connected) && read(props.active ?? true) }, timing.connectionDuration);
  return <Canvas {...decoration} testID={props.testID} style={[{ width: timing.connectionSize, height: timing.connectionSize }, props.style]}
    draw={(cx, { width: w, height: h }) => {
      const connected = read(props.connected);
      const tint = connected ? color.cyan : color.disconnected;
      if (connected) {
        cx.beginPath(); cx.globalAlpha = progress.mul(-0.4).add(0.4); cx.fillStyle = tint;
        cx.arc(w / 2, h / 2, progress.mul(1.4).add(3), 0, Math.PI * 2); cx.fill(); cx.globalAlpha = 1;
      }
      cx.beginPath(); cx.fillStyle = tint; cx.arc(w / 2, h / 2, timing.connectionRadius, 0, Math.PI * 2); cx.fill();
    }} />;
}

export interface CyberArrivalProps extends ViewProps, MotionProps { order?: number }
export function CyberArrival(props: CyberArrivalProps) {
  const allowed = useCyberMotion(props);
  const progress = createAnimatedValue(untracked(() => allowed() ? 0 : 1));
  let completed = false;
  effect(() => {
    if (!allowed() || completed) { progress.set(1); return; }
    progress.set(0);
    const enter = Animated.timing(progress, { toValue: 1, duration: timing.arrivalDuration,
      delay: Math.max(0, Math.min(6, props.order ?? 0)) * timing.arrivalStagger, easing: fastOutSlowIn });
    void enter.start(({ finished }) => { if (finished) completed = true; });
    return () => enter.stop();
  });
  const [, rest] = split(props, ["order", "active", "style", "children"] as const);
  return <View {...rest} style={[props.style, { opacity: progress, transform: [
    { translateY: progress.mul(-timing.arrivalOffset).add(timing.arrivalOffset) },
    { scale: progress.mul(1 - timing.arrivalScale).add(timing.arrivalScale) },
  ] }]}>{props.children}</View>;
}

export interface CyberPressableProps extends ButtonProps, MotionProps {
  /** 获得遥控焦点时调用（电视）。 */
  onRemoteFocus?: () => void;
}
/** Native Button semantics, keyboard activation, hover/focus and disabled
 * styles are retained; press-up also fires for native gesture cancellation. */
export function CyberPressable(props: CyberPressableProps) {
  const allowed = useCyberMotion(props);
  const scale = createAnimatedValue(1);
  let animation: CompositeAnimation | undefined;
  const animate = (pressed: boolean) => {
    animation?.stop();
    if (!allowed() || props.disabled) { scale.set(1); return; }
    animation = Animated.timing(scale, { toValue: pressed ? timing.pressScale : 1,
      duration: timing.pressDuration, easing: fastOutSlowIn });
    void animation.start();
  };
  effect(() => { if (!allowed() || props.disabled) { animation?.stop(); scale.set(1); } });
  cleanup(() => animation?.stop());
  let node: YNode | undefined;
  const id = props.testID;
  if (id) registerRemoteTarget(id, { node: () => node, activate: () => props.onPress?.(),
    disabled: () => props.disabled === true, focused: () => props.onRemoteFocus?.() });
  const style = props.style;
  const radius = style && !Array.isArray(style) && typeof style === "object" ? (style as ViewStyle).borderRadius : undefined;
  const [, rest] = split(props, ["active", "style", "children", "onPressDown", "onPressUp", "ref", "onRemoteFocus"] as const);
  return <Button {...rest} ref={(value: YNode) => { node = value; }} style={[props.style, { transform: [{ scale }] }]}
    onPressDown={() => { animate(true); props.onPressDown?.(); }}
    onPressUp={() => { animate(false); props.onPressUp?.(); }}>
    {props.children}
    <RemoteRing id={id} radius={typeof radius === "number" ? radius : 13} />
  </Button>;
}

export interface CyberNumberProps extends MotionProps {
  value: string;
  /** The exiting copy is decorative: omit the live number's testID from it. */
  render: (value: string, exiting: boolean) => JSX.Element;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

/** Source AnimatedContent timing: 240ms fade / 300ms +height/3 arrival,
 * 150ms fade / 220ms -height/3 exit. Only the live copy is accessible. */
export function CyberNumber(props: CyberNumberProps) {
  const allowed = useCyberMotion(props);
  const shown = signal(untracked(() => props.value));
  const outgoing = signal<string | null>(null);
  const height = signal(0);
  const inOpacity = createAnimatedValue(1), inY = createAnimatedValue(0);
  const outOpacity = createAnimatedValue(0), outY = createAnimatedValue(0);
  let animation: CompositeAnimation | undefined;
  let generation = 0;
  const settle = () => {
    generation += 1; animation?.stop(); outgoing.set(null);
    inOpacity.set(1); inY.set(0); outOpacity.set(0); outY.set(0);
  };
  effect(() => {
    const value = props.value, enabled = allowed();
    const previous = untracked(shown);
    if (value === previous) { if (!enabled) settle(); return; }
    settle(); shown.set(value);
    if (!enabled) return;
    outgoing.set(previous); inOpacity.set(0); inY.set(1); outOpacity.set(1);
    const run = generation;
    animation = Animated.parallel([
      Animated.timing(inOpacity, { toValue: 1, duration: timing.numberFadeIn, easing: fastOutSlowIn }),
      Animated.timing(inY, { toValue: 0, duration: timing.numberSlideIn, easing: fastOutSlowIn }),
      Animated.timing(outOpacity, { toValue: 0, duration: timing.numberFadeOut, easing: fastOutSlowIn }),
      Animated.timing(outY, { toValue: -1, duration: timing.numberSlideOut, easing: fastOutSlowIn }),
    ]);
    void animation.start(({ finished }) => { if (finished && run === generation) outgoing.set(null); });
  });
  cleanup(() => { generation += 1; animation?.stop(); });
  return <View testID={props.testID} style={[{ position: "relative" }, props.style]}>
    <View onLayout={(bounds) => height.set(bounds.size.height)}
      style={{ opacity: inOpacity, transform: [{ translateY: inY.mul(height() / 3) }] }}>
      {() => props.render(shown(), false)}
    </View>
    {() => outgoing() == null ? null : <View {...decoration}
      style={{ position: "absolute", top: 0, left: 0, right: 0, opacity: outOpacity,
        transform: [{ translateY: outY.mul(height() / 3) }] }}>{props.render(outgoing()!, true)}</View>}
  </View>;
}
