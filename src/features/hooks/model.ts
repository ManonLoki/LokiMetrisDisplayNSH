// 引入快照类型
import type { Snapshot } from "../../shared/contracts/snapshot";

/** Hooks 只关心当前图文事实，连接活性另以布尔值传入。 */
// 只取快照中与 Hooks 相关的两个字段
export type HookSnapshot = Snapshot;
