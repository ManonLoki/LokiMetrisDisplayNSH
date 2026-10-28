//! 调试构建专用的测试夹具连接:模拟桌面端的公开投影,不实现密码协议、网络或系统凭据。
//! 真实协议由 Rust 的配对核心与实网专项验证;这里只让 UI 场景测试可重复。
//! 时间全部由调用方传入(测试里是虚拟时钟),夹具自己从不读取系统时间。
use crate::features::connection::{
    identity::valid_device_id,
    snapshot::project_snapshot,
    view_state::{empty_view, native_view},
};
use crate::shared::contracts::models::{LinkCommand, LinkOp, ViewState, typed};
use nsh_core::storage::Storage;
use serde_json::{Map, Value, json};

// 配对窗口：六十秒
const PAIRING_WINDOW_MS: u64 = 60_000;
// 已配对记录的存储键
const PAIRED_KEY: &str = "loki.paired";
// 缓存快照的存储键
const CACHE_KEY: &str = "loki.cache";
// 事件夹具的存储键
const INJECT_KEY: &str = "loki.inject";
// 固定投影夹具的存储键
const STATIC_KEY: &str = "test.staticView";
// 固定投影夹具记录命令的存储键
const COMMANDS_KEY: &str = "test.connectionCommands";
// 夹具设备的默认身份
const FIXTURE_DEVICE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

// 测试夹具连接：事件场景或固定投影
pub(super) enum Fixture {
    // 按事件时间线推进的桌面端场景
    Scenario(Scenario),
    // 只返回固定投影并记录命令
    Static(StaticView),
}

impl Fixture {
    // 按存储里的夹具键选择；都没有时用真实连接
    pub(super) fn select() -> Option<Self> {
        if let Some(raw) = Storage::get_item(STATIC_KEY) {
            return Some(Self::Static(StaticView::new(&raw)));
        }
        Storage::get_item(INJECT_KEY).map(|raw| Self::Scenario(Scenario::load(&raw)))
    }

    // 初始化并返回首个公开视图
    pub(super) fn initialize(&mut self, device_id: &str, name: &str, now_ms: u64) -> Result<ViewState, String> {
        match self {
            Self::Scenario(scenario) => scenario.initialize(device_id, name, now_ms),
            Self::Static(view) => view.initialize(),
        }
    }

    // 轮询：没有变化时为 None
    pub(super) fn poll(&mut self, now_ms: u64) -> Result<Option<ViewState>, String> {
        match self {
            Self::Scenario(scenario) => scenario.poll(now_ms),
            Self::Static(view) => view.poll(),
        }
    }

    // 处理命令
    pub(super) fn command(&mut self, command: &LinkCommand, now_ms: u64) {
        match self {
            Self::Scenario(scenario) => scenario.command(command, now_ms),
            Self::Static(view) => view.command(command),
        }
    }
}

// 只在文本变化时交付新视图的去重器
#[derive(Default)]
struct ChangedOnly(Option<String>);

impl ChangedOnly {
    // 与产品 Link.poll 一样：先校验，再记录
    fn view(&mut self, raw: String) -> Result<Option<ViewState>, String> {
        if self.0.as_deref() == Some(raw.as_str()) {
            return Ok(None);
        }
        let view = typed(native_view(&raw)?)?;
        self.0 = Some(raw);
        Ok(Some(view))
    }
}

// 固定投影夹具
pub(super) struct StaticView {
    // 投影
    projection: Value,
    // 收到的命令
    commands: Vec<Value>,
    // 去重器
    changed: ChangedOnly,
}

impl StaticView {
    // 读取固定投影
    fn new(raw: &str) -> Self {
        Self {
            projection: serde_json::from_str(raw).unwrap_or(Value::Null),
            commands: Vec::new(),
            changed: ChangedOnly::default(),
        }
    }

    // 首个视图
    fn initialize(&mut self) -> Result<ViewState, String> {
        self.changed.view(self.projection.to_string())?.ok_or_else(|| "夹具投影无效".to_owned())
    }

