// 引入测试 API
import { expect, test } from "nsh/test";
// 引入打开函数、常量与存储键
import { open, PHONE, DESKTOP, T0, RECEIVED, KEY_PANEL, KEY_DEVICE, KEY_PAIRED, KEY_CACHE, KEY_INJECT, safe } from "./app-environment";
// 引入夹具快照
import { representativeSnapshot } from "./app-fixtures";
// 引入配对规则
import { formatSyncTime } from "./support/fixture-rules";

// 读取缓存快照不会改写同步时间，也不会显示为在线
test("reading a cached snapshot does not rewrite its sync time or look online", async () => {
  const snapshot = representativeSnapshot();
  const app = await open("./test-app.tsx", {
    clock: { time: 1_800_000_000_000 },
    width: 390,
    height: 1600,
    safeArea: safe,
    storage: {
      // 预置设备身份
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }),
      // 预置缓存的快照
      [KEY_CACHE]: JSON.stringify({ sourceId: DESKTOP, receivedAtMs: RECEIVED, snapshot }),
      // 预置面板设置
      [KEY_PANEL]: JSON.stringify({ showUsage: true, showHooks: false, usageDetail: "detailed", hookLayout: "1x1" }),
    },
  });
  // 缓存的数字应显示
  await app.waitUntil(() => app.texts().includes("1.5K"));
  expect(app.texts()).toContain(snapshot.tokens!.day.date);
  expect(app.texts()).toContain("Token 用量");
  // 不应显示为已连接
  expect(app.texts().join("\n")).not.toContain("已连接");
  // 不应显示当前时间作为同步时间
  expect(app.texts().join("\n")).not.toContain(formatSyncTime(1_800_000_000_000));
  // 缓存里的接收时间保持不变
  expect(JSON.parse(app.storage()[KEY_CACHE]).receivedAtMs).toBe(RECEIVED);
  expect(app.texts()).toContain("0");
  expect(app.texts()).not.toContain("SecretBot");
});

// 损坏的持久化数据不会让界面崩溃，也不会变成在线连接
test("malformed persisted data cannot crash the stage or become a live connection", async () => {
  // 注入各种非法数据
  const app = await open("./test-app.tsx", {
    storage: {
      [KEY_CACHE]: JSON.stringify({ sourceId: DESKTOP, receivedAtMs: RECEIVED, snapshot: { version: 1 } }),
      [KEY_PAIRED]: "[]",
      [KEY_DEVICE]: "null",
      [KEY_PANEL]: "null",
    },
  });
  // 展示台处于等待状态
  expect(app.texts()).toContain("等待同步");
  await app.press("设置");
  // 设置页没有任何桌面
  expect(app.texts()).toContain("还未发现桌面");
  // 非法缓存不会变成连接
  expect(app.texts()).not.toContain("已连接桌面");
});


// 新快照不会冲掉正在输入的设备名称，被移除的 Hook 详情保持关闭
test("incoming snapshots preserve an in-progress device name and removed hook details stay closed", async () => {
  const first = representativeSnapshot();
  // 去掉槽位 1 的快照
  const removed = { ...first, hooks: first.hooks.filter((hook) => hook.slot !== 1) };
  const app = await open("./test-app.tsx", {
    clock: { time: T0 }, width: 390, height: 844, safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "旧名称" }),
      [KEY_PAIRED]: JSON.stringify({ [DESKTOP]: { clientId: DESKTOP, savedAtMs: T0 } }),
      [KEY_INJECT]: JSON.stringify({ discovered: [{ id: DESKTOP, name: "桌面" }], events: [
        { atMs: T0 + 1000, kind: "snapshot", snapshot: first, receivedAtMs: RECEIVED },
        // 第二秒：快照中去掉了一项
        { atMs: T0 + 2000, kind: "snapshot", snapshot: removed, receivedAtMs: RECEIVED + 1000 },
        // 第三秒：快照恢复
        { atMs: T0 + 3000, kind: "snapshot", snapshot: first, receivedAtMs: RECEIVED + 2000 },
      // 事件序列结束
      ] }),
    },
  });
  // 点击设置菜单
  await app.press("menu-settings");
  // 连接桌面，三个快照依次到达：完整、去掉槽位 1、再次完整
  await app.press(`host-connect-${DESKTOP}`);
  await app.clear("settings-device-name");
  // 输入新名称的一部分
  await app.type("settings-device-name", "工作台");
  await app.clock.runFor(1000);
  // 快照到达后输入内容未被覆盖
  expect(app.value("settings-device-name")).toBe("工作台");
  // 继续输入
  await app.type("settings-device-name", "终端");
  await app.press("settings-save-name");
  // 保存的是完整的草稿
  await app.waitUntil(() => JSON.parse(app.storage()[KEY_DEVICE]).name === "工作台终端");
  await app.press("menu-panel");
  // 打开槽位 1 的详情
  await app.press("hook-card-1");
  expect(app.shows("hook-detail")).toBe(true);
  // 槽位 1 被移除，详情应关闭
  await app.clock.runFor(1000);
  expect(app.shows("hook-detail")).toBe(false);
  // 槽位 1 再次出现，详情不应自动重新打开
  await app.clock.runFor(1000);
  expect(app.shows("hook-detail")).toBe(false);
}, 60_000);
