// 引入原生桥接调用
import { invoke } from "../native-bridge";

// 把原始代理名归一为规范键
export function canonicalAgentKey(raw: string): string {
  // 调用规则：规范代理键
  return invoke("shared.canonicalAgentKey", raw);
}

// 代理的展示名称
export function agentDisplayName(raw: string): string {
  // 调用规则：代理显示名
  return invoke("shared.agentDisplayName", raw);
}
