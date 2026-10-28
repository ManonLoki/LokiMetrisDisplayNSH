// 引入测试 API
import { expect, test } from "nsh/test";
// 引入打开函数、常量与存储键
import { open, PHONE, DESKTOP, T0, RECEIVED, KEY_PANEL, KEY_DEVICE, KEY_PAIRED, KEY_CACHE, KEY_INJECT, KEY_FRAMES, safe } from "./app-environment";
// 引入夹具快照
import { representativeSnapshot } from "./app-fixtures";
// 引入测试用 PNG 资源
import hookPNG from "../fixtures/assets/hook.png";
// 引入测试用 GIF 资源
import hookGIF from "../fixtures/assets/hook.gif";
// 引入配对规则
import { PAIRING_WINDOW_MS, formatSyncTime, makePairingCode } from "./support/fixture-rules";

// 窄屏与宽屏导航，以及空展示台
test("narrow and wide navigation and the empty stage", async () => {
  // 以手机尺寸打开
  const app = await open("./test-app.tsx", { width: 390, height: 844, safeArea: safe });
  // 等待根节点
  await app.waitFor("app-root");
  // 窄屏使用底部导航
  expect(app.shows("nav-bottom")).toBe(true);
  // 不显示侧栏
  expect(app.shows("nav-side")).toBe(false);
  // 导航标签应可见
  expect(app.texts()).toContain("展示台");
  expect(app.texts()).toContain("设置");
  // 窄屏竖向分割
  expect(app.shows("panel-split-column")).toBe(true);
  // 等待用量卡片和 Hooks 面板都有布局高度
  await app.waitUntil(() => {
    const raw = app.storage()[KEY_FRAMES];
    if (!raw) return false;
    const frames = JSON.parse(raw) as { usage?: { height: number }; hook?: { height: number } };
    return (frames.usage?.height ?? 0) > 0 && (frames.hook?.height ?? 0) > 0;
  });
  // 读取两个面板的布局尺寸
  const frames = JSON.parse(app.storage()[KEY_FRAMES]) as {
    usage: { width: number; height: number };
    hook: { width: number; height: number };
  };
  // 两个面板都应有足够高度
  expect(frames.usage.height).toBeGreaterThan(200);
  expect(frames.hook.height).toBeGreaterThan(200);
  // 高度差不超过 25%
  expect(Math.abs(frames.usage.height - frames.hook.height)).toBeLessThan(frames.usage.height * 0.25);
  // 横向小窗：仍用底栏，面板改为左右分割
  await app.setViewport(600, 300);
  expect(app.shows("panel-split-row")).toBe(true);
  expect(app.shows("nav-bottom")).toBe(true);
  expect(app.shows("nav-side")).toBe(false);
  // 宽度达到 700 以上：改用侧栏
  await app.setViewport(800, 900);
  expect(app.shows("nav-side")).toBe(true);
  expect(app.shows("nav-bottom")).toBe(false);
  expect(app.shows("panel-split-column")).toBe(true);
  expect(app.texts()).toContain("展示台");
  expect(app.texts()).toContain("设置");

  // 回到手机尺寸
  await app.setViewport(390, 844);
  // 进入设置页
  await app.press("设置");
  // 关闭用量
  await app.press("settings-show-usage");
  // 关闭 Hooks
  await app.press("settings-show-hooks");
  // 等待配置被保存
  await app.waitUntil(() => app.storage()[KEY_PANEL]?.includes('"showUsage":false') === true);
  // 设置页提示如何重新开启
  expect(app.texts()).toContain("开启一项内容，即可在展示台中显示。");
  // 回到展示台
  await app.press("展示台");
  // 空状态提供跳转设置的入口
  expect(app.texts()).toContain("前往设置");
  expect(app.texts()).toContain("暂无卡片");
  // 点击跳转
  await app.press("前往设置");
  // 回到设置页
  expect(app.texts()).toContain("面板显示与设备连接");
}, 60_000);

