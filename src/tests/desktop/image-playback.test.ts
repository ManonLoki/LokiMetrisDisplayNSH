// 引入测试 API
import { expect, test } from "nsh/test";
// 引入播放器与时钟类型
import { ImagePlayback, type PlaybackClock } from "../../shared/ui/image-playback";
// 引入图片预处理
import { ImageFrames } from "loki-metris-mobile-display/native";

// 静态 PNG 与 JPEG 不走动图路径
test("static PNG and JPEG data URIs stay with the standard Image decoder", async () => {
  // 逐个检查
  for (const source of ["data:image/png;base64,iVBORw0KGgo=", "data:image/jpeg;base64,/9j/4AAQSkZJRg=="]) {
    // 应判定为非动图
    expect(((await ImageFrames.prepare(source)) as { animated: boolean }).animated).toBe(false);
  }
});

// 可手动推进的假时钟
function clock() {
  // 当前时间与定时器自增 id
  let time = 0, id = 0;
  // 待执行的定时器
  const pending = new Map<number, { at: number; callback: () => void }>();
  // 实现时钟接口
  const api: PlaybackClock = {
    now: () => time,
    // 登记定时器
    schedule: (callback, delay) => { pending.set(++id, { at: time + delay, callback }); return id; },
    // 取消定时器
    cancel: (timer) => { pending.delete(timer as number); },
  };
  // 推进时间并按到期顺序执行回调
  return { api, pending, advance(ms: number, blocked = false) {
    // 目标时间
    const end = time + ms;
    // blocked 模拟界面线程被阻塞：时间先跳到终点
    if (blocked) time = end;
    // 反复取出最早到期的定时器
    for (;;) {
      const next = [...pending].filter(([, value]) => value.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!next) break;
      pending.delete(next[0]);
      time = Math.max(time, next[1].at); next[1].callback();
    }
    time = end;
  } };
}

// 两帧：红 50ms，蓝 200ms
const frames = [{ source: "red", durationMs: 50 }, { source: "blue", durationMs: 200 }];

// 暂停保留当前帧与剩余帧时间
test("GIF pause preserves its current frame and the exact remaining frame time", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames, plays: null }, time.api, (value) => shown.push(value));
  // 开始播放
  image.setPlaying(true);
  // 播放 30ms 后暂停
  time.advance(30); image.setPlaying(false);
  // 仍停在第一帧
  expect(shown).toEqual(["red"]);
  // 暂停后没有定时器
  expect(time.pending.size).toBe(0);
  // 长时间暂停不累计时间
  time.advance(3_600_000);
  // 恢复后还差 20ms 才切帧
  image.setPlaying(true); time.advance(19);
  expect(shown).toEqual(["red"]);
  // 再过 1ms 切到蓝帧
  time.advance(1);
  expect(shown).toEqual(["red", "blue"]);
  time.advance(80); image.setPlaying(false);
  time.advance(10_000); image.setPlaying(true); time.advance(119);
  expect(shown).toEqual(["red", "blue"]);
  time.advance(1);
  expect(shown).toEqual(["red", "blue", "red"]);
  // 销毁后不留定时器
  image.dispose();
  expect(time.pending.size).toBe(0);
});

// 有限次数的动图停在末帧且不再调度
test("finite GIF finishes on its final frame and never schedules another wakeup", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames, plays: 2 }, time.api, (value) => shown.push(value));
  // 播放超过两轮
  image.setPlaying(true); time.advance(500);
  expect(shown).toEqual(["red", "blue", "red", "blue"]);
  expect(time.pending.size).toBe(0);
  // 暂停再恢复也不会重新调度
  image.setPlaying(false); time.advance(10_000); image.setPlaying(true);
  expect(time.pending.size).toBe(0);
  // 最终停在蓝帧
  expect(shown[shown.length - 1]).toBe("blue");
});

// 界面线程卡顿后追赶，但不延长有限动画
test("a delayed UI thread catches up without extending a finite animation", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames, plays: 2 }, time.api, (value) => shown.push(value));
  // 阻塞 1 秒后一次性追赶
  image.setPlaying(true); time.advance(1000, true);
  expect(shown).toEqual(["red", "blue"]);
  expect(time.pending.size).toBe(0);
});

// 两个消费者共享帧数据但播放进度独立
test("two consumers share frame data while preserving independent playheads", () => {
  const time = clock(), first: string[] = [], second: string[] = [];
  const a = new ImagePlayback({ frames, plays: null }, time.api, (value) => first.push(value));
  const b = new ImagePlayback({ frames, plays: null }, time.api, (value) => second.push(value));
  // 只有 A 在播放
  a.setPlaying(true); time.advance(50);
  expect(first).toEqual(["red", "blue"]);
  expect(second).toEqual(["red"]);
  b.setPlaying(true); time.advance(20); a.setPlaying(false); time.advance(30);
  expect(second).toEqual(["red", "blue"]);
  time.advance(1000);
  expect(first).toEqual(["red", "blue"]);
  a.dispose(); b.dispose();
  // 定时器全部清空
  expect(time.pending.size).toBe(0);
});

// 单帧与已销毁的图片不创建定时器
test("static and disposed images do not create an animation timer", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames: [frames[0]], plays: null }, time.api, (value) => shown.push(value));
  image.setPlaying(true); time.advance(1000); image.dispose(); image.setPlaying(true);
  expect(shown).toEqual(["red"]);
  expect(time.pending.size).toBe(0);
});
