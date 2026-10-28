// 引入挂载函数与存储
import { mount, Storage } from "nsh";
// 引入真实原生桥
import { Link, UiLifecycle } from "loki-metris-mobile-display/native";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 上一次观测到的投影
let lastProjection = "";
// 仅在投影变化时记录到存储
function observe(raw: string): string {
  if (raw !== lastProjection) {
    lastProjection = raw;
    // 仅采样真实桥的公开输出；产品入口不写这个测试键，也不读取它。
    void Storage.setItem("loki.nativeState", raw);
  }
  // 原样返回
  return raw;
}
// 包装真实 Link，观测 initialize 与 poll 的输出
const link = {
  initialize: (id: string, name: string) => observe(Link.initialize(id, name)),
  // 命令不做观测，直接转发
  command: (json: string) => Link.command(json),
  poll: () => observe(Link.poll()),
};

// 与产品使用同一真实桥和应用工厂，只增加测试观测回调。
// 与产品使用同一真实桥和应用工厂
mount(createApp({ link, foreground: () => UiLifecycle.foreground(), ...observations() }));
