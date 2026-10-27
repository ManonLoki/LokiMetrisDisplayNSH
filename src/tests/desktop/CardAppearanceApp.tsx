import { effect, mount, reduceMotion, signal, Storage, Text, View } from "nsh";
import { Button } from "nsh/base";
import { createApp } from "../../app/application";
import type { ViewState } from "../../features/connection";
import { UsageCard } from "../../features/usage";
import { CyberMotionScope } from "../../shared/ui/effects";
import { cyberColors as color } from "../../shared/ui/theme";
import { representativeSnapshot } from "./app-fixtures";

// Explicit UI-only fixture. No Link, lifecycle owner, network or saved pairing is opened.
void (async () => {
  const options = JSON.parse((await Storage.getItem("test.cardAppearance")) ?? "{}");
  const snapshot = representativeSnapshot();
  const source = "22222222-2222-4222-8222-222222222222";
  const config = { showUsage: true, showHooks: true, usageDetail: "detailed" as const, hookLayout: "2x2" as const };
  const state: ViewState = {
    deviceId: "11111111-1111-4111-8111-111111111111", deviceName: "外观验证机",
    hosts: [{ id: source, name: "隔离展示样本", paired: true, available: true }],
    selectedId: source, status: "已连接 · 正在同步", error: null, assetWarning: null,
    pairingCode: null, pairingDeadlineMs: null, connected: true, snapshot,
    receivedAtMs: 1_720_111_000_000, live: true, cacheSourceId: source,
    paired: { [source]: { clientId: source, savedAtMs: 1_720_111_000_000 } }, panelConfig: config,
  };
  if (options.mode !== "motion") {
    mount(createApp({ foreground: () => true, link: {
      initialize: () => JSON.stringify(state), poll: () => JSON.stringify(state), command: () => {},
    } }));
    return;
  }
  const active = signal(true), foreground = signal(true);
  const rect = { x: 24, y: 72, width: 472, height: 520 };
  await Storage.setItem("test.appearanceRect", JSON.stringify(rect));
  mount(() => {
    effect(() => {
      void Storage.setItem("test.appearanceGates", JSON.stringify({ active: active(), foreground: foreground(), reducedMotion: reduceMotion() }));
    });
    return <View testID="appearance-root" style={{ flex: 1, backgroundColor: color.canvas }}>
      <View style={{ position: "absolute", left: 24, top: 12, height: 44, width: 472, flexDirection: "row", gap: 12 }}>
        <Button testID="appearance-toggle-active" onPress={() => active.set(!active())} style={{ flex: 1, padding: 10, backgroundColor: color.surface }}>
          <Text style={{ color: color.ink }}>active: {() => String(active())}</Text>
        </Button>
        <Button testID="appearance-toggle-foreground" onPress={() => foreground.set(!foreground())} style={{ flex: 1, padding: 10, backgroundColor: color.surface }}>
          <Text style={{ color: color.ink }}>foreground: {() => String(foreground())}</Text>
        </Button>
      </View>
      <CyberMotionScope foreground={foreground}>
        <View testID="appearance-card-frame" style={{ position: "absolute", left: rect.x, top: rect.y, width: rect.width, height: rect.height }}>
          <UsageCard width={() => rect.width} height={() => rect.height} snapshot={() => snapshot}
            detail={() => "compact"} active={active} />
        </View>
      </CyberMotionScope>
    </View>;
  });
})();
