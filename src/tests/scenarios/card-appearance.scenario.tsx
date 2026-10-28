// 引入场景测试 API
import { expect, open, test, type App } from "nsh/test";

// 原生截图需要 --target macos。这个夹具从不打开生产环境的 Link。
// 截图输出目录占位符，由测试脚本替换
const OUTPUT = "__OUTPUT_DIRECTORY__";
// 应用入口占位符
const ENTRY = "__APP_ENTRY__";
// 视口尺寸
const viewport = { width: 520, height: 680 };

// 读取页面写入的门控状态
function gates(app: App) {
  return JSON.parse(app.storage()["test.appearanceGates"] ?? "{}");
}

// 截图并输出带元数据的日志行
async function capture(app: App, group: string, index: number) {
  const file = `${group}-${index}.png`;
  await app.screenshot(`${OUTPUT}/${file}`);
  console.log(`CARD_APPEARANCE_CAPTURE=${JSON.stringify({ file, group, index, viewport,
    rect: JSON.parse(app.storage()["test.appearanceRect"]), gates: gates(app), timeMs: Date.now() })}`);
}

// 圆角光带绕四边运行并遵守动效门控
test("rounded Token edge travels around four sides and respects motion gates", async () => {
  const app = await open(ENTRY, { ...viewport, reducedMotion: false, network: {},
    storage: { "test.cardAppearance": JSON.stringify({ mode: "motion" }) } });
  await app.waitFor("usage-surface-scan", { timeout: 15_000 });
  // 等待门控打开
  await app.waitUntil(() => gates(app).active === true && gates(app).foreground === true);
  expect(gates(app).reducedMotion).toBe(false);
  await app.wait(900); // 等卡片的入场动画结束,再只测量它的边框。
  // 连续截 12 帧观察光带位置
  for (let index = 0; index < 12; index++) {
    if (index) await app.wait(800);
    await capture(app, "orbit", index);
  }
  // 关闭 active，光带应静止
  await app.press("appearance-toggle-active");
  await app.waitUntil(() => gates(app).active === false);
  await app.wait(500);
  await capture(app, "inactive", 0);
  await app.wait(1400);
  await capture(app, "inactive", 1);
  // 恢复 active，光带应继续运行
  await app.press("appearance-toggle-active");
  await app.waitUntil(() => gates(app).active === true);
  await app.wait(800);
  await capture(app, "resumed", 0);
  await app.wait(1400);
  await capture(app, "resumed", 1);
  // 切到后台，光带应静止
  await app.press("appearance-toggle-foreground");
  await app.waitUntil(() => gates(app).foreground === false);
  await app.wait(500);
  await capture(app, "background", 0);
  await app.wait(1400);
  await capture(app, "background", 1);
  app.dispose();

  // 开启“减少动态效果”后重新验证
  const reduced = await open(ENTRY, { ...viewport, reducedMotion: true, network: {},
    storage: { "test.cardAppearance": JSON.stringify({ mode: "motion" }) } });
  await reduced.waitFor("usage-surface-scan", { timeout: 15_000 });
  await reduced.waitUntil(() => gates(reduced).reducedMotion === true);
  await reduced.wait(600);
  await capture(reduced, "reduced", 0);
  await reduced.wait(1400);
  await capture(reduced, "reduced", 1);
// 超时 90 秒
}, 90_000);

// 手机与桌面尺寸下的卡片与设置页使用隔离夹具渲染
test("shared phone and desktop cards and settings render with isolated fixture data", async () => {
  for (const [name, width, height] of [["phone", 390, 844], ["desktop", 1120, 900]] as const) {
    const app = await open(ENTRY, { width, height, reducedMotion: true, network: {},
      storage: { "test.cardAppearance": JSON.stringify({ mode: "app" }),
        "loki.panel": JSON.stringify({ showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" }) } });
    await app.waitFor("usage-total", { timeout: 15_000 });
    await app.waitFor("hook-card-4");
    await app.wait(600);
    // 截图保存
    await app.screenshot(`${OUTPUT}/${name}-panel.png`);
    // 输出截图的尺寸等信息
    console.log(`CARD_APPEARANCE_VISUAL=${JSON.stringify({ file: `${name}-panel.png`, width, height, page: "panel" })}`);
    // 切到设置页
    await app.press("menu-settings");
    await app.waitFor("settings-panel-card");
    await app.wait(400);
    await app.screenshot(`${OUTPUT}/${name}-settings.png`);
    console.log(`CARD_APPEARANCE_VISUAL=${JSON.stringify({ file: `${name}-settings.png`, width, height, page: "settings" })}`);
    app.dispose();
  }
}, 60_000);
