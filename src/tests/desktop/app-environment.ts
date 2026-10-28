// 引入 nsh 测试的打开函数与选项类型
import { open as openApp, type OpenOptions } from "nsh/test";

// 手机端设备 ID
export const PHONE = "11111111-1111-4111-8111-111111111111";
// 桌面端设备 ID
export const DESKTOP = "22222222-2222-4222-8222-222222222222";
// 测试基准时间（毫秒）
export const T0 = 1_700_000_000_000;
// 快照接收时间（毫秒）
export const RECEIVED = 1_720_111_000_000;
// 面板配置的存储键
export const KEY_PANEL = "loki.panel";
// 设备身份的存储键
export const KEY_DEVICE = "loki.device";
// 配对记录的存储键
export const KEY_PAIRED = "loki.paired";
// 快照缓存的存储键
export const KEY_CACHE = "loki.cache";
// 测试注入夹具的存储键
export const KEY_INJECT = "loki.inject";
// 布局边界观测的存储键
export const KEY_FRAMES = "loki.frames";

// 无安全区的默认边距
export const safe = { top: 0, right: 0, bottom: 0, left: 0 };

// 每次打开都显式隔离网络和配对存储，避免 UI 回归接触用户已保存的真实设备。
// 打开应用时默认注入空的发现与事件夹具，调用方可覆盖
export function open(entry: string, options: OpenOptions = {}) {
  return openApp(entry, { ...options, storage: { [KEY_INJECT]: JSON.stringify({ discovered: [], events: [] }), ...options.storage } });
}

