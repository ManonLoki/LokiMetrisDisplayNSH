/** 类型化的 UI 适配器;用量规则和整数运算都在 Rust 中执行。 */
// 引入原生桥接调用
import { invoke } from "../../shared/native-bridge";
// 引入快照类型
import type { Snapshot } from "../../shared/contracts/snapshot";

// 用量卡片只关心快照中的这四个字段
export type UsageSnapshot = Pick<Snapshot, "tokens" | "tokenError" | "providerQuota" | "enabledAgents">;
// 单个代理的展示行
export type DisplayRow = {
  // 代理名称
  name: string;
  // 用量文本
  tokenText: string | null;
  // 紧凑格式的用量
  compact: string | null;
  // 精确用量
  exact: string | null;
  // 套餐名称
  plan: string | null;
  // 额度窗口：标签、百分比、等级与剩余千分比
  windows: { kindLabel: string; percent: string; level: "red" | "yellow" | "green"; remainingTenths: number }[];
};
// 单个供应商的额度投影
export type QuotaProjection = { provider: string; name: string; plan: string | null; windows: DisplayRow["windows"] };
// 用量卡片的完整投影
export type UsageCardProjection = {
  // 是否有 Token 数据
  hasTokens: boolean;
  // 是否有精确值
  hasExact: boolean;
  // 统计日期
  date: string | null;
  // 设备时区
  zone: string | null;
  // 总量文本
  total: string;
  // 精确总量文本
  exact: string | null;
  // 无障碍朗读用文本
  amountAccessibility: string | null;
  // 数据缺失时的提示
  missingText: string;
  // 供应商额度列表
  providers: QuotaProjection[];
  // 各代理明细行
  rows: DisplayRow[];
  // 额度观测时间（Unix 秒）
  quotaObservedAtUnix: number;
};

// 把快照投影成卡片所需数据，由 Rust 计算
export function cardProjection(snapshot: UsageSnapshot | null): UsageCardProjection { return invoke("usage.cardProjection", snapshot); }

/** 业务投影决定等级;组件自己持有配色。 */
// 业务层只给出等级，颜色由组件决定
export function quotaColor(level: "red" | "yellow" | "green"): string {
  // 红色：额度紧张
  if (level === "red") return "#FA5252";
  // 黄色：额度偏低
  if (level === "yellow") return "#F5C542";
  // 绿色：额度充足
  return "#40C057";
}
