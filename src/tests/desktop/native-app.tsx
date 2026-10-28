// 引入挂载函数与存储
import { mount, Storage } from "nsh";
// 引入界面生命周期
import { UiLifecycle } from "loki-metris-mobile-display/native";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 上一次观测到的投影
let lastProjection = "";

// 与产品使用同一真实 Shell 和应用工厂，只增加测试观测回调。
mount(createApp({
  foreground: () => UiLifecycle.foreground(),
  ...observations(),
  // 仅采样真实连接交付的公开视图；产品入口不写这个测试键，也不读取它。
  onView(view) {
    const raw = JSON.stringify(view);
    if (raw === lastProjection) return;
    lastProjection = raw;
    void Storage.setItem("loki.nativeState", raw);
  },
}));
