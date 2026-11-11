// 引入挂载函数
import { mount } from "nsh";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 与产品使用同一真实 Shell 和应用工厂（不设任何夹具键，连接就是真实的），只增加布局和图片观测回调。
mount(createApp(observations()));