// 面板选择在重新打开后保留
test("panel choices persist across a new open", async () => {
  const app = await open("./test-app.tsx", {
    width: 390,
    height: 844,
    safeArea: safe,
    storage: { [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }) },
  });
  await app.waitFor("app-root");
  // 点击“设置”
  await app.press("设置");
  // 等待设置页出现
  await app.waitUntil(() => app.shows("settings-usage-detailed"));
  // 等待原生宿主完成布局，避免坐标点击落空
  await app.wait(100); // 等待原生宿主为设置页的点击目标完成布局,再进行坐标点击。
  // 选择详细模式
  await app.press("settings-usage-detailed");
  await app.waitUntil(() => (app.storage()[KEY_PANEL] ?? "").includes('"usageDetail":"detailed"'));
  await app.wait(100);
  // 选择 2x2 布局
  await app.press("settings-hooks-2x2");
  // 等待两项选择都被保存
  await app.waitUntil(() => {
    const raw = app.storage()[KEY_PANEL] ?? "";
    return raw.includes('"usageDetail":"detailed"') && raw.includes('"hookLayout":"2x2"');
  });
  // 取出需要保留的存储
  const kept = {
    [KEY_PANEL]: app.storage()[KEY_PANEL],
    [KEY_DEVICE]: app.storage()[KEY_DEVICE],
  };
  // 用保留的存储重新打开
  const again = await open("./test-app.tsx", { width: 390, height: 844, safeArea: safe, storage: kept });
  await again.waitFor("app-root");
  await again.press("menu-settings");
  await again.waitUntil(() => again.shows("settings-hooks-2x2"));
  // 详细模式的说明应显示
  expect(again.texts()).toContain("显示总量与各 Agent 明细");
  // 精简模式的说明不应显示
  expect(again.texts()).not.toContain("突出总 Token 用量");
  // 2x2 的说明应显示
  expect(again.texts()).toContain("每页四张，左右翻页查看更多");
  // 1x1 的说明不应显示
  expect(again.texts()).not.toContain("每页一张，完整展示图片");
  // 存储内容保持不变
  expect(again.storage()[KEY_PANEL]).toBe(kept[KEY_PANEL]);
});

