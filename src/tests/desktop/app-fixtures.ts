import type { Amount, Snapshot } from "../../shared/contracts/snapshot";

export function amount(total: string): Amount {
  return { total, availability: "available", completeness: "complete", confidence: "exact" };
}

/** 故意包含别名、禁用 Agent、完整整数和缺失窗口，避免测试只覆盖理想快照。 */
export function representativeSnapshot(): Snapshot {
  return {
    version: 1,
    tokens: {
      day: { date: "2026-10-21", zone: "Asia/Shanghai" },
      all: amount("1500"),
      agents: [
        { agent: "Work_Buddy", amount: amount("7") },
        { agent: "SecretBot", amount: amount("999995") },
        { agent: "Grok Build", amount: amount("1000") },
        { agent: "Claude-Code", amount: amount("221193571") },
        { agent: "codex", amount: amount("999") },
        { agent: "Other Agent", amount: amount("42") },
      ],
    },
    tokenError: null,
    hooksAvailable: true,
    hooks: [
      { agent: "codex", slot: 1, behavior: "idle", content: "空闲正文", contentTruncated: false, image: null },
      { agent: "claude", slot: 2, behavior: "running", content: "运行正文", contentTruncated: false, image: null },
      { agent: "cursor", slot: 3, behavior: "asking", content: "询问正文", contentTruncated: false, image: null },
      { agent: "grok", slot: 4, behavior: "error", content: "出错正文", contentTruncated: false, image: null },
      {
        agent: "Work_Buddy",
        slot: 5,
        behavior: "idle",
        content: "桌面回传的正文",
        contentTruncated: true,
        image: "ab".repeat(32),
      },
    ],
    providerQuota: {
      observedAtUnix: 1_720_000_000,
      providers: [
        { provider: "codex", plan: "plus", weekly: { kind: "weekly", usedTenths: 900 } },
        { provider: "claude", plan: "pro", fiveHour: { kind: "fiveHour", usedTenths: 100 } },
        {
          provider: "grok",
          plan: null,
          monthly: { kind: "monthly", usedTenths: 600 },
          weekly: { kind: "weekly", usedTenths: 200 },
          fiveHour: { kind: "fiveHour", usedTenths: 950 },
        },
      ],
    },
    enabledAgents: ["codex", "claudeCode", "cursor", "grok", "Other Agent", "Work_Buddy"],
  };
}

