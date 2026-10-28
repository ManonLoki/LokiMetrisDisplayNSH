// 引入挂载函数
import { mount } from "nsh";
// 引入原生生命周期
import { UiLifecycle } from "loki-metris-mobile-display/native";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入假端口
import { FakeLink } from "./support/fake-link";
// 引入测试观测器
import { observations } from "./support/observations";

// 载入假端口后挂载应用
void FakeLink.load().then((link) => {
  mount(createApp({ link, foreground: () => UiLifecycle.foreground(), ...observations() }));
});
