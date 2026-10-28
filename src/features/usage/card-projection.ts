/** 用量卡片的类型来自 Rust 的 `models.rs`;用量规则和整数运算都在 Rust 中执行。 */
// 引入快照类型
import type { QuotaLevel, Snapshot } from "loki-metris-mobile-display/native";

// 用量卡片使用的快照
export type UsageSnapshot = Snapshot;
// 重新导出 Rust 生成的卡片投影类型
export type { DisplayRow, DisplayWindow, QuotaProjection, UsageCardProjection } from "loki-metris-mobile-display/native";

/** 业务投影决定等级;组件自己持有配色。 */
// 业务层只给出等级，颜色由组件决定
export function quotaColor(level: QuotaLevel): string {
  // 红色：额度紧张
  if (level === "red") return "#FA5252";
  // 黄色：额度偏低
  if (level === "yellow") return "#F5C542";
  // 绿色：额度充足
  return "#40C057";
}
