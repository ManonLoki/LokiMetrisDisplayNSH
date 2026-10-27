import { expect, test } from "nsh/test";
import { open, PHONE, DESKTOP, T0, RECEIVED, KEY_PANEL, KEY_DEVICE, KEY_PAIRED, KEY_CACHE, KEY_INJECT, KEY_FRAMES, safe } from "./app-environment";
import { representativeSnapshot } from "./app-fixtures";
import hookPNG from "../fixtures/assets/hook.png";
import hookGIF from "../fixtures/assets/hook.gif";
import { PAIRING_WINDOW_MS, formatSyncTime, makePairingCode } from "./support/fixture-rules";

test("narrow and wide navigation and the empty stage", async () => {
  const app = await open("./TestApp.tsx", { width: 390, height: 844, safeArea: safe });
  await app.waitFor("app-root");
  expect(app.shows("nav-bottom")).toBe(true);
  expect(app.shows("nav-side")).toBe(false);
  expect(app.texts()).toContain("展示台");
  expect(app.texts()).toContain("设置");
  expect(app.shows("panel-split-column")).toBe(true);
  await app.waitUntil(() => {
    const raw = app.storage()[KEY_FRAMES];
    if (!raw) return false;
    const frames = JSON.parse(raw) as { usage?: { height: number }; hook?: { height: number } };
    return (frames.usage?.height ?? 0) > 0 && (frames.hook?.height ?? 0) > 0;
  });
  const frames = JSON.parse(app.storage()[KEY_FRAMES]) as {
    usage: { width: number; height: number };
    hook: { width: number; height: number };
  };
  expect(frames.usage.height).toBeGreaterThan(200);
  expect(frames.hook.height).toBeGreaterThan(200);
  expect(Math.abs(frames.usage.height - frames.hook.height)).toBeLessThan(frames.usage.height * 0.25);
  await app.setViewport(600, 300);
  expect(app.shows("panel-split-row")).toBe(true);
  expect(app.shows("nav-bottom")).toBe(true);
  expect(app.shows("nav-side")).toBe(false);
  await app.setViewport(800, 900);
  expect(app.shows("nav-side")).toBe(true);
  expect(app.shows("nav-bottom")).toBe(false);
  expect(app.shows("panel-split-column")).toBe(true);
  expect(app.texts()).toContain("展示台");
  expect(app.texts()).toContain("设置");

  await app.setViewport(390, 844);
  await app.press("设置");
  await app.press("settings-show-usage");
  await app.press("settings-show-hooks");
  await app.waitUntil(() => app.storage()[KEY_PANEL]?.includes('"showUsage":false') === true);
  expect(app.texts()).toContain("开启一项内容，即可在展示台中显示。");
  await app.press("展示台");
  expect(app.texts()).toContain("前往设置");
  expect(app.texts()).toContain("暂无卡片");
  await app.press("前往设置");
  expect(app.texts()).toContain("面板显示与设备连接");
}, 60_000);

test("panel choices persist across a new open", async () => {
  const app = await open("./TestApp.tsx", {
    width: 390,
    height: 844,
    safeArea: safe,
    storage: { [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }) },
  });
  await app.waitFor("app-root");
  await app.press("设置");
  await app.waitUntil(() => app.shows("settings-usage-detailed"));
  await app.wait(100); // Allow the native host to lay out the settings hit targets before coordinate clicks.
  await app.press("settings-usage-detailed");
  await app.waitUntil(() => (app.storage()[KEY_PANEL] ?? "").includes('"usageDetail":"detailed"'));
  await app.wait(100);
  await app.press("settings-hooks-2x2");
  await app.waitUntil(() => {
    const raw = app.storage()[KEY_PANEL] ?? "";
    return raw.includes('"usageDetail":"detailed"') && raw.includes('"hookLayout":"2x2"');
  });
  const kept = {
    [KEY_PANEL]: app.storage()[KEY_PANEL],
    [KEY_DEVICE]: app.storage()[KEY_DEVICE],
  };
  const again = await open("./TestApp.tsx", { width: 390, height: 844, safeArea: safe, storage: kept });
  await again.waitFor("app-root");
  await again.press("menu-settings");
  await again.waitUntil(() => again.shows("settings-hooks-2x2"));
  expect(again.texts()).toContain("显示总量与各 Agent 明细");
  expect(again.texts()).not.toContain("突出总 Token 用量");
  expect(again.texts()).toContain("每页四张，左右翻页查看更多");
  expect(again.texts()).not.toContain("每页一张，完整展示图片");
  expect(again.storage()[KEY_PANEL]).toBe(kept[KEY_PANEL]);
});

