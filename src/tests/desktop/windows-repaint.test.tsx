import { expect, open, test, type App } from "nsh/test";

interface Snapshot {
  sequence: number;
  value: number;
  foreground: boolean;
  attached: boolean;
  reducedMotion: boolean;
  platform: string;
}

// 每次等待新探针回执，防止把前一次异步存储结果当成当前值。
async function sample(app: App): Promise<Snapshot> {
  const previous = JSON.parse(app.storage()["test.windowsRepaint"] ?? "{}").sequence ?? 0;
  await app.press("repaint-probe");
  await app.waitUntil(() => (JSON.parse(app.storage()["test.windowsRepaint"] ?? "{}").sequence ?? 0) > previous);
  return JSON.parse(app.storage()["test.windowsRepaint"]) as Snapshot;
}

test("Windows repaint pauses in background and resumes with reduced motion enabled", async () => {
  const app = await open("./windows-repaint-app.tsx", {
    platform: "windows", clock: { time: 0 }, reducedMotion: true, network: {},
  });
  try {
    await app.waitFor("repaint-surface");
    const initial = await sample(app);
    expect(initial.platform).toBe("windows");
    expect(initial.reducedMotion).toBe(true);
    expect(initial.foreground).toBe(false);
    expect(initial.value).toBe(0);
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBe(0);

    await app.press("repaint-toggle-foreground");
    await app.clock.runFor(250);
    const moving = await sample(app);
    expect(moving.foreground).toBe(true);
    expect(moving.value).toBeGreaterThan(0);

    await app.press("repaint-toggle-foreground");
    expect((await sample(app)).value).toBe(0);
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBe(0);
    await app.press("repaint-toggle-foreground");
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBeGreaterThan(0);
  } finally { app.dispose(); }
});

test("removing a foreground repaint surface stops its animation before a new mount", async () => {
  const app = await open("./windows-repaint-app.tsx", {
    platform: "windows", clock: { time: 0 }, network: {},
  });
  try {
    await app.waitFor("repaint-surface");
    await app.press("repaint-toggle-foreground");
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBeGreaterThan(0);
    await app.press("repaint-toggle-attachment");
    expect(app.shows("repaint-surface")).toBe(false);
    const removed = await sample(app);
    expect(removed.attached).toBe(false);
    expect(removed.foreground).toBe(true);
    expect(removed.value).toBe(0);
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBe(0);

    await app.press("repaint-toggle-attachment");
    await app.waitFor("repaint-surface");
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBeGreaterThan(0);
  } finally { app.dispose(); }
});

test("other platforms keep a static backdrop through visibility changes", async () => {
  const app = await open("./windows-repaint-app.tsx", {
    platform: "macos", clock: { time: 0 }, network: {},
  });
  try {
    await app.waitFor("repaint-surface");
    expect((await sample(app)).platform).toBe("macos");
    expect((await sample(app)).value).toBe(0);
    await app.press("repaint-toggle-foreground");
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBe(0);
    await app.press("repaint-toggle-attachment");
    await app.clock.runFor(250);
    expect((await sample(app)).value).toBe(0);
  } finally { app.dispose(); }
});
