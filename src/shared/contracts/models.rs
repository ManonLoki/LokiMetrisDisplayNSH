//! TSX 与 Rust 之间传递的公开数据形状。
//! 这些结构同时决定 Rust 侧的反序列化校验和 TS 侧自动生成的类型声明。
// 引入 nsh 的 JS 对象/枚举派生与 JSON 值
use nsh_core::{JsonValue, prelude::*};
// 引入序列化
use serde::{Deserialize, Serialize};

/// 用量卡片的详细程度。
// 精简或详细
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum UsageDetail {
    // 精简
    Compact,
    // 详细
    Detailed,
}

/// Hooks 的每页布局。
// 每页一张或四张
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
pub enum HookLayout {
    // 每页一张
    #[serde(rename = "1x1")]
    #[prop(rename = "1x1")]
    OneByOne,
    // 每页四张
    #[serde(rename = "2x2")]
    #[prop(rename = "2x2")]
    TwoByTwo,
}

/// 面板偏好。
// 面板配置
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct PanelConfig {
    /// 是否显示用量卡片。
    pub show_usage: bool,
    /// 是否显示 Hooks。
    pub show_hooks: bool,
    /// 用量卡片的详细程度。
    pub usage_detail: UsageDetail,
    /// Hooks 的每页布局。
    pub hook_layout: HookLayout,
}

/// 数值的可用性。
// 可用、无记录或不可用
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    // 可用
    Available,
    // 无记录
    NoRecords,
    // 不可用
    Unavailable,
}

/// 数值的完整程度。
// 完整、部分或未知
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum Completeness {
    // 完整
    Complete,
    // 部分
    Partial,
    // 未知
    Unknown,
}

/// 数值的可信程度。
// 精确、推导或疑似
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    // 精确
    Exact,
    // 推导
    Derived,
    // 疑似
    Suspected,
}

/// 一个 Token 数量及其元信息。
// Token 数量
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct Amount {
    /// 十进制总数文本;没有时为空。
    pub total: Option<String>,
    /// 可用性。
    pub availability: Availability,
    /// 完整程度。
    pub completeness: Completeness,
    /// 可信程度。
    pub confidence: Confidence,
}

/// 额度窗口的种类。
// 月度、每周或五小时
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum QuotaKind {
    // 月度
    Monthly,
    // 每周
    Weekly,
    // 五小时
    FiveHour,
}

/// 一个额度窗口。
// 额度窗口
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    /// 窗口种类。
    pub kind: QuotaKind,
    /// 已用额度,单位为十分之一个百分点。
    pub used_tenths: u32,
}

/// 一个提供方的额度。
// 提供方额度
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct QuotaProvider {
    /// 提供方标识。
    pub provider: String,
    /// 套餐名。
    pub plan: Option<String>,
    /// 五小时窗口。
    #[serde(default)]
    pub five_hour: Option<QuotaWindow>,
    /// 每周窗口。
    #[serde(default)]
    pub weekly: Option<QuotaWindow>,
    /// 月度窗口。
    #[serde(default)]
    pub monthly: Option<QuotaWindow>,
}

/// 额度快照。
// 额度观测时间与各提供方
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuota {
    /// 观测时间,Unix 秒。
    pub observed_at_unix: i64,
    /// 各提供方的额度。
    pub providers: Vec<QuotaProvider>,
}

/// 一个 Hook 的当前图文事实。
// Hook 条目
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct HookItem {
    /// 智能体名称。
    pub agent: String,
    /// 槽位序号。
    pub slot: u32,
    /// 行为状态:idle、running、asking 等。
    pub behavior: String,
    /// 文本内容。
    pub content: String,
    /// 文本是否被截断。
    pub content_truncated: bool,
    /// 图片内容哈希。
    pub image: Option<String>,
    /// 原生通道校验 SHA-256 后返回的本地图片 URI。
    #[serde(default)]
    pub image_source: Option<String>,
}

/// 单个智能体的 Token 数量。
// 智能体 Token
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct TokenAgent {
    /// 智能体名称。
    pub agent: String,
    /// 数量。
    pub amount: Amount,
}

/// 统计所属的日期与时区。
// 日期与时区
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct TokenDay {
    /// 日期。
    pub date: String,
    /// 时区。
    pub zone: String,
}

/// Token 用量。
// 当日、总量与各智能体
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    /// 统计日。
    pub day: TokenDay,
    /// 总量。
    pub all: Amount,
    /// 各智能体。
    pub agents: Vec<TokenAgent>,
}

/// 桌面端发来并经 Rust 校验的公开快照。
// 公开快照
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 协议版本。
    pub version: u32,
    /// Token 用量。
    pub tokens: Option<Tokens>,
    /// Token 用量不可用的原因。
    pub token_error: Option<String>,
    /// Hooks 是否可用。
    pub hooks_available: bool,
    /// Hook 条目。
    pub hooks: Vec<HookItem>,
    /// 额度。
    pub provider_quota: Option<ProviderQuota>,
    /// 启用的智能体;空表示全部。
    pub enabled_agents: Option<Vec<String>>,
}

/// 已发现或已配对的桌面端。
// 桌面端
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    /// 桌面端标识。
    pub id: String,
    /// 名称。
    pub name: String,
    /// 是否已配对。
    pub paired: bool,
    /// 当前是否可连接。
    pub available: bool,
}

