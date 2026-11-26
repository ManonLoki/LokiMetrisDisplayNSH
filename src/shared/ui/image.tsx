import { Image, View, cleanup, effect, signal, split, untracked, type ImageProps, type ImageStyle, type PixelSize, type StyleProp } from "nsh";
import { useCyberMotion, type MotionProps } from "./effects";
import { ImageFrames } from "loki-metris-mobile-display/native";
import { ImagePlayback } from "./image-playback";

// NSH 样式列表是只读递归数组，保留该类型以便缩窄列表外的样式对象。
function isStyleList(style: StyleProp<ImageStyle>): style is readonly StyleProp<ImageStyle>[] {
  return Array.isArray(style);
}

// 图片绘制属性属于每个原生 Image；布局、边框与内边距只应用于外层容器。
function imageStyle(style: StyleProp<ImageStyle>): StyleProp<ImageStyle> {
  if (isStyleList(style)) return style.map(imageStyle);
  return style && { objectFit: style.objectFit, objectPosition: style.objectPosition, tintColor: style.tintColor };
}

// 图片组件：静态图直接显示，动图由播放器逐帧切换
export function CyberImage(props: ImageProps & MotionProps) {
  // 当前是否允许播放动效
  const allowed = useCyberMotion(props);
  // 实际交给原生 Image 的地址
  const source = signal<ImageProps["src"]>(undefined);
  // 双缓冲：新帧先在隐藏层加载完成再显示，避免换帧时旧帧先消失导致闪烁
  const slots = [signal<ImageProps["src"]>(undefined), signal<ImageProps["src"]>(undefined)];
  const shown = signal(0);
  // 等待显示的层与兜底定时器
  let target = -1;
  let swapTimer: ReturnType<typeof setTimeout> | undefined;
  // 隐藏层加载完成（或超时）后切换显示
  const reveal = (slot: number) => {
    if (slot !== target) return;
    clearTimeout(swapTimer); target = -1; shown.set(slot);
  };
  const player = signal<ImagePlayback | null>(null);
  // 代数：丢弃过期的异步结果
  let generation = 0;
  // 加载结果（成功或失败）是否已上报
  let settled = false;
  // 取出需要自行处理的属性，其余透传给原生 Image
  const [, nativeProps] = split(props, [
    "active", "src", "style", "testID", "fadeDuration", "accessibilityElementsHidden",
    "onLayout", "onLoad", "onLoadStart", "onLoadEnd", "onError",
  ] as const);
  // 出错时销毁播放器，并只上报一次
  const onError = (error: { message: string }) => {
    untracked(player)?.dispose();
    // 首次出错时上报错误并结束加载
    if (!settled) { settled = true; props.onError?.(error); props.onLoadEnd?.(); }
  };
  // 地址变化时重新准备图片
  effect(() => {
    const original = props.src;
    const current = ++generation;
    untracked(player)?.dispose(); player.set(null);
    settled = false; source.set(undefined);
    if (!original) return;
    // 副作用是否已被清理
    let disposed = false;
    void ImageFrames.prepare(typeof original === "string" ? original : original.url).then((prepared) => {
      if (disposed || current !== generation) return;
      // 静态图直接使用原地址
      if (!prepared.animated) { source.set(original); return; }
      // 动图：创建播放器并用真实时钟驱动
      const playback = new ImagePlayback(prepared, {
        now: () => performance.now(), schedule: (callback, delay) => setTimeout(callback, delay),
        cancel: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      // 每切换一帧更新显示地址
      }, (next) => source.set(next));
      player.set(playback);
    // 准备失败时上报错误
    }).catch((error) => {
      // 仍是当前代次才上报错误
      if (!disposed && current === generation) onError({ message: String(error) });
    });
    // 清理：标记失效并销毁播放器
    return () => { disposed = true; untracked(player)?.dispose(); };
  });
  // 地址变化时写入隐藏层，等待其加载完成后再显示
  effect(() => {
    const next = source();
    untracked(() => {
      clearTimeout(swapTimer);
      if (next === undefined) { target = -1; slots[0].set(undefined); slots[1].set(undefined); shown.set(0); return; }
      const hidden = 1 - shown();
      target = hidden; slots[hidden].set(next);
      // onLoad 未触发（如缓存命中）时的兜底
      swapTimer = setTimeout(() => reveal(hidden), 120);
    });
  });
  // 随动效许可开始或暂停播放
  effect(() => { player()?.setPlaying(allowed()); });
  // 组件销毁时使旧结果失效并销毁播放器
  cleanup(() => { clearTimeout(swapTimer); generation += 1; untracked(player)?.dispose(); });
  // 首次加载成功时上报一次
  const onLoad = (size: PixelSize, slot: number) => {
    // 隐藏层加载完成，立即切换
    reveal(slot);
    // 首次加载成功时上报尺寸并结束加载
    if (!settled) { settled = true; props.onLoad?.({ size }); props.onLoadEnd?.(); }
  };
  // 保持两层 Image 的身份稳定，由 NSH 根据布局尺寸选择解码尺寸。
  return <View style={props.style} onLayout={props.onLayout}>
    <Each of={[0, 1]} key="index">{(slot) =>
      <Image {...nativeProps} src={slots[slot()]()}
        testID={shown() === slot() ? props.testID : undefined}
        accessibilityElementsHidden={shown() !== slot() || props.accessibilityElementsHidden}
        style={[imageStyle(props.style), { position: "absolute", top: 0, left: 0, width: "100%", height: "100%", opacity: shown() === slot() ? 1 : 0 }]}
        fadeDuration={player() ? 0 : props.fadeDuration}
        onLoadStart={() => { if (!settled) props.onLoadStart?.(); }} onLoad={({ size: loadedSize }) => onLoad(loadedSize, slot())} onError={onError} />}
    </Each>
  </View>;
}
