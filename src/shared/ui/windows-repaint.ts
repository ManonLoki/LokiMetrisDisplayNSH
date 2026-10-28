import { Animated, createAnimatedValue, effect, platform, type Accessor, type CanvasNumber } from "nsh";

/** NSH 0.3.0 在本机 Windows 上局部重绘可能留下残影；这是应用侧兼容处理，
 * 不是已经确认的上游修复。原生动画让全屏背景持续参与重绘，会增加 GPU 开销，
 * 因而仅在 Windows 前台启用；后台和作用域销毁时停止并复位。
 * 该值不改变可见外观，不受“减少动态效果”限制，也不逐帧更新 JS 信号。 */
export function useWindowsRepaint(foreground: Accessor<boolean>): CanvasNumber {
  if (platform.os !== "windows") return 0;
  const value = createAnimatedValue(0);
  effect(() => {
    value.set(0);
    if (!foreground()) return;
    const loop = Animated.loop(Animated.timing(value, { toValue: 1, duration: 1_000, easing: "linear" }));
    void loop.start();
    return () => { loop.stop(); value.set(0); };
  });
  return value;
}
