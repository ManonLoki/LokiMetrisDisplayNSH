import { expect, open, test, type App } from "nsh/test";

const OUTPUT = "__OUTPUT_DIRECTORY__";
const ENTRY = "__APP_ENTRY__";
const SOURCE = "__INFINITE_SOURCE__";
const FINITE = "__FINITE_SOURCE__";
const viewport = { width: 488, height: 368 };

function state(app: App) {
  return JSON.parse(app.storage()["test.imagePlaybackState"] ?? "{}");
}

async function launch(source: string, reducedMotion = false) {
  const app = await open(ENTRY, { ...viewport, reducedMotion, network: {},
    storage: { "test.imagePlayback": JSON.stringify({ source }) } });
  await app.waitFor("image-playback-a", { timeout: 15_000 });
  await app.waitFor("image-playback-b", { timeout: 15_000 });
  await app.waitUntil(() => !!state(app).error || (state(app).loadedA && state(app).loadedB), { timeout: 15_000 });
  expect(state(app).error).toBe("");
  expect(state(app).reducedMotion).toBe(reducedMotion);
  await app.wait(200);
  return app;
}

async function capture(app: App, group: string, index: number, readyAt: number) {
  const file = `${group}-${index}.png`;
  await app.screenshot(`${OUTPUT}/${file}`);
  console.log(`IMAGE_PLAYBACK_CAPTURE=${JSON.stringify({ file, group, index, viewport,
    rects: JSON.parse(app.storage()["test.imagePlaybackRects"]), gates: state(app),
    timeMs: Date.now(), elapsedSinceReadyMs: Date.now() - readyAt })}`);
}

async function moving(app: App, group: string, readyAt: number) {
  for (let index = 0; index < 8; index++) {
    if (index) await app.wait(310 + (index % 3) * 37);
    await capture(app, group, index, readyAt);
  }
}

async function still(app: App, group: string, readyAt: number) {
  await app.wait(250); // Settle the last native image load before comparing its pixels.
  // Uneven intervals cover a complete two-frame cycle and avoid a same-color alias.
  for (const [index, delay] of [0, 370, 510, 690, 830].entries()) {
    if (delay) await app.wait(delay);
    await capture(app, group, index, readyAt);
  }
}

test("real GIF pixels advance, pause and resume independently for a shared URI", async () => {
  const app = await launch(SOURCE);
  const readyAt = Date.now();
  try {
    await moving(app, "moving", readyAt);
    await app.press("image-toggle-a");
    await app.waitUntil(() => state(app).activeA === false);
    await still(app, "inactive", readyAt);
    await app.press("image-toggle-a");
    await app.waitUntil(() => state(app).activeA === true);
    await moving(app, "resumed", readyAt);
    await app.press("image-toggle-a");
    await app.waitUntil(() => state(app).activeA === false);
    await app.press("image-toggle-b");
    await app.waitUntil(() => state(app).activeB === true);
    await moving(app, "independent", readyAt);
    await app.press("image-toggle-foreground");
    await app.waitUntil(() => state(app).foreground === false);
    await still(app, "background", readyAt);
    await app.press("image-toggle-foreground");
    await app.waitUntil(() => state(app).foreground === true);
    await moving(app, "foreground-resumed", readyAt);
  } finally { app.dispose(); }
}, 150_000);

test("reduced motion freezes real GIF pixels", async () => {
  const app = await launch(SOURCE, true);
  try { await still(app, "reduced", Date.now()); }
  finally { app.dispose(); }
}, 45_000);

test("finite GIF ends on its final blue frame while still allowed to play", async () => {
  const app = await launch(FINITE);
  const readyAt = Date.now();
  try {
    await moving(app, "finite-running", readyAt);
    await app.wait(4500); // The source has two 1s frames and exactly one repeat.
    await still(app, "finite-ended", readyAt);
    expect(state(app).activeA).toBe(true);
    expect(state(app).foreground).toBe(true);
    expect(state(app).reducedMotion).toBe(false);
  } finally { app.dispose(); }
}, 60_000);
