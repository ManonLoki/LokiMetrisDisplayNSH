export const PAIRING_WINDOW_MS = 60_000;

/** 只供隔离事件测试生成可复现的码；真实配对码由原生密码核心生成。 */
export function makePairingCode(nowMs: number, hostId: string): string {
  let hash = 2166136261;
  for (let index = 0; index < hostId.length; index += 1) {
    hash ^= hostId.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  const mixed = Math.abs((hash + (nowMs % 10000)) % 10000);
  return String(mixed).padStart(4, "0");
}

export function pairingExpired(nowMs: number, deadlineMs: number): boolean {
  return nowMs >= 0 && deadlineMs >= 0 && nowMs >= deadlineMs;
}


export function formatSyncTime(ms: number): string {
  const date = new Date(ms);
  const part = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${part(date.getMonth() + 1)}-${part(date.getDate())} ${part(date.getHours())}:${part(date.getMinutes())}`;
}
