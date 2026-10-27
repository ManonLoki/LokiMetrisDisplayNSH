/** Typed UI adapters; usage rules and integer arithmetic run in Rust. */
import { invoke } from "../../shared/logic";
import type { Amount, QuotaProvider, QuotaWindow, Snapshot } from "../../shared/contracts/snapshot";

export type UsageSnapshot = Pick<Snapshot, "tokens" | "tokenError" | "providerQuota" | "enabledAgents">;
export type UsageRowRef = { tokenIndex: number; quotaIndex: number };
export type DisplayRow = {
  name: string;
  tokenText: string | null;
  compact: string | null;
  exact: string | null;
  plan: string | null;
  windows: { kindLabel: string; percent: string; level: "red" | "yellow" | "green"; remainingTenths: number }[];
};
export type QuotaProjection = { provider: string; name: string; plan: string | null; windows: DisplayRow["windows"] };
export type UsageCardProjection = {
  hasTokens: boolean;
  hasExact: boolean;
  date: string | null;
  zone: string | null;
  total: string;
  exact: string | null;
  amountAccessibility: string | null;
  missingText: string;
  providers: QuotaProjection[];
  rows: DisplayRow[];
  quotaObservedAtUnix: number;
};

export function tokenCompact(value: string): string { return invoke("usage.tokenCompact", value); }
export function tokenExact(value: string): string { return invoke("usage.tokenExact", value); }
export function amountLabel(amount: Amount): string { return invoke("usage.amountLabel", amount); }
export function hasNumericValue(amount: Amount): boolean { return invoke("usage.hasNumericValue", amount); }
export function providerQuotaKey(agent: string): string | null { return invoke("usage.providerQuotaKey", agent); }
export function isEnabled(agent: string, enabledAgents: string[] | null): boolean {
  return invoke("usage.isEnabled", agent, enabledAgents);
}
export function displayRank(agent: string): number { return invoke("usage.displayRank", agent); }
export function visibleIndices(names: string[], enabledAgents: string[] | null): number[] {
  return invoke("usage.visibleIndices", names, enabledAgents);
}
export function projectRows(tokensPresent: boolean, tokenNames: string[], quotaProviders: string[], enabledAgents: string[] | null): UsageRowRef[] {
  return invoke("usage.projectRows", tokensPresent, tokenNames, quotaProviders, enabledAgents);
}
export function tokenText(sourceName: string, amountText: string | null, hasNumericValue: boolean): string | null {
  return invoke("usage.tokenText", sourceName, amountText, hasNumericValue);
}
export function quotaLabel(provider: string): string { return invoke("usage.quotaLabel", provider); }
export function planLabel(plan: string | null): string | null { return invoke("usage.planLabel", plan); }
export function quotaWindows(provider: QuotaProvider): QuotaWindow[] { return invoke("usage.quotaWindows", provider); }
export function kindLabel(kind: QuotaWindow["kind"]): string { return invoke("usage.kindLabel", kind); }
export function percentText(usedTenths: number): string { return invoke("usage.percentText", usedTenths); }
export function quotaLevel(usedTenths: number): "red" | "yellow" | "green" { return invoke("usage.quotaLevel", usedTenths); }
export function displayRows(snapshot: UsageSnapshot): DisplayRow[] { return invoke("usage.displayRows", snapshot); }
export function cardProjection(snapshot: UsageSnapshot | null): UsageCardProjection { return invoke("usage.cardProjection", snapshot); }

/** The business projection chooses the level; the component owns its palette. */
export function quotaColor(level: "red" | "yellow" | "green"): string {
  if (level === "red") return "#FA5252";
  if (level === "yellow") return "#F5C542";
  return "#40C057";
}