    // 轮询
    fn poll(&mut self) -> Result<Option<ViewState>, String> {
        self.changed.view(self.projection.to_string())
    }

    // 记录命令；重命名时同步修改投影
    fn command(&mut self, command: &LinkCommand) {
        if let Ok(value) = serde_json::to_value(command) {
            self.commands.push(value);
        }
        if matches!(command.op, LinkOp::Rename) {
            if let Some(name) = &command.name {
                self.projection["deviceName"] = json!(name);
            }
        }
        let _ = Storage::set_item(COMMANDS_KEY, Value::Array(self.commands.clone()).to_string());
    }
}

// 一个时间线事件
struct Event {
    // 触发时间
    at_ms: u64,
    // 种类：handshake、snapshot 或 transport-fail
    kind: String,
    // snapshot：桌面发来的快照
    snapshot: Value,
    // snapshot：接收时间
    received_at_ms: u64,
    // snapshot：图片哈希到本地地址的映射
    images: Value,
}

// 事件场景夹具
pub(super) struct Scenario {
    // 公开投影状态（含 paired，不含 images）
    state: Value,
    // 图片映射
    images: Value,
    // 按时间排序的事件
    events: Vec<Event>,
    // 下一个未触发的事件
    next: usize,
    // 界面不再活动后不再触发事件
    cancelled: bool,
    // 去重器
    changed: ChangedOnly,
}

// 取字符串字段
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

// 批量设置字段
fn set(state: &mut Value, fields: &[(&str, Value)]) {
    for (key, value) in fields {
        state[*key] = value.clone();
    }
}

// 已发现与已配对合并成主机列表
fn hosts_from(discovered: &[(String, String)], paired: &Map<String, Value>) -> Vec<Value> {
    let mut hosts: Vec<Value> = discovered
        .iter()
        .map(|(id, name)| json!({"id": id, "name": name, "paired": paired.contains_key(id), "available": true}))
        .collect();
    let mut missing: Vec<&String> = paired.keys().filter(|id| !discovered.iter().any(|(found, _)| found == *id)).collect();
    missing.sort();
    for id in missing {
        hosts.push(json!({"id": id, "name": format!("已配对桌面 · {}", &id[..8.min(id.len())]), "paired": true, "available": false}));
    }
    hosts
}

// 可复现的配对码；真实配对码由原生密码核心生成
pub(super) fn pairing_code(now_ms: u64, host_id: &str) -> String {
    let mut hash = 2_166_136_261_u32 as i32;
    for unit in host_id.encode_utf16() {
        hash ^= i32::from(unit);
        hash = hash.wrapping_mul(16_777_619);
    }
    let mixed = ((i64::from(hash) + (now_ms % 10_000) as i64) % 10_000).abs();
    format!("{mixed:04}")
}

// 配对窗口是否已过
fn pairing_expired(now_ms: u64, deadline_ms: u64) -> bool {
    now_ms >= deadline_ms
}

impl Scenario {
    // 从存储读取夹具、已配对记录和缓存
    fn load(inject: &str) -> Self {
        let (discovered, events) = read_fixture(inject);
        let paired = read_paired(Storage::get_item(PAIRED_KEY).as_deref());
        let cache = read_cache(Storage::get_item(CACHE_KEY).as_deref());
        let mut state = empty_view(FIXTURE_DEVICE, "LokiMetris Display");
        state["paired"] = Value::Object(paired.clone());
        state["hosts"] = Value::Array(hosts_from(&discovered, &paired));
        if let Some((source_id, received_at_ms, snapshot)) = cache {
            set(&mut state, &[
                ("snapshot", snapshot),
                ("receivedAtMs", json!(received_at_ms)),
                ("cacheSourceId", json!(source_id)),
            ]);
        }
        let mut events = events;
        events.sort_by_key(|event| event.at_ms);
        Self { state, images: json!({}), events, next: 0, cancelled: false, changed: ChangedOnly::default() }
    }

