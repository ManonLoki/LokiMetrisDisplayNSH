/** 原生投影共用的数据契约；仅公开展示事实，不包含凭据或界面状态。 */
export type Amount = {
  total: string | null;
  availability: "available" | "noRecords" | "unavailable";
  completeness: "complete" | "partial" | "unknown";
  confidence: "exact" | "derived" | "suspected";
};

export type QuotaWindow = { kind: "monthly" | "weekly" | "fiveHour"; usedTenths: number };

export type QuotaProvider = {
  provider: string;
  plan: string | null;
  fiveHour?: QuotaWindow | null;
  weekly?: QuotaWindow | null;
  monthly?: QuotaWindow | null;
};

export type HookItem = {
  agent: string;
  slot: number;
  behavior: string;
  content: string;
  contentTruncated: boolean;
  image: string | null;
  /** 原生通道校验 SHA-256 后返回的本地图片 URI；不把内容散列当作 URL。 */
  imageSource?: string | null;
};

export type TokenAgent = { agent: string; amount: Amount };

export type Snapshot = {
  version: 1;
  tokens: {
    day: { date: string; zone: string };
    all: Amount;
    agents: TokenAgent[];
  } | null;
  tokenError: string | null;
  hooksAvailable: boolean;
  hooks: HookItem[];
  providerQuota: { observedAtUnix: number; providers: QuotaProvider[] } | null;
  enabledAgents: string[] | null;
};
