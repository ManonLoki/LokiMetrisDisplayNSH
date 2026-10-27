import { invoke } from "../logic";

export function normalizeComparisonText(value: string): string {
  return invoke("shared.normalizeComparisonText", value);
}

export function canonicalAgentKey(raw: string): string {
  return invoke("shared.canonicalAgentKey", raw);
}

export function agentDisplayName(raw: string): string {
  return invoke("shared.agentDisplayName", raw);
}