    // 套用身份，返回首个视图；事件在之后的调用里按时间触发
    fn initialize(&mut self, device_id: &str, name: &str, now_ms: u64) -> Result<ViewState, String> {
        if !device_id.is_empty() {
            self.state["deviceId"] = json!(device_id);
        }
        self.state["deviceName"] = json!(name);
        let raw = self.projection(now_ms);
        self.changed.view(raw)?.ok_or_else(|| "夹具投影无效".to_owned())
    }

    // 先触发到期事件，再返回变化
    fn poll(&mut self, now_ms: u64) -> Result<Option<ViewState>, String> {
        self.fire_due(now_ms);
        let raw = self.projection(now_ms);
        self.changed.view(raw)
    }

    // 处理命令
    fn command(&mut self, command: &LinkCommand, now_ms: u64) {
        self.fire_due(now_ms);
        let id = command.client_id.as_deref().unwrap_or("");
        match command.op {
            LinkOp::Select => self.select_desktop(id, now_ms),
            LinkOp::Forget => {
                self.forget_desktop(id);
                self.persist_paired();
                self.persist_cache();
            }
            LinkOp::Rename => {
                if let Some(name) = &command.name {
                    self.state["deviceName"] = json!(name);
                }
            }
            LinkOp::Refresh | LinkOp::Discover => {
                set(&mut self.state, &[("error", Value::Null), ("status", json!("正在发现桌面端…"))]);
            }
            LinkOp::Active => {
                if command.value == Some(false) {
                    self.cancelled = true;
                }
            }
        }
    }

    // 触发所有到期事件，每个事件按自己的时间生效
    fn fire_due(&mut self, now_ms: u64) {
        while !self.cancelled && self.next < self.events.len() && self.events[self.next].at_ms <= now_ms {
            self.next += 1;
            let index = self.next - 1;
            self.apply(index);
        }
    }

    // 先让过期的配对失效，再输出投影文本
    fn projection(&mut self, now_ms: u64) -> String {
        self.expire_pairing(now_ms);
        let mut value = self.state.clone();
        value["images"] = self.images.clone();
        value.to_string()
    }

    // 应用一个事件
    fn apply(&mut self, index: usize) {
        let at_ms = self.events[index].at_ms;
        match self.events[index].kind.as_str() {
            // 握手：保存配对
            "handshake" => {
                let before = self.state["paired"].clone();
                self.accept_handshake(at_ms);
                if self.state["paired"] != before {
                    self.persist_paired();
                }
            }
            // 快照：解析后接受并缓存
            "snapshot" => {
                let event = &self.events[index];
                let Some(snapshot) = project_snapshot(&event.snapshot, &event.images) else { return };
                let (received, images) = (event.received_at_ms, event.images.clone());
                let before = self.state.clone();
                self.accept_snapshot(snapshot, received);
                if self.state == before {
                    return;
                }
                self.images = images;
                self.persist_cache();
            }
            // 传输中断
            _ => set(&mut self.state, &[
                ("connected", json!(false)),
                ("live", json!(false)),
                ("error", json!("连接中断，正在等待桌面恢复…")),
                ("status", json!("暂未连接")),
            ]),
        }
    }

    // 用户点选后才进入配对；已有凭据则直接等快照
    fn select_desktop(&mut self, id: &str, now_ms: u64) {
        let host = self.state["hosts"].as_array().and_then(|hosts| hosts.iter().find(|host| text(host, "id") == id)).cloned();
        let Some(host) = host else { return };
        if host["available"] != true {
            return;
        }
        if self.state["paired"].get(id).is_some() {
            set(&mut self.state, &[
                ("selectedId", json!(id)),
                ("pairingCode", Value::Null),
                ("pairingDeadlineMs", Value::Null),
                ("connected", json!(false)),
                ("error", Value::Null),
                ("status", json!("正在认证桌面…")),
            ]);
        } else {
            set(&mut self.state, &[
                ("selectedId", json!(id)),
                ("pairingCode", json!(pairing_code(now_ms, id))),
                ("pairingDeadlineMs", json!(now_ms + PAIRING_WINDOW_MS)),
                ("connected", json!(false)),
                ("error", Value::Null),
                ("status", json!("等待在桌面输入手机配对码")),
            ]);
        }
    }

