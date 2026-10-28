// 引入 nsh 的图片、视图与响应式原语
import { Image, View, cleanup, effect, signal, split, untracked, type ImageProps, type Size } from "nsh";
// 引入动效许可钩子
import { useCyberMotion, type MotionProps } from "./effects";
// 引入图片预处理
import { ImageFrames } from "loki-metris-mobile-display/native";
// 引入播放器
import { ImagePlayback } from "./image-playback";

// 图片组件：静态图直接显示，动图由播放器逐帧切换
export function CyberImage(props: ImageProps & MotionProps) {
  // 当前是否允许播放动效
  const allowed = useCyberMotion(props);
  // 实际交给原生 Image 的地址
  const source = signal<ImageProps["src"]>(undefined);
  // 当前动图播放器
  const player = signal<ImagePlayback | null>(null);
  // 容器实测尺寸
  const frame = signal<{ width: number; height: number } | null>(null);
  // 代数：丢弃过期的异步结果
  let generation = 0;
  // 是否已触发加载完成
  let loaded = false;
  // 加载结果（成功或失败）是否已上报
  let settled = false;
  // 当前图片是否为动图
  let animated = false;
  // 取出需要自行处理的属性，其余透传给原生 Image
  const [, nativeProps] = split(props, ["active", "src", "style", "onLayout", "onLoad", "onLoadStart", "onLoadEnd", "onError"] as const);
  // 出错时销毁播放器，并只上报一次
  const onError = (error: { message: string }) => {
    // 释放播放器
    untracked(player)?.dispose();
    // 首次出错时上报错误并结束加载
    if (!settled) { settled = true; props.onError?.(error); props.onLoadEnd?.(); }
  };
  // 地址变化时重新准备图片
  effect(() => {
    // 当前地址
    const original = props.src;
    // 递增代数
    const current = ++generation;
    // 清理旧播放器
    untracked(player)?.dispose(); player.set(null);
    // 重置所有状态
    loaded = false; settled = false; animated = false; source.set(undefined);
    // 无地址则不加载
    if (!original) return;
    // 副作用是否已被清理
    let disposed = false;
    // 异步准备图片
    void ImageFrames.prepare(typeof original === "string" ? original : original.url).then((prepared) => {
      // 结果过期则丢弃
      if (disposed || current !== generation) return;
      // 记录是否动图
      animated = prepared.animated;
      // 静态图直接使用原地址
      if (!prepared.animated) { source.set(original); return; }
      // 动图：创建播放器并用真实时钟驱动
      const playback = new ImagePlayback(prepared, {
        // 时钟与定时调度
        now: () => performance.now(), schedule: (callback, delay) => setTimeout(callback, delay),
        // 取消定时器
        cancel: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      // 每切换一帧更新显示地址
      }, (next) => source.set(next));
      // 保存播放器
      player.set(playback);
    // 准备失败时上报错误
    }).catch((error) => {
      // 仍是当前代次才上报错误
      if (!disposed && current === generation) onError({ message: String(error) });
    });
    // 清理：标记失效并销毁播放器
    return () => { disposed = true; untracked(player)?.dispose(); };
  });
  // 随动效许可开始或暂停播放
  effect(() => { player()?.setPlaying(allowed()); });
  // 组件销毁时使旧结果失效并销毁播放器
  cleanup(() => { generation += 1; untracked(player)?.dispose(); });
  // 首次加载成功时上报一次
  const onLoad = (size: Size) => {
    // 首次加载成功时上报尺寸并结束加载
    if (!loaded && !settled) { loaded = true; settled = true; props.onLoad?.(size); props.onLoadEnd?.(); }
  };
  // 等尺寸明确后才挂载原生 Image；放大后重建图片视图以触发官方解码。
  // 等尺寸明确后才挂载原生 Image；尺寸明显变化时重建以触发解码
  return <View style={props.style} onLayout={(bounds) => {
    // 先转发外部布局回调
    props.onLayout?.(bounds);
    // 对比旧尺寸与新尺寸
    const old = untracked(frame), next = bounds.size;
    // 有效且变化超过 0.5 才更新
    if (next.width > 0 && next.height > 0 && (!old || Math.abs(old.width - next.width) > 0.5 || Math.abs(old.height - next.height) > 0.5)) {
      // 记录帧尺寸
      frame.set({ width: next.width, height: next.height });
    }
  }}>
    {/* 以尺寸为 key，尺寸变化时重建 Image */}
    <Each of={frame() ? [frame()!] : []} key={(size) => `${size.width}x${size.height}`}>{(size) =>
      <Image {...nativeProps} src={source()} style={{ width: size().width, height: size().height }}
        // 动图不做淡入
        fadeDuration={animated ? 0 : props.fadeDuration}
        // 加载开始、成功与出错回调
        onLoadStart={() => { if (!loaded && !settled) props.onLoadStart?.(); }} onLoad={onLoad} onError={onError} />}
    </Each>
  </View>;
}
