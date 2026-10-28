// 引入 nsh 运行时与存储
import { effect, mount, reduceMotion, signal, Storage, Text, View } from "nsh";
// 引入按钮
import { Button } from "nsh/base";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入视图状态类型
import type { ViewState } from "../../features/connection";
// 引入用量卡片
import { UsageCard } from "../../features/usage";
// 引入动效作用域
import { CyberMotionScope } from "../../shared/ui/effects";
// 引入色板
import { cyberColors as color } from "../../shared/ui/theme";
// 引入代表性快照
import { representativeSnapshot } from "./app-fixtures";

// 显式的仅 UI 夹具。不会打开 Link、生命周期所有者、网络或已保存的配对。
// 读取测试选项后挂载
void (async () => {
  // 读取测试选项
  const options = JSON.parse((await Storage.getItem("test.cardAppearance")) ?? "{}");
  // 使用夹具快照
  const snapshot = representativeSnapshot();
  // 假想桌面 ID
  const source = "22222222-2222-4222-8222-222222222222";
  // 固定的面板配置
  const config = { showUsage: true, showHooks: true, usageDetail: "detailed" as const, hookLayout: "2x2" as const };
  // 构造已连接的视图状态
  const state: ViewState = {
    deviceId: "11111111-1111-4111-8111-111111111111", deviceName: "外观验证机",
    hosts: [{ id: source, name: "隔离展示样本", paired: true, available: true }],
    selectedId: source, status: "已连接 · 正在同步", error: null, assetWarning: null,
    pairingCode: null, pairingDeadlineMs: null, connected: true, snapshot,
    receivedAtMs: 1_720_111_000_000, live: true, cacheSourceId: source,
    paired: { [source]: { clientId: source, savedAtMs: 1_720_111_000_000 } }, panelConfig: config,
  };
  // 非动效模式：挂载完整应用，端口返回固定状态
  if (options.mode !== "motion") {
    mount(createApp({ foreground: () => true, link: {
      initialize: () => JSON.stringify(state), poll: () => JSON.stringify(state), command: () => {},
    } }));
    return;
  }
  // 动效模式：用开关控制 active 与前台
  const active = signal(true), foreground = signal(true);
  // 卡片固定矩形
  const rect = { x: 24, y: 72, width: 472, height: 520 };
  // 把矩形写入存储供测试读取
  await Storage.setItem("test.appearanceRect", JSON.stringify(rect));
  // 挂载只含用量卡片和两个开关的页面
  mount(() => {
    // 把门控状态持续写入存储
    effect(() => {
      void Storage.setItem("test.appearanceGates", JSON.stringify({ active: active(), foreground: foreground(), reducedMotion: reduceMotion() }));
    });
    return <View testID="appearance-root" style={{ flex: 1, backgroundColor: color.canvas }}>
      {/* 两个开关按钮 */}
      <View style={{ position: "absolute", left: 24, top: 12, height: 44, width: 472, flexDirection: "row", gap: 12 }}>
        {/* 切换 active */}
        <Button testID="appearance-toggle-active" onPress={() => active.set(!active())} style={{ flex: 1, padding: 10, backgroundColor: color.surface }}>
          <Text style={{ color: color.ink }}>active: {() => String(active())}</Text>
        </Button>
        {/* 切换前台状态 */}
        <Button testID="appearance-toggle-foreground" onPress={() => foreground.set(!foreground())} style={{ flex: 1, padding: 10, backgroundColor: color.surface }}>
          <Text style={{ color: color.ink }}>foreground: {() => String(foreground())}</Text>
        </Button>
      </View>
      {/* 动效作用域 */}
      <CyberMotionScope foreground={foreground}>
        {/* 卡片固定位置容器 */}
        <View testID="appearance-card-frame" style={{ position: "absolute", left: rect.x, top: rect.y, width: rect.width, height: rect.height }}>
          {/* 被测的用量卡片 */}
          <UsageCard width={() => rect.width} height={() => rect.height} snapshot={() => snapshot}
            detail={() => "compact"} active={active} />
        </View>
      </CyberMotionScope>
    </View>;
  });
})();
