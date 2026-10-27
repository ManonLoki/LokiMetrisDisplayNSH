import { Image, View, cleanup, effect, signal, split, untracked, type ImageProps, type Size } from "nsh";
import { useCyberMotion, type MotionProps } from "./effects";
import { prepareImage } from "./image-frames";
import { ImagePlayback } from "./image-playback";

export function CyberImage(props: ImageProps & MotionProps) {
  const allowed = useCyberMotion(props);
  const source = signal<ImageProps["src"]>(undefined);
  const player = signal<ImagePlayback | null>(null);
  const frame = signal<{ width: number; height: number } | null>(null);
  let generation = 0;
  let loaded = false;
  let settled = false;
  let animated = false;
  const [, nativeProps] = split(props, ["active", "src", "style", "onLayout", "onLoad", "onLoadStart", "onLoadEnd", "onError"] as const);
  const onError = (error: { message: string }) => {
    untracked(player)?.dispose();
    if (!settled) { settled = true; props.onError?.(error); props.onLoadEnd?.(); }
  };
  effect(() => {
    const original = props.src;
    const current = ++generation;
    untracked(player)?.dispose(); player.set(null);
    loaded = false; settled = false; animated = false; source.set(undefined);
    if (!original) return;
    let disposed = false;
    void prepareImage(original).then((prepared) => {
      if (disposed || current !== generation) return;
      animated = prepared.animated;
      if (!prepared.animated) { source.set(original); return; }
      const playback = new ImagePlayback(prepared, {
        now: () => performance.now(), schedule: (callback, delay) => setTimeout(callback, delay),
        cancel: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      }, (next) => source.set(next));
      player.set(playback);
    }).catch((error) => {
      if (!disposed && current === generation) onError({ message: String(error) });
    });
    return () => { disposed = true; untracked(player)?.dispose(); };
  });
  effect(() => { player()?.setPlaying(allowed()); });
  cleanup(() => { generation += 1; untracked(player)?.dispose(); });
  const onLoad = (size: Size) => {
    if (!loaded && !settled) { loaded = true; settled = true; props.onLoad?.(size); props.onLoadEnd?.(); }
  };
  // 等尺寸明确后才挂载原生 Image；放大后重建图片视图以触发官方解码。
  return <View style={props.style} onLayout={(bounds) => {
    props.onLayout?.(bounds);
    const old = untracked(frame), next = bounds.size;
    if (next.width > 0 && next.height > 0 && (!old || Math.abs(old.width - next.width) > 0.5 || Math.abs(old.height - next.height) > 0.5)) {
      frame.set({ width: next.width, height: next.height });
    }
  }}>
    <Each of={frame() ? [frame()!] : []} key={(size) => `${size.width}x${size.height}`}>{(size) =>
      <Image {...nativeProps} src={source()} style={{ width: size().width, height: size().height }}
        fadeDuration={animated ? 0 : props.fadeDuration}
        onLoadStart={() => { if (!loaded && !settled) props.onLoadStart?.(); }} onLoad={onLoad} onError={onError} />}
    </Each>
  </View>;
}
