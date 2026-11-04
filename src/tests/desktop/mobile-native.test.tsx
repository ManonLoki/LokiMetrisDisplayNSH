import { expect, test } from "nsh/test";
import { open, PHONE, DESKTOP, T0, RECEIVED, KEY_DEVICE, KEY_PANEL, KEY_PAIRED, KEY_CACHE, KEY_INJECT } from "./app-environment";
import { representativeSnapshot } from "./app-fixtures";

// 用 --target ios 运行：不指定虚拟视口，截图与输入均来自模拟器的真实 UIKit 宿主。
const output = "output/tests/nsh-0.4.0/ios";
const device = JSON.stringify({ id: PHONE, name: "Simulator" });
// 与 fixtures/assets/two-frame.gif 相同的 64×64 两帧资源；内联后不依赖宿主电脑的文件路径。
const image = "data:image/gif;base64,R0lGODdhQABAAKIAAAAAAP8mAAQz/////wAAAAAAAAAAAAAAACH/C05FVFNDQVBFMi4wAwEAAAAh+QQFZAAEACwAAAAAQABAAAADRxi63P4wykmrvTjrzbv/YCiOZGmeaKqubOu+cCzPdG3feK7vfO//wKBwSCwaj8ikcslsOp/QqHRKrVqv2Kx2y+16v+CwOJAAACH5BAVkAAQALAAAAABAAEAAAANHKLrc/jDKSau9OOvNu/9gKI5kaZ5oqq5s675wLM90bd94ru987//AoHBILBqPyKRyyWw6n9CodEqtWq/YrHbL7Xq/4LBYkAAAOw==";

test("native mobile input saves the name and panel choices survive reopening", async () => {
  const app = await open("../../../App.tsx", {
    colorScheme: "dark", reducedMotion: true, storage: { [KEY_DEVICE]: device },
  });
  await app.waitFor("app-root");
  await app.press("menu-settings");
  await app.press("settings-usage-detailed");
  await app.press("settings-hooks-2x2");
  await app.waitUntil(() => {
    const panel = JSON.parse(app.storage()[KEY_PANEL] ?? "{}");
    return panel.usageDetail === "detailed" && panel.hookLayout === "2x2";
  });

  await app.waitUntil(() => app.value("settings-device-name") === "Simulator");
  await app.clear("settings-device-name");
  await app.type("settings-device-name", "NSH040");
  await app.waitUntil(() => app.value("settings-device-name") === "NSH040");
  await app.press("settings-save-name");
  await app.waitUntil(() => JSON.parse(app.storage()[KEY_DEVICE]).name === "NSH040");
  expect(app.texts()).toContain("已保存，下次连接生效");
  await app.screenshot(`${output}/settings-saved.png`);

  const kept = { [KEY_DEVICE]: app.storage()[KEY_DEVICE], [KEY_PANEL]: app.storage()[KEY_PANEL] };
  app.dispose();
  const again = await open("../../../App.tsx", { colorScheme: "dark", reducedMotion: true, storage: kept });
  await again.waitFor("app-root");
  await again.press("menu-settings");
  await again.waitUntil(() => again.value("settings-device-name") === "NSH040");
  expect(again.storage()[KEY_PANEL]).toBe(kept[KEY_PANEL]);
  expect(again.texts()).toContain("显示总量与各 Agent 明细");
  expect(again.texts()).toContain("每页四张，左右翻页查看更多");
  await again.screenshot(`${output}/settings-reopened.png`);
}, 60_000);

