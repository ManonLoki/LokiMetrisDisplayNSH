/** 原生连接公开投影；展示层不会接触真实配对密钥。 */
// 引入原生规则模块
import { Rules } from "loki-metris-mobile-display/native";
// 引入面板配置类型
import type { PanelConfig } from "../../shared/contracts/panel";
// 引入快照类型
import type { Snapshot } from "../../shared/contracts/snapshot";

// 一台可发现的桌面主机
export type Host = { id: string; name: string; paired: boolean; available: boolean };

// 配对记录：客户端 ID 与保存时间（毫秒）
export type PairingRecord = { clientId: string; savedAtMs: number };

// 连接视图的完整状态
export type ViewState = {
  // 桌面同步下来的面板配置，可能缺省
  panelConfig?: PanelConfig;
  // 本机设备 ID
  deviceId: string;
  // 本机设备名称
  deviceName: string;
  // 已发现或已配对的桌面列表
  hosts: Host[];
  // 当前选中的桌面 ID
  selectedId: string | null;
  // 状态文案
  status: string;
  // 错误文案
  error: string | null;
  // 资源告警文案
  assetWarning?: string | null;
  // 配对码，未处于配对流程时为 null
  pairingCode: string | null;
  // 配对码截止时间（毫秒时间戳）
  pairingDeadlineMs: number | null;
  // 是否已连接
  connected: boolean;
  // 最近一次快照
  snapshot: Snapshot | null;
  // 快照接收时间（毫秒时间戳）
  receivedAtMs: number | null;
  /** 当前这次会话收到过快照。缓存回放为 false。 */
  // 是否实时
  live: boolean;
  // 缓存快照来源的桌面 ID
  cacheSourceId: string | null;
  // 已配对记录，以客户端 ID 为键
  paired: Record<string, PairingRecord>;
};

// 构造指定设备的空视图
export function emptyView(deviceId: string, deviceName: string): ViewState {
  // 空状态由原生侧生成，保证与 Rust 模型一致
  return Rules.connectionEmptyView(deviceId, deviceName) as ViewState;
}

// 把原生端口返回的 JSON 文本解析为视图
export function nativeView(raw: string): ViewState {
  // 解析交给原生侧
  return Rules.connectionNativeView(raw) as ViewState;
}
