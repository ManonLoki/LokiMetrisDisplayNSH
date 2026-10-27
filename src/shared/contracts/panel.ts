import { invoke } from "../native-bridge";

/** 持久化偏好与移动宿主投影共用的校验契约，不依赖任何页面。 */
export type UsageDetail = "compact" | "detailed";
export type HookLayout = "1x1" | "2x2";

export type PanelConfig = {
  showUsage: boolean;
  showHooks: boolean;
  usageDetail: UsageDetail;
  hookLayout: HookLayout;
};

export function defaultPanel(): PanelConfig {
  return invoke("shared.defaultPanel");
}

export function restorePanel(raw: string | null): PanelConfig {
  return invoke("shared.restorePanel", raw);
}
