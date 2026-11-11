// 引入挂载函数
import { mount } from "nsh";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 桌面端由 Rust 夹具（调试构建，读取 loki.inject 等存储键）扮演；应用本身和产品一致。
mount(createApp({ ...observations(), foreground: () => false }));
