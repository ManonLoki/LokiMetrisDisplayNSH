import { effect, mount, reduceMotion, signal, Storage, Text, View } from "nsh";
import { Button } from "nsh/base";
import { createApp } from "../../app/application";
import type { ViewState } from "../../features/connection";
import { UsageCard } from "../../features/usage";
import { CyberMotionScope } from "../../shared/ui/effects";
import { cyberColors as color } from "../../shared/ui/theme";
import { representativeSnapshot } from "./app-fixtures";

// 显式的仅 UI 夹具。不会打开 Link、生命周期所有者、网络或已保存的配对。
// 读取测试选项后挂载
void (async () => {
  const options = JSON.parse((await Storage.getItem("test.cardAppearance")) ?? "{}");
  const snapshot = representativeSnapshot();
  const source = "22222222-2222-4222-8222-222222222222";
  // 构造已连接的视图状态
  const state: ViewState = {
    deviceId: "11111111-1111-4111-8111-111111111111", deviceName: "外观验证机",
    hosts: [{ id: source, name: "隔离展示样本", paired: true, available: true }],
    selectedId: source, status: "已连接 · 正在同步", error: null, assetWarning: null,
    pairingCode: null, pairingDeadlineMs: null, connected: true, snapshot,
    receivedAtMs: 1_720_111_000_000, live: true, cacheSourceId: source,
  };
  // 非动效模式：挂载完整应用，端口返回固定状态
  if (options.mode !== "motion") {
    // Rust 的固定投影夹具（调试构建）读取这个键，其余行为与产品一致
    await Storage.setItem("test.staticView", JSON.stringify(state));
    mount(createApp({ foreground: () => true }));
    return;
  }
  // 动效模式：用开关控制 active 与前台
  const active = signal(true), foreground = signal(true);
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
