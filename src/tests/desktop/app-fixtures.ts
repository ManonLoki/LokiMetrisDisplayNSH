// 引入数值与快照类型
import type { Amount, Snapshot } from "../../shared/contracts/snapshot";

// 构造一项完整、精确、可用的用量
export function amount(total: string): Amount {
  return { total, availability: "available", completeness: "complete", confidence: "exact" };
}

/** 故意包含别名、禁用 Agent、完整整数和缺失窗口，避免测试只覆盖理想快照。 */
// 构造具代表性的快照
export function representativeSnapshot(): Snapshot {
  return {
    // 协议版本
    version: 1,
    // Token 统计
    tokens: {
      // 统计日与时区
      day: { date: "2026-10-21", zone: "Asia/Shanghai" },
      // 总量
      all: amount("1500"),
      // 各代理用量，含别名与超大数值
      agents: [
        { agent: "Work_Buddy", amount: amount("7") },
        { agent: "SecretBot", amount: amount("999995") },
        { agent: "Grok Build", amount: amount("1000") },
        { agent: "Claude-Code", amount: amount("221193571") },
        { agent: "codex", amount: amount("999") },
        { agent: "Other Agent", amount: amount("42") },
      ],
    },
    // 无 Token 错误
    tokenError: null,
    // Hooks 可用
    hooksAvailable: true,
    // 五条 Hook，覆盖空闲、运行、询问、错误四种状态
    hooks: [
      // 空闲
      { agent: "codex", slot: 1, behavior: "idle", content: "空闲正文", contentTruncated: false, image: null },
      // 运行中
      { agent: "claude", slot: 2, behavior: "running", content: "运行正文", contentTruncated: false, image: null },
      // 询问中
      { agent: "cursor", slot: 3, behavior: "asking", content: "询问正文", contentTruncated: false, image: null },
      // 出错
      { agent: "grok", slot: 4, behavior: "error", content: "出错正文", contentTruncated: false, image: null },
      {
        // 带图片与截断正文的 Hook
        agent: "Work_Buddy",
        slot: 5,
        behavior: "idle",
        content: "桌面回传的正文",
        contentTruncated: true,
        image: "ab".repeat(32),
      },
    ],
    // 供应商额度，部分窗口缺失
    providerQuota: {
      observedAtUnix: 1_720_000_000,
      providers: [
        // 只有每周窗口
        { provider: "codex", plan: "plus", weekly: { kind: "weekly", usedTenths: 900 } },
        // 只有五小时窗口
        { provider: "claude", plan: "pro", fiveHour: { kind: "fiveHour", usedTenths: 100 } },
        {
          // 三种窗口齐全且无套餐
          provider: "grok",
          plan: null,
          monthly: { kind: "monthly", usedTenths: 600 },
          weekly: { kind: "weekly", usedTenths: 200 },
          fiveHour: { kind: "fiveHour", usedTenths: 950 },
        },
      ],
    },
    // 启用的代理列表
    enabledAgents: ["codex", "claudeCode", "cursor", "grok", "Other Agent", "Work_Buddy"],
  };
}