// 快照数字、Hooks 与连接阶段
test("snapshot numbers, hooks and connection phases", async () => {
  // 取夹具快照
  const snapshot = representativeSnapshot();
  // 使用假时钟，并注入一次握手和一次快照
  const app = await open("./test-app.tsx", {
    clock: { time: T0 },
    width: 390,
    height: 1800,
    safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }),
      // 发现一台桌面；1 秒后握手，2 秒后收到快照
      [KEY_INJECT]: JSON.stringify({
        discovered: [{ id: DESKTOP, name: "书房桌面" }],
        events: [
          { atMs: T0 + 1000, kind: "handshake" },
          { atMs: T0 + 2000, kind: "snapshot", receivedAtMs: RECEIVED, snapshot },
        ],
      }),
    },
  });
  // 未连接前展示台显示“等待同步”
  await app.waitUntil(() => app.shows("app-root") && app.texts().includes("等待同步"));
  // 此时不应显示已连接
  expect(app.texts().join("\n")).not.toContain("已连接");
  // 主机只出现在设置页
  expect(app.texts()).not.toContain("书房桌面");
  // 进入设置页
  await app.press("设置");
  // 等待主机出现
  await app.waitUntil(() => app.texts().includes("书房桌面"));
  // 尚未配对的标题
  expect(app.texts()).toContain("自动发现桌面");
  // 发起配对
  await app.press(`host-connect-${DESKTOP}`);
  // 找到四位配对码
  const code = app.texts().find((text) => /^\d{4}$/.test(text));
  // 配对码应与夹具规则一致
  expect(code).toBe(makePairingCode(T0, DESKTOP));
  // 显示有效期提示
  expect(app.texts()).toContain("60 秒内有效 · 保持应用在前台");
  expect(app.texts().join("\n")).not.toContain("已连接");
  // 推进 1 秒触发握手
  await app.clock.runFor(1000);
  expect(app.texts().join("\n")).not.toContain("已连接");
  // 握手后配对码消失
  expect(app.texts().includes(code ?? "")).toBe(false);
  // 配对记录已保存
  expect(app.storage()[KEY_PAIRED]?.includes(DESKTOP)).toBe(true);
  // 再推进 1 秒收到快照
  await app.clock.runFor(1000);
  // 收到快照后才算已连接
  await app.waitUntil(() => app.texts().join("\n").includes("已连接"));
  expect(app.texts()).toContain("已连接桌面");
  // 快照应被缓存
  const cached = JSON.parse(app.storage()[KEY_CACHE]);
  // 接收时间保持事件里的值
  expect(cached.receivedAtMs).toBe(RECEIVED);
  // 代理顺序与快照一致
  expect(cached.snapshot.tokens.agents.map((agent: { agent: string }) => agent.agent)).toEqual(
    snapshot.tokens?.agents.map((agent) => agent.agent),
  );

  // 切到详细模式
  await app.press("详细");
  await app.press("展示台");
  // 详细模式特有的时区行
  await app.waitUntil(() => app.texts().includes("设备时区"));
  // 取全部可见文本
  const texts = app.texts();
  // 统计日期
  expect(texts).toContain(snapshot.tokens!.day.date);
  // 不显示同步时间
  expect(texts.join("\n")).not.toContain(formatSyncTime(T0 + 2000));
  // 总量的紧凑与精确写法
  expect(texts).toContain("1.5K");
  expect(texts).toContain("1,500");
  expect(texts).toContain("999");
  // 大数值的紧凑与精确写法
  expect(texts).toContain("221.19M");
  expect(texts).toContain("221,193,571");
  expect(texts).toContain("0");
  // 未启用的代理不显示
  expect(texts).not.toContain("SecretBot");
  expect(texts).not.toContain("1M");
  // 设备时区
  expect(texts).toContain("Asia/Shanghai");
  // 代理的显示顺序
  const order = ["Codex", "Claude Code", "Cursor", "Grok", "Other Agent", "WorkBuddy"].map((name) => texts.indexOf(name));
  expect(order.every((index) => index >= 0)).toBe(true);
  // 并按固定顺序排列
  for (let index = 1; index < order.length; index += 1) expect(order[index]).toBeGreaterThan(order[index - 1]);
  // 额度窗口出现次数：每月 1 次、每周 2 次、5h 2 次
  expect(texts.filter((text) => text === "每月").length).toBe(1);
  expect(texts.filter((text) => text === "每周").length).toBe(2);
  expect(texts.filter((text) => text === "5h").length).toBe(2);
  // 同一供应商内窗口顺序：每月、每周、5h
  const afterMonthly = texts.slice(texts.indexOf("每月"));
  expect(afterMonthly.indexOf("每周")).toBeGreaterThan(0);
  expect(afterMonthly.indexOf("每周")).toBeLessThan(afterMonthly.indexOf("5h"));
  // 各窗口的剩余百分比
  expect(texts).toContain("10%");
  expect(texts).toContain("40%");
  expect(texts).toContain("90%");
  expect(texts).not.toContain("Token 用量 · 缓存");

  // 第一页是“空闲”的 Hook
  expect(texts).toContain("空闲");
  // 原生分页器会让相邻页面保持挂载,以便滑动预览。
  // 它的无障碍树包含那些页面;当前页在下面断言。
  // 1x1 布局共 5 页
  expect(app.texts()).toContain("1 / 5 · 左右滑动");
  // 向左滑到第二页
  await app.drag("hooks-pager", { x: -260 });
  expect(app.texts()).toContain("运行");
  // 页码随之更新
  expect(app.texts()).toContain("2 / 5 · 左右滑动");
  await app.press("设置");
  // 切到 2x2 布局
  await app.press("settings-hooks-2x2");
  await app.press("展示台");
  // 2x2 共两页
  expect(app.texts()).toContain("1 / 2 · 左右滑动");
  expect(app.texts()).toContain("空闲");
  expect(app.texts()).toContain("运行");
  // 第一页四张卡片的状态
  expect(app.texts()).toContain("询问");
  expect(app.texts()).toContain("出错");
  // 卡片上不显示正文
  expect(app.texts()).not.toContain("桌面回传的正文");
  await app.drag("hooks-pager", { x: -260 });
  // 第二页有占位
  expect(app.texts()).toContain("暂无活动");
  expect(app.texts()).toContain("2 / 2 · 左右滑动");
  // 打开第五条 Hook 的详情
  await app.press("hook-card-5");
  // 详情显示截断提示与完整正文
  expect(app.texts()).toContain("正文已截断");
  expect(app.texts()).toContain("桌面回传的正文");
  // 图片尚未缓存时的说明
  expect(app.texts()).toContain("图片尚未缓存");
  // 关闭详情
  await app.press("关闭");
  expect(app.shows("hook-detail")).toBe(false);

  // 点击“设置”
  await app.press("设置");
  // 自动确认忘记对话框
  app.onDialog(() => "忘记");
  // 忘记桌面
  await app.press(`host-forget-${DESKTOP}`);
  // 缓存应被清除
  await app.waitUntil(() => app.storage()[KEY_CACHE] == null);
  // 配对记录应被删除
  expect(app.storage()[KEY_PAIRED]?.includes(DESKTOP) ?? false).toBe(false);
  expect(app.texts().join("\n")).not.toContain("已连接");
  await app.press("展示台");
  expect(app.texts()).toContain("等待同步");
  // 旧数字不应残留
  expect(app.texts()).not.toContain("1.5K");
}, 60_000);

