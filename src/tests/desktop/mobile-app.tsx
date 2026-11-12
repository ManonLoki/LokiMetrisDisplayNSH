import { mount, Storage } from "nsh";
import { createApp } from "../../app/application";
import { observations } from "./support/observations";

// 隔离的公开连接：固定投影由 Rust 夹具（调试构建，读取 test.staticView）返回，
// 它还把收到的命令写入 test.connectionCommands。生产环境绝不读取这些夹具键。
void (async () => {
  const fixture = JSON.parse((await Storage.getItem("test.mobileApp"))!);
  await Storage.setItem("test.staticView", JSON.stringify(fixture.projection));
  mount(createApp({
    ...observations(),
    foreground: () => true,
    // 仅电视形态启用遥控
    remoteNavigation: () => fixture.formFactor === "tv",
  }));
})();
