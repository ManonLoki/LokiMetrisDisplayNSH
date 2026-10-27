import type { Snapshot } from "../../shared/contracts/snapshot";

/** Hooks 只关心当前图文事实，连接活性另以布尔值传入。 */
export type HookSnapshot = Pick<Snapshot, "hooksAvailable" | "hooks">;
