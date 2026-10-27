import { mount, Storage } from "nsh";
import { Link, UiLifecycle } from "loki-metris-mobile-display/native";
import { createApp } from "../../app/application";
import { observations } from "./support/observations";

let lastProjection = "";
function observe(raw: string): string {
  if (raw !== lastProjection) {
    lastProjection = raw;
    // 仅采样真实桥的公开输出；产品入口不写这个测试键，也不读取它。
    void Storage.setItem("loki.nativeState", raw);
  }
  return raw;
}
const link = {
  initialize: (id: string, name: string) => observe(Link.initialize(id, name)),
  command: (json: string) => Link.command(json),
  poll: () => observe(Link.poll()),
};

// 与产品使用同一真实桥和应用工厂，只增加测试观测回调。
mount(createApp({ link, foreground: () => UiLifecycle.foreground(), ...observations() }));
