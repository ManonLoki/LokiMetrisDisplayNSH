// 引入原生规则模块
import { Rules } from "loki-metris-mobile-display/native";

// 把原始代理名归一为规范键
export function canonicalAgentKey(raw: string): string {
  // 调用规则：规范代理键
  return Rules.canonicalAgentKey(raw);
}

// 代理的展示名称
export function agentDisplayName(raw: string): string {
  // 调用规则：代理显示名
  return Rules.agentDisplayName(raw);
}
