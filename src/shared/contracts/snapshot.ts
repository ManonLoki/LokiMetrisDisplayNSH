/** 原生投影共用的数据契约；仅公开展示事实，不包含凭据或界面状态。 */
// 一项用量数值及其可信度
export type Amount = {
  // 总量文本
  total: string | null;
  // 可用性：有数据、无记录、不可用
  availability: "available" | "noRecords" | "unavailable";
  // 完整度：完整、部分、未知
  completeness: "complete" | "partial" | "unknown";
  // 置信度：精确、推算、疑似
  confidence: "exact" | "derived" | "suspected";
};

// 额度窗口：周期种类与已用千分比
export type QuotaWindow = { kind: "monthly" | "weekly" | "fiveHour"; usedTenths: number };

// 某供应商的额度
export type QuotaProvider = {
  // 供应商标识
  provider: string;
  // 套餐名称
  plan: string | null;
  // 五小时窗口
  fiveHour?: QuotaWindow | null;
  // 每周窗口
  weekly?: QuotaWindow | null;
  // 每月窗口
  monthly?: QuotaWindow | null;
};

// 一条 Hook 事件
export type HookItem = {
  // 来源代理
  agent: string;
  // 槽位序号
  slot: number;
  // 行为状态
  behavior: string;
  // 正文
  content: string;
  // 正文是否被截断
  contentTruncated: boolean;
  // 图片哈希
  image: string | null;
  /** 原生通道校验 SHA-256 后返回的本地图片 URI；不把内容散列当作 URL。 */
  // 本地图片地址
  imageSource?: string | null;
};

// 单个代理的用量
export type TokenAgent = { agent: string; amount: Amount };

// 桌面发来的完整快照
export type Snapshot = {
  // 协议版本
  version: 1;
  // Token 统计：统计日、总量与各代理用量
  tokens: {
    // 统计日与时区
    day: { date: string; zone: string };
    // 总量
    all: Amount;
    // 各代理用量
    agents: TokenAgent[];
  // 可为空
  } | null;
  // Token 统计错误信息
  tokenError: string | null;
  // Hooks 是否可用
  hooksAvailable: boolean;
  // Hook 列表
  hooks: HookItem[];
  // 供应商额度及观测时间
  providerQuota: { observedAtUnix: number; providers: QuotaProvider[] } | null;
  // 启用的代理，null 表示不过滤
  enabledAgents: string[] | null;
};
