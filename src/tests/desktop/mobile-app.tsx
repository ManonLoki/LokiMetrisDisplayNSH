// 引入挂载函数与存储
import { mount, Storage } from "nsh";
// 引入应用工厂
import { createApp } from "../../app/application";
// 引入测试观测器
import { observations } from "./support/observations";

// 隔离的公开连接端口。生产环境绝不读取这些夹具键。
// 读取夹具后挂载
void (async () => {
  // 读取夹具
  const fixture = JSON.parse((await Storage.getItem("test.mobileApp"))!);
  // 固定的连接投影
  const projection = fixture.projection;
  // 记录收到的命令
  const commands: unknown[] = [];
  mount(createApp({
    // 注入测试观测回调
    ...observations(),
    // 始终视为前台
    foreground: () => true,
    // 仅电视形态启用遥控
    remoteNavigation: () => fixture.formFactor === "tv",
    // 隔离的端口，只返回固定投影
    link: {
      initialize: () => JSON.stringify(projection),
      poll: () => JSON.stringify(projection),
      // 记录命令，重命名时同步修改投影
      command: (json) => {
        const command = JSON.parse(json);
        commands.push(command);
        if (command.op === "rename") projection.deviceName = command.name;
        // 把命令序列写入存储供断言
        void Storage.setItem("test.connectionCommands", JSON.stringify(commands));
      },
    },
  }));
})();
