/** 图片控件自己的可见播放时钟；暂停期间不累计时间，每个控件独立持有。 */
// 单帧：图片地址与时长
export interface ImageFrame { source: string; durationMs: number }
// 帧序列与播放次数，null 表示无限循环
export interface ImageSequence { frames: ImageFrame[]; plays?: number | null }
// 时钟抽象，便于测试替换
export interface PlaybackClock {
  // 当前时间（毫秒）
  now: () => number;
  // 延迟执行
  schedule: (callback: () => void, delayMs: number) => unknown;
  // 取消定时器
  cancel: (timer: unknown) => void;
}

// 播放器
export class ImagePlayback {
  // 已累计的播放时间
  private elapsed = 0;
  // 本次开始播放的时刻
  private started = 0;
  // 是否正在播放
  private playing = false;
  // 是否已销毁
  private disposed = false;
  // 下一帧定时器
  private timer: unknown;
  // 当前显示的帧下标
  private index = -1;
  // 各帧结束的累计时间
  private readonly ends: number[];
  // 单轮总时长
  private readonly duration: number;

  // 构造时计算累计时间并显示第一帧
  constructor(private sequence: ImageSequence, private clock: PlaybackClock,
    // 显示回调
    private show: (source: string) => void) {
    // 累计时长
    let total = 0;
    // 依次累加各帧时长
    this.ends = sequence.frames.map((frame) => total += frame.durationMs);
    // 记录单轮总时长
    this.duration = total;
    // 显示起始帧
    this.update(0);
  }

  // 切换播放状态：暂停期间不累计时间
  setPlaying(playing: boolean): void {
    // 已销毁或状态未变则忽略
    if (this.disposed || playing === this.playing) return;
    // 停止前把本段播放时间计入累计
    if (this.playing) this.elapsed += Math.max(0, this.clock.now() - this.started);
    // 取消旧定时器
    this.clear();
    // 更新状态
    this.playing = playing;
    // 记录新的起点
    this.started = this.clock.now();
    // 按累计时间刷新画面并安排下一帧
    this.update(this.elapsed);
  }

  // 销毁播放器，停止定时器
  dispose(): void {
    // 标记已销毁
    this.disposed = true;
    // 停止播放
    this.playing = false;
    // 清除画面
    this.clear();
  }

  // 取消待执行的定时器
  private clear(): void {
    // 有定时器则取消
    if (this.timer !== undefined) this.clock.cancel(this.timer);
    // 清空定时器
    this.timer = undefined;
  }

  // 根据累计时间刷新当前帧并安排下一次
  private update(elapsed: number): void {
    // 已销毁或没有有效帧则不处理
    if (this.disposed || !this.sequence.frames.length || this.duration <= 0) return;
    // 是否已播完指定轮数
    const finished = this.sequence.plays != null && elapsed >= this.duration * this.sequence.plays;
    // 当前轮内的位置
    const position = elapsed % this.duration;
    // 播完则停在最后一帧，否则找到包含当前位置的帧
    const next = finished ? this.ends.length - 1 : this.ends.findIndex((end) => position < end);
    // 帧变化才通知显示
    if (next !== this.index) {
      // 记录当前帧下标
      this.index = next;
      // 显示该帧
      this.show(this.sequence.frames[next].source);
    }
    // 播完、暂停或只有一帧时不再调度
    if (finished || !this.playing || this.sequence.frames.length === 1) return;
    // 在当前帧结束时重新计算
    this.timer = this.clock.schedule(() => {
      // 清空定时器
      this.timer = undefined;
      // 累加已播放时间
      this.update(this.elapsed + Math.max(0, this.clock.now() - this.started));
    // 距离下一帧结束的延迟，至少 1ms
    }, Math.max(1, Math.ceil(this.ends[next] - position)));
  }
}
