/** 图片控件自己的可见播放时钟；暂停期间不累计时间，每个控件独立持有。 */
export interface ImageFrame { source: string; durationMs: number }
export interface ImageSequence { frames: ImageFrame[]; plays: number | null }
export interface PlaybackClock {
  now: () => number;
  schedule: (callback: () => void, delayMs: number) => unknown;
  cancel: (timer: unknown) => void;
}

export class ImagePlayback {
  private elapsed = 0;
  private started = 0;
  private playing = false;
  private disposed = false;
  private timer: unknown;
  private index = -1;
  private readonly ends: number[];
  private readonly duration: number;

  constructor(private sequence: ImageSequence, private clock: PlaybackClock,
    private show: (source: string) => void) {
    let total = 0;
    this.ends = sequence.frames.map((frame) => total += frame.durationMs);
    this.duration = total;
    this.update(0);
  }

  setPlaying(playing: boolean): void {
    if (this.disposed || playing === this.playing) return;
    if (this.playing) this.elapsed += Math.max(0, this.clock.now() - this.started);
    this.clear();
    this.playing = playing;
    this.started = this.clock.now();
    this.update(this.elapsed);
  }

  dispose(): void {
    this.disposed = true;
    this.playing = false;
    this.clear();
  }

  private clear(): void {
    if (this.timer !== undefined) this.clock.cancel(this.timer);
    this.timer = undefined;
  }

  private update(elapsed: number): void {
    if (this.disposed || !this.sequence.frames.length || this.duration <= 0) return;
    const finished = this.sequence.plays != null && elapsed >= this.duration * this.sequence.plays;
    const position = elapsed % this.duration;
    const next = finished ? this.ends.length - 1 : this.ends.findIndex((end) => position < end);
    if (next !== this.index) {
      this.index = next;
      this.show(this.sequence.frames[next].source);
    }
    if (finished || !this.playing || this.sequence.frames.length === 1) return;
    this.timer = this.clock.schedule(() => {
      this.timer = undefined;
      this.update(this.elapsed + Math.max(0, this.clock.now() - this.started));
    }, Math.max(1, Math.ceil(this.ends[next] - position)));
  }
}
