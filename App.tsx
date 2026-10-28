// 从 nsh 运行时引入挂载函数，用于把根组件渲染到宿主界面
import { mount } from "nsh";
// 引入原生桥接：Link 负责连接通道，UiLifecycle 负责界面生命周期与设备形态
import { Link, UiLifecycle } from "loki-metris-mobile-display/native";
// 引入应用工厂，负责组装整个界面树
import { createApp } from "./src/app/application";

// 注入真实原生端口；foreground 查询是否处于前台；电视形态下启用遥控器导航
mount(createApp({ link: Link, foreground: () => UiLifecycle.foreground(), remoteNavigation: () => UiLifecycle.formFactor() === "tv" }));