/// 原生连接的公开投影;展示层不会接触真实配对密钥。
// 连接视图
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    /// 移动宿主遗留的面板偏好,仅首次视图携带。
    #[serde(default)]
    pub panel_config: Option<PanelConfig>,
    /// 本机设备标识。
    pub device_id: String,
    /// 本机设备名称。
    pub device_name: String,
    /// 桌面端列表。
    pub hosts: Vec<Host>,
    /// 当前选中的桌面端。
    pub selected_id: Option<String>,
    /// 状态文案。
    pub status: String,
    /// 错误文案。
    pub error: Option<String>,
    /// 图片资源告警。
    #[serde(default)]
    pub asset_warning: Option<String>,
    /// 配对码。
    pub pairing_code: Option<String>,
    /// 配对截止时间,毫秒。
    pub pairing_deadline_ms: Option<u64>,
    /// 是否已连接。
    pub connected: bool,
    /// 当前快照。
    pub snapshot: Option<Snapshot>,
    /// 快照接收时间,毫秒。
    pub received_at_ms: Option<u64>,
    /// 当前会话收到过快照;缓存回放为 false。
    pub live: bool,
    /// 缓存快照来源的桌面标识。
    pub cache_source_id: Option<String>,
}

/// 额度等级。
// 红、黄、绿
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum QuotaLevel {
    // 紧张
    Red,
    // 偏低
    Yellow,
    // 充足
    Green,
}

/// 额度窗口的展示投影。
// 展示用窗口
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct DisplayWindow {
    /// 窗口名称文案。
    pub kind_label: String,
    /// 百分比文案。
    pub percent: String,
    /// 等级;配色由组件决定。
    pub level: QuotaLevel,
    /// 剩余额度,十分之一个百分点。
    pub remaining_tenths: u32,
}

/// 用量卡片里的一行。
// 展示行
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRow {
    /// 名称。
    pub name: String,
    /// 数量文本。
    pub token_text: Option<String>,
    /// 紧凑写法。
    pub compact: Option<String>,
    /// 精确写法。
    pub exact: Option<String>,
    /// 套餐名。
    pub plan: Option<String>,
    /// 额度窗口。
    pub windows: Vec<DisplayWindow>,
}

/// 一个提供方的额度投影。
// 提供方额度展示
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct QuotaProjection {
    /// 提供方标识。
    pub provider: String,
    /// 展示名称。
    pub name: String,
    /// 套餐名。
    pub plan: Option<String>,
    /// 额度窗口。
    pub windows: Vec<DisplayWindow>,
}

/// 用量卡片所需的全部业务事实。
// 卡片投影
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct UsageCardProjection {
    /// 是否有 Token 数据。
    pub has_tokens: bool,
    /// 是否有精确写法。
    pub has_exact: bool,
    /// 日期。
    pub date: Option<String>,
    /// 时区。
    pub zone: Option<String>,
    /// 总量文本。
    pub total: String,
    /// 总量精确写法。
    pub exact: Option<String>,
    /// 总量无障碍文本。
    pub amount_accessibility: Option<String>,
    /// 缺失时的说明。
    pub missing_text: String,
    /// 各提供方的额度。
    pub providers: Vec<QuotaProjection>,
    /// 各行。
    pub rows: Vec<DisplayRow>,
    /// 额度观测时间,Unix 秒。
    pub quota_observed_at_unix: i64,
}

/// 连接命令的种类。
// 发现、选择、遗忘、重命名、活动状态或刷新
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PropEnum)]
#[serde(rename_all = "camelCase")]
pub enum LinkOp {
    // 发现桌面端
    Discover,
    // 选择桌面端
    Select,
    // 遗忘桌面端
    Forget,
    // 重命名本机
    Rename,
    // 报告界面是否活动
    Active,
    // 刷新
    Refresh,
}

/// 下发给连接所有者的命令;各种类需要的字段不同。
// 连接命令
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct LinkCommand {
    /// 命令种类。
    pub op: LinkOp,
    /// select、forget:目标桌面端的客户端标识。
    #[prop(skip_none)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// rename:新的设备名称。
    #[prop(skip_none)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// active:界面是否活动。
    #[prop(skip_none)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<bool>,
}

/// 设备身份。
// 设备身份
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// 设备标识;连接所有者确认之前为空。
    pub id: String,
    /// 设备名称。
    pub name: String,
}

/// 设置用例返回的公开状态;空字段表示无变化。
// 设置结果
#[derive(Clone, Debug, Serialize, Deserialize, JsObject)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResult {
    /// 会话编号;后续调用都要带上它。
    pub session: u64,
    /// 是否正在保存名称。
    pub saving_name: bool,
    /// 名称问题提示。
    pub name_issue: Option<String>,
    /// 名称保存反馈。
    pub name_feedback: Option<String>,
    /// 面板问题提示。
    pub panel_issue: Option<String>,
    /// 需要覆盖的面板配置。
    pub panel: Option<PanelConfig>,
    /// 当前设备身份。
    pub device: Option<Device>,
    /// 用于初始化原生端口的身份。
    pub initialize: Option<Device>,
    /// 需要覆盖的名称草稿。
    pub draft: Option<String>,
    /// 需要覆盖的视图。
    pub view: Option<ViewState>,
    /// 需要下发给 Link 端口的命令。
    pub commands: Vec<LinkCommand>,
}

/// 把动态 JSON 校验并转换为公开结构。
// 形状不符时报错，而不是把残缺数据交给 UI
pub(crate) fn typed<T: serde::de::DeserializeOwned>(value: JsonValue) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("公开数据形状无效: {error}"))
}
