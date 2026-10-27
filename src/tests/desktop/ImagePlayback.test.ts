import { expect, test } from "nsh/test";
import { ImagePlayback, type PlaybackClock } from "../../shared/ui/image-playback";
import { prepareImage } from "../../shared/ui/image-frames";

test("static PNG and JPEG data URIs stay with the standard Image decoder", async () => {
  for (const source of ["data:image/png;base64,iVBORw0KGgo=", "data:image/jpeg;base64,/9j/4AAQSkZJRg=="]) {
    expect((await prepareImage(source)).animated).toBe(false);
  }
});

function clock() {
  let time = 0, id = 0;
  const pending = new Map<number, { at: number; callback: () => void }>();
  const api: PlaybackClock = {
    now: () => time,
    schedule: (callback, delay) => { pending.set(++id, { at: time + delay, callback }); return id; },
    cancel: (timer) => { pending.delete(timer as number); },
  };
  return { api, pending, advance(ms: number, blocked = false) {
    const end = time + ms;
    if (blocked) time = end;
    for (;;) {
      const next = [...pending].filter(([, value]) => value.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!next) break;
      pending.delete(next[0]);
      time = Math.max(time, next[1].at); next[1].callback();
    }
    time = end;
  } };
}

const frames = [{ source: "red", durationMs: 50 }, { source: "blue", durationMs: 200 }];

test("GIF pause preserves its current frame and the exact remaining frame time", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames, plays: null }, time.api, (value) => shown.push(value));
  image.setPlaying(true);
  time.advance(30); image.setPlaying(false);
  expect(shown).toEqual(["red"]);
  expect(time.pending.size).toBe(0);
  time.advance(3_600_000);
  image.setPlaying(true); time.advance(19);
  expect(shown).toEqual(["red"]);
  time.advance(1);
  expect(shown).toEqual(["red", "blue"]);
  time.advance(80); image.setPlaying(false);
  time.advance(10_000); image.setPlaying(true); time.advance(119);
  expect(shown).toEqual(["red", "blue"]);
  time.advance(1);
  expect(shown).toEqual(["red", "blue", "red"]);
  image.dispose();
  expect(time.pending.size).toBe(0);
});

test("finite GIF finishes on its final frame and never schedules another wakeup", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames, plays: 2 }, time.api, (value) => shown.push(value));
  image.setPlaying(true); time.advance(500);
  expect(shown).toEqual(["red", "blue", "red", "blue"]);
  expect(time.pending.size).toBe(0);
  image.setPlaying(false); time.advance(10_000); image.setPlaying(true);
  expect(time.pending.size).toBe(0);
  expect(shown[shown.length - 1]).toBe("blue");
});

test("a delayed UI thread catches up without extending a finite animation", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames, plays: 2 }, time.api, (value) => shown.push(value));
  image.setPlaying(true); time.advance(1000, true);
  expect(shown).toEqual(["red", "blue"]);
  expect(time.pending.size).toBe(0);
});

test("two consumers share frame data while preserving independent playheads", () => {
  const time = clock(), first: string[] = [], second: string[] = [];
  const a = new ImagePlayback({ frames, plays: null }, time.api, (value) => first.push(value));
  const b = new ImagePlayback({ frames, plays: null }, time.api, (value) => second.push(value));
  a.setPlaying(true); time.advance(50);
  expect(first).toEqual(["red", "blue"]);
  expect(second).toEqual(["red"]);
  b.setPlaying(true); time.advance(20); a.setPlaying(false); time.advance(30);
  expect(second).toEqual(["red", "blue"]);
  time.advance(1000);
  expect(first).toEqual(["red", "blue"]);
  a.dispose(); b.dispose();
  expect(time.pending.size).toBe(0);
});

test("static and disposed images do not create an animation timer", () => {
  const time = clock(), shown: string[] = [];
  const image = new ImagePlayback({ frames: [frames[0]], plays: null }, time.api, (value) => shown.push(value));
  image.setPlaying(true); time.advance(1000); image.dispose(); image.setPlaying(true);
  expect(shown).toEqual(["red"]);
  expect(time.pending.size).toBe(0);
});