test("snapshot numbers, hooks and connection phases", async () => {
  const snapshot = representativeSnapshot();
  const app = await open("./TestApp.tsx", {
    clock: { time: T0 },
    width: 390,
    height: 1800,
    safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }),
      [KEY_INJECT]: JSON.stringify({
        discovered: [{ id: DESKTOP, name: "书房桌面" }],
        events: [
          { atMs: T0 + 1000, kind: "handshake" },
          { atMs: T0 + 2000, kind: "snapshot", receivedAtMs: RECEIVED, snapshot },
        ],
      }),
    },
  });
  await app.waitUntil(() => app.shows("app-root") && app.texts().includes("等待同步"));
  expect(app.texts().join("\n")).not.toContain("已连接");
  expect(app.texts()).not.toContain("书房桌面");
  await app.press("设置");
  await app.waitUntil(() => app.texts().includes("书房桌面"));
  expect(app.texts()).toContain("自动发现桌面");
  await app.press(`host-connect-${DESKTOP}`);
  const code = app.texts().find((text) => /^\d{4}$/.test(text));
  expect(code).toBe(makePairingCode(T0, DESKTOP));
  expect(app.texts()).toContain("60 秒内有效 · 保持应用在前台");
  expect(app.texts().join("\n")).not.toContain("已连接");
  await app.clock.runFor(1000);
  expect(app.texts().join("\n")).not.toContain("已连接");
  expect(app.texts().includes(code ?? "")).toBe(false);
  expect(app.storage()[KEY_PAIRED]?.includes(DESKTOP)).toBe(true);
  await app.clock.runFor(1000);
  await app.waitUntil(() => app.texts().join("\n").includes("已连接"));
  expect(app.texts()).toContain("已连接桌面");
  const cached = JSON.parse(app.storage()[KEY_CACHE]);
  expect(cached.receivedAtMs).toBe(RECEIVED);
  expect(cached.snapshot.tokens.agents.map((agent: { agent: string }) => agent.agent)).toEqual(
    snapshot.tokens?.agents.map((agent) => agent.agent),
  );

  await app.press("详细");
  await app.press("展示台");
  await app.waitUntil(() => app.texts().includes("设备时区"));
  const texts = app.texts();
  expect(texts).toContain(snapshot.tokens!.day.date);
  expect(texts.join("\n")).not.toContain(formatSyncTime(T0 + 2000));
  expect(texts).toContain("1.5K");
  expect(texts).toContain("1,500");
  expect(texts).toContain("999");
  expect(texts).toContain("221.19M");
  expect(texts).toContain("221,193,571");
  expect(texts).toContain("0");
  expect(texts).not.toContain("SecretBot");
  expect(texts).not.toContain("1M");
  expect(texts).toContain("Asia/Shanghai");
  const order = ["Codex", "Claude Code", "Cursor", "Grok", "Other Agent", "WorkBuddy"].map((name) => texts.indexOf(name));
  expect(order.every((index) => index >= 0)).toBe(true);
  for (let index = 1; index < order.length; index += 1) expect(order[index]).toBeGreaterThan(order[index - 1]);
  expect(texts.filter((text) => text === "每月").length).toBe(1);
  expect(texts.filter((text) => text === "每周").length).toBe(2);
  expect(texts.filter((text) => text === "5h").length).toBe(2);
  const afterMonthly = texts.slice(texts.indexOf("每月"));
  expect(afterMonthly.indexOf("每周")).toBeGreaterThan(0);
  expect(afterMonthly.indexOf("每周")).toBeLessThan(afterMonthly.indexOf("5h"));
  expect(texts).toContain("10%");
  expect(texts).toContain("40%");
  expect(texts).toContain("90%");
  expect(texts).not.toContain("Token 用量 · 缓存");

  expect(texts).toContain("空闲");
  // The native pager keeps neighbouring pages mounted for swipe peeking.
  // Its accessibility tree includes those pages; current page is asserted below.
  expect(app.texts()).toContain("1 / 5 · 左右滑动");
  await app.drag("hooks-pager", { x: -260 });
  expect(app.texts()).toContain("运行");
  expect(app.texts()).toContain("2 / 5 · 左右滑动");
  await app.press("设置");
  await app.press("settings-hooks-2x2");
  await app.press("展示台");
  expect(app.texts()).toContain("1 / 2 · 左右滑动");
  expect(app.texts()).toContain("空闲");
  expect(app.texts()).toContain("运行");
  expect(app.texts()).toContain("询问");
  expect(app.texts()).toContain("出错");
  expect(app.texts()).not.toContain("桌面回传的正文");
  await app.drag("hooks-pager", { x: -260 });
  expect(app.texts()).toContain("暂无活动");
  expect(app.texts()).toContain("2 / 2 · 左右滑动");
  await app.press("hook-card-5");
  expect(app.texts()).toContain("正文已截断");
  expect(app.texts()).toContain("桌面回传的正文");
  expect(app.texts()).toContain("图片尚未缓存");
  await app.press("关闭");
  expect(app.shows("hook-detail")).toBe(false);

  await app.press("设置");
  app.onDialog(() => "忘记");
  await app.press(`host-forget-${DESKTOP}`);
  await app.waitUntil(() => app.storage()[KEY_CACHE] == null);
  expect(app.storage()[KEY_PAIRED]?.includes(DESKTOP) ?? false).toBe(false);
  expect(app.texts().join("\n")).not.toContain("已连接");
  await app.press("展示台");
  expect(app.texts()).toContain("等待同步");
  expect(app.texts()).not.toContain("1.5K");
}, 60_000);

