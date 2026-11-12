import { mount } from "nsh";
import { createApp } from "../../app/application";
import { observations } from "./support/observations";

// 桌面端由 Rust 夹具（调试构建，读取 loki.inject 等存储键）扮演；应用本身和产品一致。
mount(createApp({ ...observations(), foreground: () => false }));
