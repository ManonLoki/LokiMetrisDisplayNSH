import { expect, test } from "nsh/test";
import { open, PHONE, safe } from "./app-environment";
import { representativeSnapshot } from "./app-fixtures";

// 构造夹具：未选择、未连接的空视图（Rust 夹具连接原样返回它）
function fixture(formFactor = "phone") {
  return { formFactor, projection: {
    deviceId: PHONE, deviceName: "客厅手机", hosts: [], selectedId: null, status: "正在发现桌面端…", error: null,
    pairingCode: null, pairingDeadlineMs: null, connected: false, snapshot: null, receivedAtMs: null, live: false,
    cacheSourceId: null,
  } as Record<string, unknown> };
}

// 移动端身份被写入 NSH 存储，面板选择随后可保存
test("mobile identity is adopted into NSH storage and panel choices are saved", async () => {
  const app = await open("./mobile-app.tsx", { width: 390, height: 2600, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(fixture()) } });
  await app.waitUntil(() => app.storage()["loki.device"] != null);
  // 设备 ID 应为手机 ID
  expect(JSON.parse(app.storage()["loki.device"]).id).toBe(PHONE);
  await app.press("menu-settings");
  await app.press("settings-hooks-2x2");
  await app.waitUntil(() => JSON.parse(app.storage()["loki.panel"] ?? "{}").hookLayout === "2x2");
});

// 移动端 Hook 卡片没有点击动作
test("mobile Hook cards have no press action", async () => {
  const value = fixture();
  // 填入快照并标记已连接
  value.projection.snapshot = representativeSnapshot();
  value.projection.receivedAtMs = 1_720_111_000_000;
  value.projection.connected = true;
  const app = await open("./mobile-app.tsx", { width: 390, height: 844, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(value) } });
  await app.waitFor("hook-card-1");
  await expect(app.press("hook-card-1")).rejects.toMatchObject({ code: "ERR_NSH_TEST_NOT_FOUND" });
});

// 电视形态保留遥控导航
test("televisions keep remote navigation in the shared NSH application", async () => {
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(fixture("tv")) } });
  await app.waitFor("nav-side");
  // 第一次按下：焦点落在“展示台”
  await app.keyboard.press("down");
  await app.waitFor("remote-focus-menu-panel");
  // 第二次按下：焦点移到“设置”
  await app.keyboard.press("down");
  await app.waitFor("remote-focus-menu-settings");
  await app.keyboard.press("enter");
  await app.waitFor("settings-panel-card");
});

test("television focus reveals a Hook on the next Carousel page", async () => {
  const value = fixture("tv");
  const snapshot = representativeSnapshot();
  snapshot.hooks = snapshot.hooks.slice(0, 2);
  value.projection.snapshot = snapshot;
  value.projection.receivedAtMs = 1_720_111_000_000;
  value.projection.connected = true;
  const panel = { showUsage: false, showHooks: true, usageDetail: "detailed", hookLayout: "1x1" };
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    reducedMotion: true, storage: { "test.mobileApp": JSON.stringify(value), "loki.panel": JSON.stringify(panel) } });

  await app.waitFor("hook-card-1");
  await app.keyboard.press("down");
  await app.waitUntil(() => app.shows("remote-focus-menu-panel") || app.shows("remote-focus-hook-card-1"));
  if (app.shows("remote-focus-menu-panel")) await app.keyboard.press("right");
  await app.waitFor("remote-focus-hook-card-1");
  await app.keyboard.press("right");
  await app.waitFor("remote-focus-hook-card-2");
  await app.waitUntil(() => app.texts().includes("2 / 2 · 方向键翻页"));
  await app.keyboard.press("enter");
  expect(app.shows("remote-focus-hook-card-2")).toBe(true);
  expect(app.texts()).toContain("2 / 2 · 方向键翻页");
});

// 手机与平板忽略遥控按键
test("phones and tablets ignore remote navigation keys", async () => {
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(fixture("tablet")) } });
  await app.waitFor("nav-side");
  await app.keyboard.press("down");
  expect(app.shows("remote-focus-menu-panel")).toBe(false);
});

// 系统栏通过安全区缩小内容区域
test("mobile system bars reduce the content area through NSH safe-area facts", async () => {
  const value = JSON.stringify(fixture());
  const plain = await open("./mobile-app.tsx", { width: 390, height: 844, safeArea: safe,
    storage: { "test.mobileApp": value } });
  // 等待用量卡片布局被记录
  await plain.waitUntil(() => plain.storage()["loki.frames"]?.includes('"usage"') === true);
  const fullHeight = JSON.parse(plain.storage()["loki.frames"]).usage.height;
  plain.dispose();
  // 顶部 48、底部 34 的安全区
  const inset = await open("./mobile-app.tsx", { width: 390, height: 844,
    safeArea: { top: 48, right: 0, bottom: 34, left: 0 }, storage: { "test.mobileApp": value } });
  await inset.waitUntil(() => inset.storage()["loki.frames"]?.includes('"usage"') === true);
  const insetHeight = JSON.parse(inset.storage()["loki.frames"]).usage.height;
  // 高度差应在 35 到 45 之间
  expect(fullHeight - insetHeight).toBeGreaterThan(35);
  expect(fullHeight - insetHeight).toBeLessThan(45);
  await inset.press("menu-settings");
  await inset.waitFor("settings-panel-card");
});