// 配对窗口过期后可以重新开始
test("pairing window expires and can be started again", async () => {
  const app = await open("./test-app.tsx", {
    clock: { time: T0 },
    width: 390,
    height: 844,
    safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }),
      // 预置发现到的桌面
      [KEY_INJECT]: JSON.stringify({ discovered: [{ id: DESKTOP, name: "书房桌面" }], events: [] }),
    },
  });
  // 等待应用根节点
  await app.waitFor("app-root");
  // 点击“设置”
  await app.press("设置");
  // 等到桌面名称出现
  await app.waitUntil(() => app.texts().includes("书房桌面"));
  // 发起配对
  await app.press(`host-connect-${DESKTOP}`);
  // 显示配对码
  expect(app.texts()).toContain(makePairingCode(T0, DESKTOP));
  // 快进整个配对窗口
  await app.clock.fastForward(PAIRING_WINDOW_MS);
  // 提示窗口已过期
  await app.waitUntil(() => app.texts().includes("配对窗口已过期，请重新选择桌面。"));
  expect(app.texts().join("\n")).not.toContain("已连接");
  // 旧配对码消失
  expect(app.texts().includes(makePairingCode(T0, DESKTOP))).toBe(false);
  await app.press(`host-connect-${DESKTOP}`);
  // 重新配对后的新配对码基于新的时间
  expect(app.texts()).toContain(makePairingCode(T0 + PAIRING_WINDOW_MS, DESKTOP));
});

// 设备名称边输入边校验，只保存合法的改动
test("device name validates while typing and only saves a legal dirty draft", async () => {
  const app = await open("./test-app.tsx", {
    width: 390,
    height: 844,
    safeArea: safe,
    storage: { [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "客厅手机" }) },
  });
  await app.waitFor("app-root");
  // 点击“设置”
  await app.press("设置");
  // 输入框载入已保存名称
  await app.waitUntil(() => app.value("settings-device-name") === "客厅手机");
  await app.clear("settings-device-name");
  // 构造超过 160 字节的名称
  const tooLong = "a".repeat(161);
  // 输入超长名称
  await app.type("settings-device-name", tooLong);
  await app.waitUntil(() => app.value("settings-device-name") === tooLong);
  // 保存按钮应被禁用
  expect(app.enabled("settings-save-name")).toBe(false);
  // 显示错误提示
  expect(app.texts()).toContain("设备名称不能超过 160 个 UTF-8 字节");
  // 存储中的名称未变
  expect(JSON.parse(app.storage()[KEY_DEVICE]).name).toBe("客厅手机");
  await app.clear("settings-device-name");
  // 输入合法名称
  await app.type("settings-device-name", "书房展示");
  // 保存
  await app.press("保存名称");
  await app.waitUntil(() => JSON.parse(app.storage()[KEY_DEVICE]).name === "书房展示");
  // 保存后的反馈
  expect(app.texts()).toContain("已保存，下次连接生效");
  // 重新打开后应读到新名称
  const again = await open("./test-app.tsx", {
    width: 390,
    height: 844,
    safeArea: safe,
    storage: { [KEY_DEVICE]: app.storage()[KEY_DEVICE] },
  });
  await again.press("设置");
  await again.waitUntil(() => again.value("settings-device-name") === "书房展示");
});

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