test("pairing window expires and can be started again", async () => {
  const app = await open("./TestApp.tsx", {
    clock: { time: T0 },
    width: 390,
    height: 844,
    safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }),
      [KEY_INJECT]: JSON.stringify({ discovered: [{ id: DESKTOP, name: "书房桌面" }], events: [] }),
    },
  });
  await app.waitFor("app-root");
  await app.press("设置");
  await app.waitUntil(() => app.texts().includes("书房桌面"));
  await app.press(`host-connect-${DESKTOP}`);
  expect(app.texts()).toContain(makePairingCode(T0, DESKTOP));
  await app.clock.fastForward(PAIRING_WINDOW_MS);
  await app.waitUntil(() => app.texts().includes("配对窗口已过期，请重新选择桌面。"));
  expect(app.texts().join("\n")).not.toContain("已连接");
  expect(app.texts().includes(makePairingCode(T0, DESKTOP))).toBe(false);
  await app.press(`host-connect-${DESKTOP}`);
  expect(app.texts()).toContain(makePairingCode(T0 + PAIRING_WINDOW_MS, DESKTOP));
});

test("device name validates while typing and only saves a legal dirty draft", async () => {
  const app = await open("./TestApp.tsx", {
    width: 390,
    height: 844,
    safeArea: safe,
    storage: { [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }) },
  });
  await app.waitFor("app-root");
  await app.press("设置");
  await app.waitUntil(() => app.value("settings-device-name") === "客厅手机");
  await app.clear("settings-device-name");
  const tooLong = "a".repeat(161);
  await app.type("settings-device-name", tooLong);
  await app.waitUntil(() => app.value("settings-device-name") === tooLong);
  expect(app.enabled("settings-save-name")).toBe(false);
  expect(app.texts()).toContain("设备名称不能超过 160 个 UTF-8 字节");
  expect(JSON.parse(app.storage()[KEY_DEVICE]).name).toBe("客厅手机");
  await app.clear("settings-device-name");
  await app.type("settings-device-name", "书房展示");
  await app.press("保存名称");
  await app.waitUntil(() => JSON.parse(app.storage()[KEY_DEVICE]).name === "书房展示");
  expect(app.texts()).toContain("已保存，下次连接生效");
  const again = await open("./TestApp.tsx", {
    width: 390,
    height: 844,
    safeArea: safe,
    storage: { [KEY_DEVICE]: app.storage()[KEY_DEVICE] },
  });
  await again.press("设置");
  await again.waitUntil(() => again.value("settings-device-name") === "书房展示");
});