    // 配对窗口过期
    fn expire_pairing(&mut self, now_ms: u64) {
        let Some(deadline) = self.state["pairingDeadlineMs"].as_u64() else { return };
        if !pairing_expired(now_ms, deadline) {
            return;
        }
        set(&mut self.state, &[
            ("pairingCode", Value::Null),
            ("pairingDeadlineMs", Value::Null),
            ("connected", json!(false)),
            ("error", json!("配对窗口已过期，请重新选择桌面。")),
            ("status", json!("暂未连接")),
        ]);
    }

    // 握手成功只保存配对材料，不算已连接
    fn accept_handshake(&mut self, now_ms: u64) {
        let (Some(id), true, Some(deadline)) = (
            self.state["selectedId"].as_str().map(str::to_owned),
            !self.state["pairingCode"].is_null(),
            self.state["pairingDeadlineMs"].as_u64(),
        ) else {
            return;
        };
        if pairing_expired(now_ms, deadline) {
            return self.expire_pairing(now_ms);
        }
        let mut paired = self.state["paired"].as_object().cloned().unwrap_or_default();
        paired.insert(id.clone(), json!({"clientId": id, "savedAtMs": now_ms}));
        let hosts: Vec<Value> = self.state["hosts"]
            .as_array()
            .map(|hosts| hosts.iter().map(|host| if text(host, "id") == id { json!({"id": host["id"], "name": host["name"], "paired": true, "available": host["available"]}) } else { host.clone() }).collect())
            .unwrap_or_default();
        set(&mut self.state, &[
            ("pairingCode", Value::Null),
            ("pairingDeadlineMs", Value::Null),
            ("connected", json!(false)),
            ("paired", Value::Object(paired)),
            ("hosts", Value::Array(hosts)),
            ("error", Value::Null),
            ("status", json!("正在认证桌面…")),
        ]);
    }

    // 第一份真实快照才标记已连接，并保留事件里的接收时间
    fn accept_snapshot(&mut self, snapshot: Value, received_at_ms: u64) {
        let Some(id) = self.state["selectedId"].as_str().map(str::to_owned) else { return };
        if self.state["paired"].get(&id).is_none() || !self.state["pairingCode"].is_null() {
            return;
        }
        set(&mut self.state, &[
            ("snapshot", snapshot),
            ("receivedAtMs", json!(received_at_ms)),
            ("cacheSourceId", json!(id)),
            ("connected", json!(true)),
            ("live", json!(true)),
            ("error", Value::Null),
            ("status", json!("已连接，正在同步")),
        ]);
    }

    // 忘记桌面：移除配对，必要时清空展示内容
    fn forget_desktop(&mut self, id: &str) {
        let mut paired = self.state["paired"].as_object().cloned().unwrap_or_default();
        paired.remove(id);
        let clear = self.state["selectedId"] == id || self.state["cacheSourceId"] == id;
        let selected_gone = self.state["selectedId"] == id;
        let available: Vec<(String, String)> = self.state["hosts"]
            .as_array()
            .map(|hosts| hosts.iter().filter(|host| host["available"] == true).map(|host| (text(host, "id").to_owned(), text(host, "name").to_owned())).collect())
            .unwrap_or_default();
        let hosts = hosts_from(&available, &paired);
        set(&mut self.state, &[("paired", Value::Object(paired)), ("hosts", Value::Array(hosts))]);
        if selected_gone {
            set(&mut self.state, &[("selectedId", Value::Null), ("pairingCode", Value::Null), ("pairingDeadlineMs", Value::Null)]);
        }
        if clear {
            set(&mut self.state, &[
                ("connected", json!(false)),
                ("snapshot", Value::Null),
                ("receivedAtMs", Value::Null),
                ("live", json!(false)),
                ("cacheSourceId", Value::Null),
            ]);
        }
        set(&mut self.state, &[("error", Value::Null), ("status", json!("已忘记，可重新配对"))]);
    }

