// 配对窗口时长：60 秒
export const PAIRING_WINDOW_MS = 60_000;

/** 只供隔离事件测试生成可复现的码；真实配对码由原生密码核心生成。 */
// 生成可复现的四位配对码
export function makePairingCode(nowMs: number, hostId: string): string {
  // FNV-1a 初始值
  let hash = 2166136261;
  // 逐字符混入主机 ID
  for (let index = 0; index < hostId.length; index += 1) {
    hash ^= hostId.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  // 再混入时间，取四位
  const mixed = Math.abs((hash + (nowMs % 10000)) % 10000);
  // 补零到四位
  return String(mixed).padStart(4, "0");
}

// 判断配对窗口是否已过期
export function pairingExpired(nowMs: number, deadlineMs: number): boolean {
  return nowMs >= 0 && deadlineMs >= 0 && nowMs >= deadlineMs;
}


// 把毫秒时间格式化为本地 “年-月-日 时:分”
export function formatSyncTime(ms: number): string {
  const date = new Date(ms);
  // 补零到两位
  const part = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${part(date.getMonth() + 1)}-${part(date.getDate())} ${part(date.getHours())}:${part(date.getMinutes())}`;
}