// Hook 滑动翻页，缓存的 PNG 与 GIF 进入图片渲染器
test("hook swipes change pages and cached PNG and GIF reach the image renderer", async () => {
  const snapshot = representativeSnapshot();
  // 两张图片的哈希
  const pngHash = "cd".repeat(32);
  const gifHash = "ef".repeat(32);
  // 给前两条 Hook 绑定图片
  snapshot.hooks[0].image = pngHash;
  snapshot.hooks[1].image = gifHash;
  snapshot.hooks[0].content = "图片详情正文";
  // 已配对的桌面在 1 秒后推送带图片的快照
  const app = await open("./test-app.tsx", {
    clock: { time: T0 }, width: 390, height: 844, safeArea: safe,
    storage: {
      [KEY_DEVICE]: JSON.stringify({ id: PHONE, name: "手机" }),
      [KEY_PAIRED]: JSON.stringify({ [DESKTOP]: { clientId: DESKTOP, savedAtMs: T0 } }),
      [KEY_INJECT]: JSON.stringify({ discovered: [{ id: DESKTOP, name: "桌面" }], events: [
        { atMs: T0 + 1000, kind: "snapshot", snapshot, receivedAtMs: RECEIVED, images: { [pngHash]: hookPNG.url.replace(/^asset:\/*/, "file:///"), [gifHash]: hookGIF.url.replace(/^asset:\/*/, "file:///") } },
      ] }),
    },
  });
  // 点击“设置”
  await app.press("设置");
  // 连接桌面
  await app.press(`host-connect-${DESKTOP}`);
  await app.clock.runFor(1000);
  await app.press("展示台");
  // 第一张图片已挂载
  expect(app.shows("hook-image-1")).toBe(true);
  // 是否具备真实渲染平台
  let rendered = true;
  // 能截图才说明有真实渲染
  try { await app.screenshot(); }
  catch (error) {
    // 无真实平台时跳过像素相关断言
    if (String(error).includes("a screenshot needs a real platform")) rendered = false;
    else throw error;
  }
  // 等待 PNG 解码，尺寸 48
  if (rendered) {
    await app.waitUntil(() => (JSON.parse(app.storage()["loki.imageLoads"] ?? "{}"))[pngHash]?.width === 48);
  }
  // 翻到第二页
  await app.drag("hooks-pager", { x: -260 });
  expect(app.texts()).toContain("2 / 5 · 左右滑动");
  // 第二页的图片已挂载
  expect(app.shows("hook-image-2")).toBe(true);
  if (rendered) {
    await app.waitUntil(() => (JSON.parse(app.storage()["loki.imageLoads"] ?? "{}"))[gifHash]?.width === 1);
    // 读取解码记录
    const decoded = JSON.parse(app.storage()["loki.imageLoads"]);
    // PNG 为 48x48
    expect(decoded[pngHash]).toEqual({ width: 48, height: 48 });
    expect(decoded[gifHash]).toEqual({ width: 1, height: 1 });
  }
  // 翻回第一页
  await app.drag("hooks-pager", { x: 260 });
  expect(app.texts()).toContain("1 / 5 · 左右滑动");
  // 打开详情
  await app.press("hook-card-1");
  // 详情里有大图
  expect(app.shows("hook-detail-image")).toBe(true);
  expect(app.texts()).toContain("图片详情正文");
  await app.press("hook-detail-close");
  await app.waitUntil(() => !app.shows("hook-detail"));
  // 切到详细用量模式
  await app.press("menu-settings");
  await app.waitFor("settings-usage-detailed");
  await app.press("settings-usage-detailed");
  await app.press("展示台");
  expect(app.shows("usage-scroll")).toBe(true);
  // 滚动用量卡片
  await app.scroll("usage-scroll", 500);
  // 明细末尾的代理可见
  expect(app.texts()).toContain("WorkBuddy");
  // 两个面板的高度差应在 80 以内
  const frames = JSON.parse(app.storage()[KEY_FRAMES]);
  expect(Math.abs(frames.usage.height - frames.hook.height)).toBeLessThan(80);
}, 60_000);

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
