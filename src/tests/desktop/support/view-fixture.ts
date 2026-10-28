/** UI 场景替身，不执行密码协议；真实协议由 Rust 与实网专项验证。 */
// 引入主机、配对记录与视图类型
import type { Host, PairingRecord, ViewState } from "../../../features/connection";
// 引入快照类型
import type { Snapshot } from "../../../shared/contracts/snapshot";
// 引入配对规则
import { makePairingCode, PAIRING_WINDOW_MS, pairingExpired } from "./fixture-rules";

// 合并发现列表与配对记录得到主机列表
export function hostsFrom(discovered: { id: string; name: string }[], paired: Record<string, PairingRecord>): Host[] {
  // 已发现的主机，可用，按是否有配对记录标记
  const found = discovered.map((host) => ({
    id: host.id,
    name: host.name,
    paired: paired[host.id] != null,
    available: true,
  }));
  // 已配对但未发现的主机，标记为不可用
  const missing = Object.keys(paired)
    .filter((id) => !discovered.some((host) => host.id === id))
    .sort()
    .map((id) => ({ id, name: `已配对桌面 · ${id.slice(0, 8)}`, paired: true, available: false }));
  return [...found, ...missing];
}

/** 用户点选后才进入配对。已有凭据则直接等快照，不显示四位码。 */
// 选择桌面：已有配对则等待认证，否则生成配对码
export function selectDesktop(state: ViewState, id: string, nowMs: number): ViewState {
  const host = state.hosts.find((item) => item.id === id);
  // 不可用的主机忽略
  if (!host || !host.available) return state;
  // 已配对：直接等待快照，不显示配对码
  if (state.paired[id]) {
    return {
      ...state,
      selectedId: id,
      pairingCode: null,
      pairingDeadlineMs: null,
      connected: false,
      error: null,
      // 状态：正在认证
      status: "正在认证桌面…",
    };
  }
  // 返回新的视图状态
  return {
    // 沿用原状态
    ...state,
    // 选中该设备
    selectedId: id,
    // 生成配对码
    pairingCode: makePairingCode(nowMs, id),
    // 配对期限
    pairingDeadlineMs: nowMs + PAIRING_WINDOW_MS,
    // 未连接
    connected: false,
    // 无错误
    error: null,
    // 未配对：生成配对码并开始计时
    status: "等待在桌面输入手机配对码",
  };
}

// 配对窗口过期则清除配对码并报错
export function expirePairing(state: ViewState, nowMs: number): ViewState {
  if (state.pairingDeadlineMs == null || !pairingExpired(nowMs, state.pairingDeadlineMs)) return state;
  return {
    ...state,
    pairingCode: null,
    pairingDeadlineMs: null,
    connected: false,
    error: "配对窗口已过期，请重新选择桌面。",
    // 状态：暂未连接
    status: "暂未连接",
  };
}

/** 握手成功只保存配对材料，不算已连接。 */
// 握手成功：保存配对，等待认证
export function acceptHandshake(state: ViewState, nowMs: number): ViewState {
  // 缺少选择或配对码则忽略
  if (state.selectedId == null || state.pairingCode == null || state.pairingDeadlineMs == null) return state;
  // 已过期则按过期处理
  if (pairingExpired(nowMs, state.pairingDeadlineMs)) return expirePairing(state, nowMs);
  const id = state.selectedId;
  return {
    ...state,
    pairingCode: null,
    pairingDeadlineMs: null,
    connected: false,
    // 记录配对
    paired: { ...state.paired, [id]: { clientId: id, savedAtMs: nowMs } },
    // 标记该主机已配对
    hosts: state.hosts.map((host) => (host.id === id ? { ...host, paired: true } : host)),
    error: null,
    status: "正在认证桌面…",
  };
}

/** 第一份真实快照才标记已连接，并保留事件里的接收时间。 */
// 收到首份快照才标记已连接
export function acceptSnapshot(state: ViewState, snapshot: Snapshot, receivedAtMs: number): ViewState {
  // 未选择或未配对则忽略
  if (state.selectedId == null || state.paired[state.selectedId] == null) return state;
  // 仍在配对中则忽略
  if (state.pairingCode != null) return state;
  return {
    ...state,
    snapshot,
    receivedAtMs,
    cacheSourceId: state.selectedId,
    connected: true,
    live: true,
    // 无错误
    error: null,
    // 状态：已连接
    status: "已连接，正在同步",
  };
}

// 忘记桌面：移除配对，必要时清空展示内容
export function forgetDesktop(state: ViewState, id: string): ViewState {
  const paired = { ...state.paired };
  delete paired[id];
  // 被忘记的桌面正在展示时需要清空
  const clearDisplay = state.selectedId === id || state.cacheSourceId === id;
  return {
    ...state,
    paired,
    // 重建主机列表
    hosts: hostsFrom(
      state.hosts.filter((host) => host.available).map((host) => ({ id: host.id, name: host.name })),
      paired,
    ),
    // 取消选择
    selectedId: state.selectedId === id ? null : state.selectedId,
    pairingCode: state.selectedId === id ? null : state.pairingCode,
    pairingDeadlineMs: state.selectedId === id ? null : state.pairingDeadlineMs,
    connected: clearDisplay ? false : state.connected,
    snapshot: clearDisplay ? null : state.snapshot,
    receivedAtMs: clearDisplay ? null : state.receivedAtMs,
    live: clearDisplay ? false : state.live,
    cacheSourceId: clearDisplay ? null : state.cacheSourceId,
    // 无错误
    error: null,
    // 状态：已忘记
    status: "已忘记，可重新配对",
  };
}

