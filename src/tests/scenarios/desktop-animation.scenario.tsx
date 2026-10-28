// 引入场景测试 API
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
    width: 1120, height: 900, network: "live",
    storage: {
      // 设备名称
      "loki.device": JSON.stringify({ id: "", name: "macOS 动图链路验证" }),
      // 面板配置
      "loki.panel": JSON.stringify({ showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" }),
    },
  });
  // 确认没有注入夹具
  expect(app.storage()["loki.inject"]).toBeNull();
  // 进入设置页
  await app.press("menu-settings");
  // 等待发现夹具桌面
  await app.waitFor(`host-connect-${CLIENT}`, { timeout: 30_000 });
  // 滚动到连接按钮
  await app.scroll("settings-scroll", 700);
  // 发起配对
  await app.press(`host-connect-${CLIENT}`);
  // 等待配对码出现
  await app.waitFor("devices-pair-code", { timeout: 10_000 });
  // 输出配对码供脚本在桌面端输入
  console.log(`ANIMATION_PAIR_CODE=${app.texts().find((text) => /^[0-9]{4}$/.test(text))}`);
  // 等待连接成功
  await app.waitUntil(() => app.texts().includes("已连接 · 正在同步"), { timeout: 55_000 });
  // 回到展示台
  await app.press("menu-panel");
  await app.waitFor("hook-image-1", { timeout: 20_000 });
  // 等待图片解码完成
  await app.waitUntil(() => Object.keys(JSON.parse(app.storage()["loki.imageLoads"] ?? "{}")).length > 0,
    { timeout: 20_000 });
  // 打开详情
  await app.press("hook-card-1");
  await app.waitFor("hook-detail-image");
  // 通知脚本开始观察
  console.log("ANIMATION_OBSERVE_READY=true");
  // 等待外部观察动图
  await app.wait(25_000);
  // 连续截 6 帧
  for (let index = 0; index < 6; index++) {
    await app.wait(700);
    await app.screenshot(`${OUTPUT}/frame-${index}.png`);
  }
  // 输出解码记录
  console.log(`ANIMATION_DECODED=${app.storage()["loki.imageLoads"]}`);
  // 关闭详情
  await app.press("hook-detail-close");
  // 回到设置页
  await app.press("menu-settings");
  await app.scroll("settings-scroll", 800);
  // 自动确认忘记对话框
  app.onDialog(() => "忘记");
  // 忘记该桌面
  await app.press(`host-forget-${CLIENT}`);
  await app.waitUntil(() => app.texts().includes("已忘记，可重新配对"));
}, 120_000);
