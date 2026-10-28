//! TSX 调用 Rust 业务规则的类型化入口：每条规则都是 `Rules` 模块上的一个方法。
// 引入 nsh 模块宏与 JsonValue
use nsh_core::{JsonValue, prelude::*};

use crate::features::{connection, hooks, usage};
use crate::shared::{
    contracts::{
        models::{PanelConfig, Snapshot, UsageCardProjection, ViewState, typed},
        panel,
    },
    domain::agents,
};

// 声明 nsh 原生模块；TS 里是 `import { Rules } from "loki-metris-mobile-display/native"`
#[module]
/// 在每个 NSH 宿主上于 Rust 中执行的业务规则;TS 只负责传参。
// 规则模块：所有 NSH 宿主共用的业务规则
pub struct Rules;

// 模块方法：snake_case 的函数名在 TS 中自动变为 camelCase
#[module]
impl Rules {
    /// 默认面板配置。
    pub fn default_panel() -> Result<PanelConfig, String> {
        typed(panel::default_panel())
    }

    /// 智能体的展示名称。
    pub fn agent_display_name(raw: String) -> String {
        agents::agent_display_name(&raw)
    }

    /// 指定设备的空视图。
    pub fn connection_empty_view(device_id: String, device_name: String) -> Result<ViewState, String> {
        typed(connection::view_state::empty_view(&device_id, &device_name))
    }

    /// 校验设备名称,返回错误文案;合法时返回 null。
    pub fn connection_name_error(name: String) -> Option<String> {
        connection::identity::name_error(&name).map(str::to_owned)
    }

    /// 设备 ID 是否为规范格式。
    pub fn connection_valid_device_id(id: String) -> bool {
        connection::identity::valid_device_id(&id)
    }

    /// 把用量快照投影成卡片所需数据。
    pub fn usage_card_projection(snapshot: Option<Snapshot>) -> Result<UsageCardProjection, String> {
        let snapshot = serde_json::to_value(snapshot).map_err(|error| error.to_string())?;
        typed(usage::rows::card_projection(&snapshot))
    }

    /// Hook 行为状态对应的展示文案。
    pub fn hooks_label(behavior: String) -> String {
        hooks::label(&behavior).to_owned()
    }

    /// 把 count 个 Hook 按布局("1x1" 或 "2x2")排成页;每格是 Hook 序号,空位为 null。
    pub fn hook_pages(count: u32, layout: String) -> Vec<Vec<Option<u32>>> {
        hooks::pages(count as usize, &layout)
    }
}

/// 仅调试构建提供:测试替身(模拟桌面端的夹具)借此让自己的投影经过与产品
/// `Link` 相同的 Rust 校验。产品构建不包含它。
// 测试夹具专用规则，只在调试构建注册
#[cfg(debug_assertions)]
#[module]
pub struct FixtureRules;

// 模块方法
#[cfg(debug_assertions)]
#[module]
impl FixtureRules {
    /// 把原生端口的 JSON 文本校验为公开视图。产品里由 `Link` 在 Rust 内部完成,
    /// 这个入口让测试替身的投影也经过同一套校验。
    pub fn connection_native_view(raw: String) -> Result<ViewState, String> {
        typed(connection::view_state::native_view(&raw)?)
    }

    /// 解析桌面发来的快照;非法数据返回 null。
    pub fn connection_parse_snapshot(value: JsonValue, images: JsonValue) -> Result<Option<Snapshot>, String> {
        connection::snapshot::project_snapshot(&value, &images)
            .map(typed)
            .transpose()
    }
}
