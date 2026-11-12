/** 用量卡片的类型来自 Rust 的 `models.rs`;用量规则和整数运算都在 Rust 中执行。 */
import type { QuotaLevel, Snapshot } from "loki-metris-mobile-display/native";

// 用量卡片使用的快照
export type UsageSnapshot = Snapshot;
export type { DisplayRow, DisplayWindow, QuotaProjection, UsageCardProjection } from "loki-metris-mobile-display/native";

/** 业务投影决定等级;组件自己持有配色。 */
// 业务层只给出等级，颜色由组件决定
export function quotaColor(level: QuotaLevel): string {
  if (level === "red") return "#FA5252";
  if (level === "yellow") return "#F5C542";
  return "#40C057";
}
