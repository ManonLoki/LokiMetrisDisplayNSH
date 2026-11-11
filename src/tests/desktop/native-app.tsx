// 引入挂载函数
import { mount } from "nsh";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 真实连接与渲染照常运行。截图测试固定前台状态，避免宿主窗口失焦暂停 GIF。
mount(createApp({ ...observations(), foreground: () => true }));
