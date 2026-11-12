import { expect, open, test } from "nsh/test";

// 显式选择启用的集成场景;夹具通过真实的已认证 TCP
// 提供两帧 GIF,而不是注入快照或图片 URI。
// 截图输出目录占位符
const OUTPUT = "__OUTPUT_DIRECTORY__";
// 夹具桌面的客户端 ID
const CLIENT = "f0f0f0f0-f0f0-40f0-80f0-f0f0f0f0f0f0";

// 正常的 macOS 应用经认证的 TCP 收到动图
test("normal macOS app receives an animated GIF over authenticated TCP", async () => {
  // 使用真实网络，空设备 ID 让应用自行生成
  const app = await open("__APP_ENTRY__", {
    width: 1120, height: 900, network: "live", reducedMotion: false,
    storage: {
      "loki.device": JSON.stringify({ id: "", name: "macOS 动图链路验证" }),
      "loki.panel": JSON.stringify({ showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" }),
    },
  });
  expect("loki.inject" in app.storage()).toBe(false);
  await app.press("menu-settings");
  await app.waitFor(`host-connect-${CLIENT}`, { timeout: 30_000 });
  await app.scroll("settings-scroll", 700);
  await app.press(`host-connect-${CLIENT}`);
  await app.waitFor("devices-pair-code", { timeout: 10_000 });
  // 输出配对码供脚本在桌面端输入
  console.log(`ANIMATION_PAIR_CODE=${app.texts().find((text) => /^[0-9]{4}$/.test(text))}`);
  await app.waitUntil(() => app.texts().includes("已连接 · 正在同步"), { timeout: 55_000 });
  await app.press("menu-panel");
  await app.waitFor("hook-image-1", { timeout: 20_000 });
  await app.waitUntil(() => Object.keys(JSON.parse(app.storage()["loki.imageLoads"] ?? "{}")).length > 0,
    { timeout: 20_000 });
  console.log("ANIMATION_OBSERVE_READY=true");
  await app.wait(25_000);
  for (let index = 0; index < 6; index++) {
    await app.wait(700);
    await app.screenshot(`${OUTPUT}/frame-${index}.png`);
  }
  console.log(`ANIMATION_DECODED=${app.storage()["loki.imageLoads"]}`);
  await app.press("menu-settings");
  await app.scroll("settings-scroll", 800);
  // 自动确认忘记对话框
  app.onDialog(() => "忘记");
  await app.press(`host-forget-${CLIENT}`);
  await app.waitUntil(() => app.texts().includes("已忘记，可重新配对"));
}, 120_000);