test("reading a cached snapshot does not rewrite its sync time or look online", async () => {
  const snapshot = representativeSnapshot();
  const app = await open("./TestApp.tsx", {
    clock: { time: 1_800_000_000_000 },
    width: 390,
    height: 1600,
    safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }),
      [KEY_CACHE]: JSON.stringify({ sourceId: DESKTOP, receivedAtMs: RECEIVED, snapshot }),
      [KEY_PANEL]: JSON.stringify({ showUsage: true, showHooks: false, usageDetail: "detailed", hookLayout: "1x1" }),
    },
  });
  await app.waitUntil(() => app.texts().includes("1.5K"));
  expect(app.texts()).toContain(snapshot.tokens!.day.date);
  expect(app.texts()).toContain("Token 用量");
  expect(app.texts().join("\n")).not.toContain("已连接");
  expect(app.texts().join("\n")).not.toContain(formatSyncTime(1_800_000_000_000));
  expect(JSON.parse(app.storage()[KEY_CACHE]).receivedAtMs).toBe(RECEIVED);
  expect(app.texts()).toContain("0");
  expect(app.texts()).not.toContain("SecretBot");
});

test("hook swipes change pages and cached PNG and GIF reach the image renderer", async () => {
  const snapshot = representativeSnapshot();
  const pngHash = "cd".repeat(32);
  const gifHash = "ef".repeat(32);
  snapshot.hooks[0].image = pngHash;
  snapshot.hooks[1].image = gifHash;
  snapshot.hooks[0].content = "图片详情正文";
  const app = await open("./TestApp.tsx", {
    clock: { time: T0 }, width: 390, height: 844, safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "手机" }),
      [KEY_PAIRED]: JSON.stringify({ [DESKTOP]: { clientId: DESKTOP, savedAtMs: T0 } }),
      [KEY_INJECT]: JSON.stringify({ discovered: [{ id: DESKTOP, name: "桌面" }], events: [
        { atMs: T0 + 1000, kind: "snapshot", snapshot, receivedAtMs: RECEIVED, images: { [pngHash]: hookPNG.url.replace(/^asset:\/*/, "file:///"), [gifHash]: hookGIF.url.replace(/^asset:\/*/, "file:///") } },
      ] }),
    },
  });
  await app.press("设置");
  await app.press(`host-connect-${DESKTOP}`);
  await app.clock.runFor(1000);
  await app.press("展示台");
  expect(app.shows("hook-image-1")).toBe(true);
  let rendered = true;
  try { await app.screenshot(); }
  catch (error) {
    if (String(error).includes("a screenshot needs a real platform")) rendered = false;
    else throw error;
  }
  if (rendered) {
    await app.waitUntil(() => (JSON.parse(app.storage()["loki.imageLoads"] ?? "{}"))[pngHash]?.width === 48);
  }
  await app.drag("hooks-pager", { x: -260 });
  expect(app.texts()).toContain("2 / 5 · 左右滑动");
  expect(app.shows("hook-image-2")).toBe(true);
  if (rendered) {
    await app.waitUntil(() => (JSON.parse(app.storage()["loki.imageLoads"] ?? "{}"))[gifHash]?.width === 1);
    const decoded = JSON.parse(app.storage()["loki.imageLoads"]);
    expect(decoded[pngHash]).toEqual({ width: 48, height: 48 });
    expect(decoded[gifHash]).toEqual({ width: 1, height: 1 });
  }
  await app.drag("hooks-pager", { x: 260 });
  expect(app.texts()).toContain("1 / 5 · 左右滑动");
  await app.press("hook-card-1");
  expect(app.shows("hook-detail-image")).toBe(true);
  expect(app.texts()).toContain("图片详情正文");
  await app.press("hook-detail-close");
  await app.waitUntil(() => !app.shows("hook-detail"));
  await app.press("menu-settings");
  await app.waitFor("settings-usage-detailed");
  await app.press("settings-usage-detailed");
  await app.press("展示台");
  expect(app.shows("usage-scroll")).toBe(true);
  await app.scroll("usage-scroll", 500);
  expect(app.texts()).toContain("WorkBuddy");
  const frames = JSON.parse(app.storage()[KEY_FRAMES]);
  expect(Math.abs(frames.usage.height - frames.hook.height)).toBeLessThan(80);
}, 60_000);

