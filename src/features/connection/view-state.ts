/** 原生连接公开投影；展示层不会接触真实配对密钥。 */
import { invoke } from "../../shared/native-bridge";
import type { PanelConfig } from "../../shared/contracts/panel";
import type { Snapshot } from "../../shared/contracts/snapshot";

export type Host = { id: string; name: string; paired: boolean; available: boolean };

export type PairingRecord = { clientId: string; savedAtMs: number };

export type ViewState = {
  panelConfig?: PanelConfig;
  deviceId: string;
  deviceName: string;
  hosts: Host[];
  selectedId: string | null;
  status: string;
  error: string | null;
  assetWarning?: string | null;
  pairingCode: string | null;
  pairingDeadlineMs: number | null;
  connected: boolean;
  snapshot: Snapshot | null;
  receivedAtMs: number | null;
  /** 当前这次会话收到过快照。缓存回放为 false。 */
  live: boolean;
  cacheSourceId: string | null;
  paired: Record<string, PairingRecord>;
};

export function emptyView(deviceId: string, deviceName: string): ViewState {
  return invoke("connection.emptyView", deviceId, deviceName);
}

export function nativeView(raw: string): ViewState {
  return invoke("connection.nativeView", raw);
}
