/** Typed UI adapters; usage rules and integer arithmetic run in Rust. */
import { invoke } from "../../shared/native-bridge";
import type { Snapshot } from "../../shared/contracts/snapshot";

export type UsageSnapshot = Pick<Snapshot, "tokens" | "tokenError" | "providerQuota" | "enabledAgents">;
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

export function cardProjection(snapshot: UsageSnapshot | null): UsageCardProjection { return invoke("usage.cardProjection", snapshot); }

/** The business projection chooses the level; the component owns its palette. */
export function quotaColor(level: "red" | "yellow" | "green"): string {
  if (level === "red") return "#FA5252";
  if (level === "yellow") return "#F5C542";
  return "#40C057";
}
