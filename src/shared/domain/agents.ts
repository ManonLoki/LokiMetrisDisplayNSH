import { invoke } from "../native-bridge";

export function canonicalAgentKey(raw: string): string {
  return invoke("shared.canonicalAgentKey", raw);
}

export function agentDisplayName(raw: string): string {
  return invoke("shared.agentDisplayName", raw);
}
