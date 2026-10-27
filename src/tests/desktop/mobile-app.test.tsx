import { expect, test } from "nsh/test";
import type { ViewState } from "../../features/connection";
import { open, PHONE, safe } from "./app-environment";
import { representativeSnapshot } from "./app-fixtures";
import { nativeFunctions, query } from "./support/native-logic";

const oldPanel = { showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" };
async function fixture(formFactor = "phone") {
  const call = await nativeFunctions([query("connection.emptyView", PHONE, "客厅手机")]);
  return { formFactor, projection: {
    ...call<ViewState>("connection.emptyView", PHONE, "客厅手机"), panelConfig: oldPanel,
  } };
}

test("mobile identity and previous panel choices are adopted into NSH storage", async () => {
  const app = await open("./mobile-app.tsx", { width: 390, height: 2600, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(await fixture()) } });
  await app.waitUntil(() => app.storage()["loki.panel"] != null && app.storage()["loki.device"] != null);
  expect(JSON.parse(app.storage()["loki.device"]).id).toBe(PHONE);
  expect(JSON.parse(app.storage()["loki.panel"])).toEqual(oldPanel);
  await app.press("menu-settings");
  await app.press("settings-hooks-1x1");
  await app.waitUntil(() => JSON.parse(app.storage()["loki.panel"]).hookLayout === "1x1");
});

test("mobile Hook details use the shared NSH dialog", async () => {
  const value = await fixture();
  value.projection.snapshot = representativeSnapshot();
  value.projection.receivedAtMs = 1_720_111_000_000;
  value.projection.connected = true;
  const app = await open("./mobile-app.tsx", { width: 390, height: 844, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(value) } });
  await app.press("hook-card-1");
  await app.waitFor("hook-detail-dialog");
  expect(app.shows("hook-detail-content")).toBe(true);
  await app.press("hook-detail-close");
  await app.waitUntil(() => !app.shows("hook-detail"));
  expect(app.shows("hook-card-1")).toBe(true);
});

test("televisions keep remote navigation in the shared NSH application", async () => {
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(await fixture("tv")) } });
  await app.waitFor("nav-side");
  await app.keyboard.press("down");
  await app.waitFor("remote-focus-menu-panel");
  await app.keyboard.press("down");
  await app.waitFor("remote-focus-menu-settings");
  await app.keyboard.press("enter");
  await app.waitFor("settings-panel-card");
});

test("phones and tablets ignore remote navigation keys", async () => {
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(await fixture("tablet")) } });
  await app.waitFor("nav-side");
  await app.keyboard.press("down");
  expect(app.shows("remote-focus-menu-panel")).toBe(false);
});

test("mobile system bars reduce the content area through NSH safe-area facts", async () => {
  const value = JSON.stringify(await fixture());
  const plain = await open("./mobile-app.tsx", { width: 390, height: 844, safeArea: safe,
    storage: { "test.mobileApp": value } });
  await plain.waitUntil(() => plain.storage()["loki.frames"]?.includes('"usage"') === true);
  const fullHeight = JSON.parse(plain.storage()["loki.frames"]).usage.height;
  plain.dispose();
  const inset = await open("./mobile-app.tsx", { width: 390, height: 844,
    safeArea: { top: 48, right: 0, bottom: 34, left: 0 }, storage: { "test.mobileApp": value } });
  await inset.waitUntil(() => inset.storage()["loki.frames"]?.includes('"usage"') === true);
  const insetHeight = JSON.parse(inset.storage()["loki.frames"]).usage.height;
  expect(fullHeight - insetHeight).toBeGreaterThan(35);
  expect(fullHeight - insetHeight).toBeLessThan(45);
  await inset.press("menu-settings");
  await inset.waitFor("settings-panel-card");
});
