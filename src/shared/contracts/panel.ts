/** 持久化偏好与移动宿主投影共用的校验契约，不依赖任何页面。 */
// 用量卡片详细程度：精简或详细
export type UsageDetail = "compact" | "detailed";
// Hooks 布局：每页一张或四张
export type HookLayout = "1x1" | "2x2";

// 面板配置
export type PanelConfig = {
  // 是否显示用量卡片
  showUsage: boolean;
  // 是否显示 Hooks 面板
  showHooks: boolean;
  // 用量详细程度
  usageDetail: UsageDetail;
  // Hooks 布局
  hookLayout: HookLayout;
};