    // 保存配对记录
    fn persist_paired(&self) {
        let _ = Storage::set_item(PAIRED_KEY, self.state["paired"].to_string());
    }

    // 保存或清除缓存快照
    fn persist_cache(&self) {
        let state = &self.state;
        if state["snapshot"].is_null() || state["receivedAtMs"].is_null() || state["cacheSourceId"].is_null() {
            Storage::remove_item(CACHE_KEY);
        } else {
            let cache = json!({"sourceId": state["cacheSourceId"], "receivedAtMs": state["receivedAtMs"], "snapshot": state["snapshot"]});
            let _ = Storage::set_item(CACHE_KEY, cache.to_string());
        }
    }
}

// 安全整数：非负且不超过 2^53-1
fn safe_int(value: &Value) -> Option<u64> {
    crate::shared::contracts::snapshot::safe_unsigned(value)
}

// 读取事件夹具；任何一处不合法就整体忽略
fn read_fixture(raw: &str) -> (Vec<(String, String)>, Vec<Event>) {
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return (Vec::new(), Vec::new()) };
    if !value.is_object() {
        return (Vec::new(), Vec::new());
    }
    let mut discovered = Vec::new();
    if let Some(items) = value.get("discovered").filter(|items| !items.is_null()) {
        let Some(items) = items.as_array() else { return (Vec::new(), Vec::new()) };
        for item in items {
            match (item["id"].as_str(), item["name"].as_str()) {
                (Some(id), Some(name)) if valid_device_id(id) => discovered.push((id.to_owned(), name.to_owned())),
                _ => return (Vec::new(), Vec::new()),
            }
        }
    }
    let mut events = Vec::new();
    if let Some(items) = value.get("events").filter(|items| !items.is_null()) {
        let Some(items) = items.as_array() else { return (Vec::new(), Vec::new()) };
        for item in items {
            let kind = item["kind"].as_str().unwrap_or("");
            let Some(at_ms) = item["atMs"].as_f64().filter(|at| at.is_finite()) else { return (Vec::new(), Vec::new()) };
            if !["handshake", "snapshot", "transport-fail"].contains(&kind) {
                return (Vec::new(), Vec::new());
            }
            events.push(Event {
                at_ms: at_ms.max(0.0) as u64,
                kind: kind.to_owned(),
                snapshot: item["snapshot"].clone(),
                received_at_ms: item["receivedAtMs"].as_u64().unwrap_or(0),
                images: if item["images"].is_object() { item["images"].clone() } else { json!({}) },
            });
        }
    }
    (discovered, events)
}

// 读取已配对记录，丢弃不合法的项
fn read_paired(raw: Option<&str>) -> Map<String, Value> {
    let mut result = Map::new();
    let Some(Value::Object(map)) = raw.and_then(|raw| serde_json::from_str::<Value>(raw).ok()) else { return result };
    for (id, item) in map {
        if valid_device_id(&id) && item["clientId"].is_string() && safe_int(&item["savedAtMs"]).is_some() {
            result.insert(id, json!({"clientId": item["clientId"], "savedAtMs": item["savedAtMs"]}));
        }
    }
    result
}

// 读取缓存快照
fn read_cache(raw: Option<&str>) -> Option<(String, u64, Value)> {
    let value: Value = serde_json::from_str(raw?).ok()?;
    let source = value["sourceId"].as_str().filter(|id| valid_device_id(id))?;
    let received = safe_int(&value["receivedAtMs"])?;
    let snapshot = project_snapshot(&value["snapshot"], &json!({}))?;
    Some((source.to_owned(), received, snapshot))
}