test("native mobile Hook image decodes and its shared detail dialog opens and closes", async () => {
  const snapshot = representativeSnapshot();
  snapshot.hooks = [snapshot.hooks[0]];
  const hash = "cd".repeat(32);
  snapshot.hooks[0].image = hash;
  snapshot.hooks[0].content = "模拟器图片详情正文";
  const app = await open("./test-app.tsx", {
    colorScheme: "dark", reducedMotion: true,
    storage: {
      [KEY_DEVICE]: device,
      "test.staticView": JSON.stringify({
        deviceId: PHONE, deviceName: "Simulator", hosts: [], selectedId: null,
        status: "已连接", error: null, pairingCode: null, pairingDeadlineMs: null,
        connected: true, snapshot, receivedAtMs: RECEIVED, live: true,
        cacheSourceId: DESKTOP, images: { [hash]: image },
      }),
    },
  });
  await app.waitFor("hook-image-1");
  await app.waitUntil(() => JSON.parse(app.storage()["loki.imageLoads"] ?? "{}")[hash]?.width === 64);
  expect(JSON.parse(app.storage()["loki.imageLoads"])[hash]).toEqual({ width: 64, height: 64 });
  expect(app.texts()).not.toContain("图片无法读取");
  await app.screenshot(`${output}/hook-image.png`);
  await app.press("hook-card-1");
  await app.waitFor("hook-detail-image");
  expect(app.texts()).toContain("模拟器图片详情正文");
  await app.screenshot(`${output}/hook-detail.png`);
  await app.press("hook-detail-close");
  await app.waitUntil(() => !app.shows("hook-detail"));
  expect(app.shows("hook-card-1")).toBe(true);
}, 60_000);

test("native mobile pairing waits for a snapshot and forgetting requires confirmation", async () => {
  const app = await open("./test-app.tsx", {
    clock: { time: T0 }, colorScheme: "dark", reducedMotion: true,
    storage: {
      [KEY_DEVICE]: device,
      [KEY_INJECT]: JSON.stringify({
        discovered: [{ id: DESKTOP, name: "模拟器桌面" }],
        // 留出真实触摸和滚动所需时间；手动时钟分别推进握手与首份快照。
        events: [
          { atMs: T0 + 20_000, kind: "handshake" },
          { atMs: T0 + 40_000, kind: "snapshot", receivedAtMs: RECEIVED, snapshot: representativeSnapshot() },
        ],
      }),
    },
  });
  await app.waitFor("app-root");
  await app.press("menu-settings");
  await app.waitFor(`host-connect-${DESKTOP}`);
  await app.press(`host-connect-${DESKTOP}`);
  await app.waitFor("devices-pair-code");
  expect(app.texts().some((text) => /^\d{4}$/.test(text))).toBe(true);
  expect(app.texts().join("\n")).not.toContain("已连接桌面");
  await app.screenshot(`${output}/pairing-code.png`);

  await app.clock!.runFor(20_000);
  await app.waitUntil(() => app.storage()[KEY_PAIRED]?.includes(DESKTOP) === true);
  expect(app.shows("devices-pair-code")).toBe(false);
  expect(app.storage()[KEY_CACHE]).toBeUndefined();
  await app.clock!.runFor(20_000);
  await app.waitUntil(() => app.texts().includes("已连接桌面"));
  expect(JSON.parse(app.storage()[KEY_CACHE]).receivedAtMs).toBe(RECEIVED);
  await app.screenshot(`${output}/paired-connected.png`);

  app.onDialog(() => "取消");
  await app.press(`host-forget-${DESKTOP}`);
  expect(app.storage()[KEY_PAIRED]?.includes(DESKTOP)).toBe(true);
  expect(app.dialogs()[0].title).toBe("忘记此桌面？");
  app.onDialog(() => "忘记");
  await app.press(`host-forget-${DESKTOP}`);
  await app.waitUntil(() => app.storage()[KEY_CACHE] == null);
  expect(app.storage()[KEY_PAIRED]?.includes(DESKTOP) ?? false).toBe(false);
  await app.press("menu-panel");
  await app.waitUntil(() => app.texts().includes("等待同步"));
  expect(app.texts()).not.toContain("1.5K");
  await app.screenshot(`${output}/forgotten.png`);
}, 60_000);
