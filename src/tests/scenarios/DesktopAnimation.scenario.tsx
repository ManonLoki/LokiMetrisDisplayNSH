import { expect, open, test } from "nsh/test";

// Explicit opt-in integration scenario; the fixture supplies the two-frame GIF
// over real authenticated TCP, rather than injecting a snapshot or image URI.
const OUTPUT = "__OUTPUT_DIRECTORY__";
const CLIENT = "f0f0f0f0-f0f0-40f0-80f0-f0f0f0f0f0f0";

test("normal macOS app receives an animated GIF over authenticated TCP", async () => {
  const app = await open("__APP_ENTRY__", {
    width: 1120, height: 900, network: "live",
    storage: {
      "loki.device": JSON.stringify({ id: "", name: "macOS 动图链路验证" }),
      "loki.panel": JSON.stringify({ showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" }),
    },
  });
  expect(app.storage()["loki.inject"]).toBeNull();
  await app.press("menu-settings");
  await app.waitFor(`host-connect-${CLIENT}`, { timeout: 30_000 });
  await app.scroll("settings-scroll", 700);
  await app.press(`host-connect-${CLIENT}`);
  await app.waitFor("devices-pair-code", { timeout: 10_000 });
  console.log(`ANIMATION_PAIR_CODE=${app.texts().find((text) => /^[0-9]{4}$/.test(text))}`);
  await app.waitUntil(() => app.texts().includes("已连接 · 正在同步"), { timeout: 55_000 });
  await app.press("menu-panel");
  await app.waitFor("hook-image-1", { timeout: 20_000 });
  await app.waitUntil(() => Object.keys(JSON.parse(app.storage()["loki.imageLoads"] ?? "{}")).length > 0,
    { timeout: 20_000 });
  await app.press("hook-card-1");
  await app.waitFor("hook-detail-image");
  console.log("ANIMATION_OBSERVE_READY=true");
  await app.wait(25_000);
  for (let index = 0; index < 6; index++) {
    await app.wait(700);
    await app.screenshot(`${OUTPUT}/frame-${index}.png`);
  }
  console.log(`ANIMATION_DECODED=${app.storage()["loki.imageLoads"]}`);
  await app.press("hook-detail-close");
  await app.press("menu-settings");
  await app.scroll("settings-scroll", 800);
  app.onDialog(() => "忘记");
  await app.press(`host-forget-${CLIENT}`);
  await app.waitUntil(() => app.texts().includes("已忘记，可重新配对"));
}, 120_000);
