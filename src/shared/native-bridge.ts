// 引入原生规则模块
import { Rules } from "loki-metris-mobile-display/native";

/** 只搬运参数和公开结果；规则在所属 Rust 业务模块执行。 */
// 序列化操作名与参数，调用原生规则，再解析返回的 JSON
export function invoke<T>(op: string, ...args: unknown[]): T {
  // 调用原生规则并解析结果
  return JSON.parse(Rules.call(JSON.stringify({ op, args }))) as T;
}
