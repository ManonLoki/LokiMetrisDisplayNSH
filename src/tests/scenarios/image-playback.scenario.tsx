// 引入场景测试 API
import { expect, open, test, type App } from "nsh/test";

// 截图输出目录占位符
const OUTPUT = "__OUTPUT_DIRECTORY__";
// 应用入口占位符
const ENTRY = "__APP_ENTRY__";
// 无限循环图片的占位符
const SOURCE = "__INFINITE_SOURCE__";
// 有限循环图片的占位符
const FINITE = "__FINITE_SOURCE__";
// 视口尺寸
const viewport = { width: 488, height: 368 };

// 读取页面写入的状态
function state(app: App) {
  return JSON.parse(app.storage()["test.imagePlaybackState"] ?? "{}");
}

// 启动并等待两张图片加载完成
async function launch(source: string, reducedMotion = false) {
  const app = await open(ENTRY, { ...viewport, reducedMotion, network: {},
    storage: { "test.imagePlayback": JSON.stringify({ source }) } });
  await app.waitFor("image-playback-a", { timeout: 15_000 });
  await app.waitFor("image-playback-b", { timeout: 15_000 });
  // 出错或两张都加载完成
  await app.waitUntil(() => !!state(app).error || (state(app).loadedA && state(app).loadedB), { timeout: 15_000 });
  // 不应有错误
  expect(state(app).error).toBe("");
  // 减少动效设置应与预期一致
  expect(state(app).reducedMotion).toBe(reducedMotion);
  await app.wait(200);
  return app;
}

// 截图并输出带矩形与门控信息的日志行
async function capture(app: App, group: string, index: number, readyAt: number) {
  const file = `${group}-${index}.png`;
  await app.screenshot(`${OUTPUT}/${file}`);
  console.log(`IMAGE_PLAYBACK_CAPTURE=${JSON.stringify({ file, group, index, viewport,
    rects: JSON.parse(app.storage()["test.imagePlaybackRects"]), gates: state(app),
    timeMs: Date.now(), elapsedSinceReadyMs: Date.now() - readyAt })}`);
}

// 以不规则间隔连续截 8 帧，避开与帧周期同步
async function moving(app: App, group: string, readyAt: number) {
  for (let index = 0; index < 8; index++) {
    if (index) await app.wait(310 + (index % 3) * 37);
    await capture(app, group, index, readyAt);
  }
}

// 静止场景：在不均匀间隔截图，覆盖完整两帧周期
async function still(app: App, group: string, readyAt: number) {
  await app.wait(250); // 等最后一次原生图片加载稳定,再比较其像素。
  // 不均匀的间隔涵盖完整的两帧周期,并避免同色混叠。
  for (const [index, delay] of [0, 370, 510, 690, 830].entries()) {
    if (delay) await app.wait(delay);
    await capture(app, group, index, readyAt);
  }
}

// 真实 GIF 像素前进、暂停与恢复，同一地址的两个控件互不影响
test("real GIF pixels advance, pause and resume independently for a shared URI", async () => {
  // 启动无限循环的 GIF
  const app = await launch(SOURCE);
  // 记录就绪时刻
  const readyAt = Date.now();
  try {
    // 初始 A 活跃，应在动
    await moving(app, "moving", readyAt);
    // 关闭 A，应静止
    await app.press("image-toggle-a");
    await app.waitUntil(() => state(app).activeA === false);
    // 静止
    await still(app, "inactive", readyAt);
    await app.press("image-toggle-a");
    await app.waitUntil(() => state(app).activeA === true);
    // 重新打开 A，应恢复
    await moving(app, "resumed", readyAt);
    await app.press("image-toggle-a");
    await app.waitUntil(() => state(app).activeA === false);
    // 关闭 A、打开 B，验证两者独立
    await app.press("image-toggle-b");
    await app.waitUntil(() => state(app).activeB === true);
    // B 在动
    await moving(app, "independent", readyAt);
    // 切到后台，应静止
    await app.press("image-toggle-foreground");
    await app.waitUntil(() => state(app).foreground === false);
    // 静止
    await still(app, "background", readyAt);
    await app.press("image-toggle-foreground");
    await app.waitUntil(() => state(app).foreground === true);
    // 回到前台，应恢复
    await moving(app, "foreground-resumed", readyAt);
  // 释放应用
  } finally { app.dispose(); }
}, 150_000);

// 开启“减少动态效果”时 GIF 保持静止
test("reduced motion freezes real GIF pixels", async () => {
  const app = await launch(SOURCE, true);
  try { await still(app, "reduced", Date.now()); }
  finally { app.dispose(); }
}, 45_000);

// 有限循环的 GIF 播完后停在最后一帧
test("finite GIF ends on its final blue frame while still allowed to play", async () => {
  const app = await launch(FINITE);
  const readyAt = Date.now();
  try {
    // 播放期间
    await moving(app, "finite-running", readyAt);
    // 等待播完（两帧各 1 秒外加一次重复）
    await app.wait(4500); // 来源有两个 1 秒的帧,且恰好重复一次。
    // 播完后应静止
    await still(app, "finite-ended", readyAt);
    // 仍处于允许播放的状态
    expect(state(app).activeA).toBe(true);
    expect(state(app).foreground).toBe(true);
    expect(state(app).reducedMotion).toBe(false);
  } finally { app.dispose(); }
}, 60_000);
