/** UI 场景替身，不执行密码协议；真实协议由 Rust 与实网专项验证。 */
import type { Host, PairingRecord, ViewState } from "../../../features/connection";
import type { Snapshot } from "../../../shared/contracts/snapshot";
import { makePairingCode, PAIRING_WINDOW_MS, pairingExpired } from "./fixture-rules";

export function hostsFrom(discovered: { id: string; name: string }[], paired: Record<string, PairingRecord>): Host[] {
  const found = discovered.map((host) => ({
    id: host.id,
    name: host.name,
    paired: paired[host.id] != null,
    available: true,
  }));
  const missing = Object.keys(paired)
    .filter((id) => !discovered.some((host) => host.id === id))
    .sort()
    .map((id) => ({ id, name: `已配对桌面 · ${id.slice(0, 8)}`, paired: true, available: false }));
  return [...found, ...missing];
}

/** 用户点选后才进入配对。已有凭据则直接等快照，不显示四位码。 */
export function selectDesktop(state: ViewState, id: string, nowMs: number): ViewState {
  const host = state.hosts.find((item) => item.id === id);
  if (!host || !host.available) return state;
  if (state.paired[id]) {
    return {
      ...state,
      selectedId: id,
      pairingCode: null,
      pairingDeadlineMs: null,
      connected: false,
      error: null,
      status: "正在认证桌面…",
    };
  }
  return {
    ...state,
    selectedId: id,
    pairingCode: makePairingCode(nowMs, id),
    pairingDeadlineMs: nowMs + PAIRING_WINDOW_MS,
    connected: false,
    error: null,
    status: "等待在桌面输入手机配对码",
  };
}

export function expirePairing(state: ViewState, nowMs: number): ViewState {
  if (state.pairingDeadlineMs == null || !pairingExpired(nowMs, state.pairingDeadlineMs)) return state;
  return {
    ...state,
    pairingCode: null,
    pairingDeadlineMs: null,
    connected: false,
    error: "配对窗口已过期，请重新选择桌面。",
    status: "暂未连接",
  };
}

/** 握手成功只保存配对材料，不算已连接。 */
export function acceptHandshake(state: ViewState, nowMs: number): ViewState {
  if (state.selectedId == null || state.pairingCode == null || state.pairingDeadlineMs == null) return state;
  if (pairingExpired(nowMs, state.pairingDeadlineMs)) return expirePairing(state, nowMs);
  const id = state.selectedId;
  return {
    ...state,
    pairingCode: null,
    pairingDeadlineMs: null,
    connected: false,
    paired: { ...state.paired, [id]: { clientId: id, savedAtMs: nowMs } },
    hosts: state.hosts.map((host) => (host.id === id ? { ...host, paired: true } : host)),
    error: null,
    status: "正在认证桌面…",
  };
}

/** 第一份真实快照才标记已连接，并保留事件里的接收时间。 */
export function acceptSnapshot(state: ViewState, snapshot: Snapshot, receivedAtMs: number): ViewState {
  if (state.selectedId == null || state.paired[state.selectedId] == null) return state;
  if (state.pairingCode != null) return state;
  return {
    ...state,
    snapshot,
    receivedAtMs,
    cacheSourceId: state.selectedId,
    connected: true,
    live: true,
    error: null,
    status: "已连接，正在同步",
  };
}

export function forgetDesktop(state: ViewState, id: string): ViewState {
  const paired = { ...state.paired };
  delete paired[id];
  const clearDisplay = state.selectedId === id || state.cacheSourceId === id;
  return {
    ...state,
    paired,
    hosts: hostsFrom(
      state.hosts.filter((host) => host.available).map((host) => ({ id: host.id, name: host.name })),
      paired,
    ),
    selectedId: state.selectedId === id ? null : state.selectedId,
    pairingCode: state.selectedId === id ? null : state.pairingCode,
    pairingDeadlineMs: state.selectedId === id ? null : state.pairingDeadlineMs,
    connected: clearDisplay ? false : state.connected,
    snapshot: clearDisplay ? null : state.snapshot,
    receivedAtMs: clearDisplay ? null : state.receivedAtMs,
    live: clearDisplay ? false : state.live,
    cacheSourceId: clearDisplay ? null : state.cacheSourceId,
    error: null,
    status: "已忘记，可重新配对",
  };
}

