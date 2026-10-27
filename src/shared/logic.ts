import { Presentation } from "loki-metris-mobile-display/native";
import type { PanelConfig } from "./contracts/panel";

/** 只搬运参数和公开结果；规则在所属 Rust 业务模块执行。 */
export function invoke<T>(op: string, ...args: unknown[]): T {
  return JSON.parse(Presentation.call(JSON.stringify({ op, args }))) as T;
}

export function defaultPanel(): PanelConfig {
  return invoke("shared.defaultPanel");
}

export function restorePanel(raw: string | null): PanelConfig {
  return invoke("shared.restorePanel", raw);
}