test("malformed persisted data cannot crash the stage or become a live connection", async () => {
  const app = await open("./TestApp.tsx", {
    storage: {
      [KEY_CACHE]: JSON.stringify({ sourceId: DESKTOP, receivedAtMs: RECEIVED, snapshot: { version: 1 } }),
      [KEY_PAIRED]: "[]",
      [KEY_DEVICE]: "null",
      [KEY_PANEL]: "null",
    },
  });
  expect(app.texts()).toContain("等待同步");
  await app.press("设置");
  expect(app.texts()).toContain("还未发现桌面");
  expect(app.texts()).not.toContain("已连接桌面");
});


test("incoming snapshots preserve an in-progress device name and removed hook details stay closed", async () => {
  const first = representativeSnapshot();
  const removed = { ...first, hooks: first.hooks.filter((hook) => hook.slot !== 1) };
  const app = await open("./TestApp.tsx", {
    clock: { time: T0 }, width: 390, height: 844, safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "旧名称" }),
      [KEY_PAIRED]: JSON.stringify({ [DESKTOP]: { clientId: DESKTOP, savedAtMs: T0 } }),
      [KEY_INJECT]: JSON.stringify({ discovered: [{ id: DESKTOP, name: "桌面" }], events: [
        { atMs: T0 + 1000, kind: "snapshot", snapshot: first, receivedAtMs: RECEIVED },
        { atMs: T0 + 2000, kind: "snapshot", snapshot: removed, receivedAtMs: RECEIVED + 1000 },
        { atMs: T0 + 3000, kind: "snapshot", snapshot: first, receivedAtMs: RECEIVED + 2000 },
      ] }),
    },
  });
  await app.press("menu-settings");
  await app.press(`host-connect-${DESKTOP}`);
  await app.clear("settings-device-name");
  await app.type("settings-device-name", "工作台");
  await app.clock.runFor(1000);
  expect(app.value("settings-device-name")).toBe("工作台");
  await app.type("settings-device-name", "终端");
  await app.press("settings-save-name");
  await app.waitUntil(() => JSON.parse(app.storage()[KEY_DEVICE]).name === "工作台终端");
  await app.press("menu-panel");
  await app.press("hook-card-1");
  expect(app.shows("hook-detail")).toBe(true);
  await app.clock.runFor(1000);
  expect(app.shows("hook-detail")).toBe(false);
  await app.clock.runFor(1000);
  expect(app.shows("hook-detail")).toBe(false);
}, 60_000);
