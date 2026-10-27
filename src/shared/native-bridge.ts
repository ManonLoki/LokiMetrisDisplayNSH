import { Rules } from "loki-metris-mobile-display/native";

/** 只搬运参数和公开结果；规则在所属 Rust 业务模块执行。 */
export function invoke<T>(op: string, ...args: unknown[]): T {
  return JSON.parse(Rules.call(JSON.stringify({ op, args }))) as T;
}
