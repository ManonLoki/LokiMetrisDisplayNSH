// 引入挂载函数与存储
import { mount, Storage } from "nsh";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 隔离的公开连接：固定投影由 Rust 夹具（调试构建，读取 test.staticView）返回，
// 它还把收到的命令写入 test.connectionCommands。生产环境绝不读取这些夹具键。
// 读取夹具后挂载
void (async () => {
  // 读取夹具
  const fixture = JSON.parse((await Storage.getItem("test.mobileApp"))!);
  // 固定的连接投影
  await Storage.setItem("test.staticView", JSON.stringify(fixture.projection));
  mount(createApp({
    // 注入测试观测回调
    ...observations(),
    // 始终视为前台
    foreground: () => true,
    // 仅电视形态启用遥控
    remoteNavigation: () => fixture.formFactor === "tv",
  }));
})();
