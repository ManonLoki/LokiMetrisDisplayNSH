// 引入测试 API
import { expect, test } from "nsh/test";
// 引入视图状态类型
import type { ViewState } from "../../features/connection";
// 引入打开函数与常量
import { open, PHONE, safe } from "./app-environment";
// 引入夹具快照
import { representativeSnapshot } from "./app-fixtures";
// 引入原生规则查询辅助
import { nativeFunctions, query } from "./support/native-logic";

// 旧版本遗留的面板偏好
const oldPanel = { showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" };
// 构造夹具：空视图加旧面板偏好
async function fixture(formFactor = "phone") {
  // 向原生查询空视图
  const call = await nativeFunctions([query("connection.emptyView", PHONE, "客厅手机")]);
  return { formFactor, projection: {
    ...call<ViewState>("connection.emptyView", PHONE, "客厅手机"), panelConfig: oldPanel,
  } };
}

// 移动端身份与旧面板偏好被迁移到 NSH 存储
test("mobile identity and previous panel choices are adopted into NSH storage", async () => {
  const app = await open("./mobile-app.tsx", { width: 390, height: 2600, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(await fixture()) } });
  // 等待迁移写入
  await app.waitUntil(() => app.storage()["loki.panel"] != null && app.storage()["loki.device"] != null);
  // 设备 ID 应为手机 ID
  expect(JSON.parse(app.storage()["loki.device"]).id).toBe(PHONE);
  // 面板偏好应原样保留
  expect(JSON.parse(app.storage()["loki.panel"])).toEqual(oldPanel);
  // 进入设置页
  await app.press("menu-settings");
  // 改成每页一张
  await app.press("settings-hooks-1x1");
  // 新布局应被保存
  await app.waitUntil(() => JSON.parse(app.storage()["loki.panel"]).hookLayout === "1x1");
});

// 移动端 Hook 详情使用共享对话框
test("mobile Hook details use the shared NSH dialog", async () => {
  const value = await fixture();
  // 填入快照并标记已连接
  value.projection.snapshot = representativeSnapshot();
  value.projection.receivedAtMs = 1_720_111_000_000;
  value.projection.connected = true;
  const app = await open("./mobile-app.tsx", { width: 390, height: 844, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(value) } });
  // 点开第一张卡片
  await app.press("hook-card-1");
  // 详情对话框应出现
  await app.waitFor("hook-detail-dialog");
  // 正文可见
  expect(app.shows("hook-detail-content")).toBe(true);
  // 关闭详情
  await app.press("hook-detail-close");
  // 详情应消失
  await app.waitUntil(() => !app.shows("hook-detail"));
  // 卡片仍在
  expect(app.shows("hook-card-1")).toBe(true);
});

// 电视形态保留遥控导航
test("televisions keep remote navigation in the shared NSH application", async () => {
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(await fixture("tv")) } });
  // 宽屏使用侧栏
  await app.waitFor("nav-side");
  // 第一次按下：焦点落在“展示台”
  await app.keyboard.press("down");
  await app.waitFor("remote-focus-menu-panel");
  // 第二次按下：焦点移到“设置”
  await app.keyboard.press("down");
  await app.waitFor("remote-focus-menu-settings");
  // 确认进入设置页
  await app.keyboard.press("enter");
  await app.waitFor("settings-panel-card");
});

// 手机与平板忽略遥控按键
test("phones and tablets ignore remote navigation keys", async () => {
  const app = await open("./mobile-app.tsx", { width: 960, height: 540, safeArea: safe,
    storage: { "test.mobileApp": JSON.stringify(await fixture("tablet")) } });
  await app.waitFor("nav-side");
  await app.keyboard.press("down");
  // 不应出现焦点框
  expect(app.shows("remote-focus-menu-panel")).toBe(false);
});

// 系统栏通过安全区缩小内容区域
test("mobile system bars reduce the content area through NSH safe-area facts", async () => {
  const value = JSON.stringify(await fixture());
  // 无安全区
  const plain = await open("./mobile-app.tsx", { width: 390, height: 844, safeArea: safe,
    storage: { "test.mobileApp": value } });
  // 等待用量卡片布局被记录
  await plain.waitUntil(() => plain.storage()["loki.frames"]?.includes('"usage"') === true);
  // 记录完整高度
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
  // 设置页仍可打开
  await inset.waitFor("settings-panel-card");
});
