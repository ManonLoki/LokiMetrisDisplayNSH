import { open as openApp, type OpenOptions } from "nsh/test";

export const PHONE = "11111111-1111-4111-8111-111111111111";
export const DESKTOP = "22222222-2222-4222-8222-222222222222";
export const T0 = 1_700_000_000_000;
export const RECEIVED = 1_720_111_000_000;
export const KEY_PANEL = "loki.panel";
export const KEY_DEVICE = "loki.device";
export const KEY_PAIRED = "loki.paired";
export const KEY_CACHE = "loki.cache";
export const KEY_INJECT = "loki.inject";
export const KEY_FRAMES = "loki.frames";

export const safe = { top: 0, right: 0, bottom: 0, left: 0 };

// 每次打开都显式隔离网络和配对存储，避免 UI 回归接触用户已保存的真实设备。
export function open(entry: string, options: OpenOptions = {}) {
  return openApp(entry, { ...options, storage: { [KEY_INJECT]: JSON.stringify({ discovered: [], events: [] }), ...options.storage } });
}

