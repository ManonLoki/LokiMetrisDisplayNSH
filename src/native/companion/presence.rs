//! 真实Watch前台回执到桌面既有wire的转换；系统可达性和背景文件投递从不生成证明。

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// WC即时请求的逻辑期限；超时保留物理槽，终态回调前不叠加请求。
pub(in crate::native) const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// 与桌面严格wire一致，历史观察最多保留七天。
const MAX_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// 桌面接受的精确JSON整数上限，达到后不得复位高水位。
const MAX_SEQUENCE: u64 = 9_007_199_254_740_991;
/// 即时回执只承载公开身份和证明，不能夹带无界业务内容。
const MAX_REPLY: usize = 4096;

/// 一条当前LAN租约的探测意图；sink只提交公开回执到原连接owner的有界事件通道。
pub(in crate::native) struct ProbeRequest {
    pub root: PathBuf,
    pub source_id: String,
    pub scope: String,
    pub sink: Arc<dyn Fn(WireReport) + Send + Sync>,
}

/// 用户切换、退出和ping仅提交意图，所有WC操作仍在原有iPhone单worker执行。
pub(in crate::native) enum Command {
    Probe(ProbeRequest),
    Cancel(String),
}

/// 实际发送给Watch的完整快照身份；与当前认证来源绑定，不含数据或凭据。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::native) struct ExpectedSnapshot {
    pub stream_id: String,
    pub revision: String,
    pub source_id: String,
}

/// Watch安装身份，保持桌面既有严格info字段。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::native) struct WatchInfo {
    pub device_id: String,
    pub name: String,
    pub app_version: String,
}

/// 手机单调时间产生的保守年龄，sequence只在真实回执上递增。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::native) struct Observation {
    pub sequence: u64,
    pub age_ms: u64,
}

/// 与桌面WatchCompanionReport完全相同的wire形状。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::native) struct WireReport {
    pub info: WatchInfo,
    pub reachable: bool,
    pub contact: Option<Observation>,
    pub sync: Option<Observation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SyncAck {
    stream_id: String,
    revision: String,
    source_id: String,
    age_ms: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    version: u8,
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    info: WatchInfo,
    active: bool,
    #[serde(deserialize_with = "required_nullable")]
    foreground_id: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    sync: Option<SyncAck>,
}

/// 允许显式null但拒绝省略字段，保持回执的固定形状。
fn required_nullable<'de, D, T>(input: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(input)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Revoked {
    version: u8,
    #[serde(rename = "type")]
    kind: String,
    device_id: String,
    foreground_id: String,
}

/// 时间锚与初始年龄分开，避免系统刚开机时减穿单调时间起点。
#[derive(Clone, Copy)]
struct Observed {
    sequence: u64,
    received: Instant,
    initial_age: Duration,
}
impl Observed {
    /// 向上取整且保留七天上限；倒退时钟不能制造新鲜事实。
    fn at(self, now: Instant) -> Option<Observation> {
        let age = self
            .initial_age
            .checked_add(now.checked_duration_since(self.received)?)?;
        let millis = age
            .as_millis()
            .checked_add(u128::from(age.subsec_nanos() % 1_000_000 != 0))?;
        let age_ms = u64::try_from(millis)
            .ok()
            .filter(|age| *age <= MAX_AGE_MS)?;
        Some(Observation {
            sequence: self.sequence,
            age_ms,
        })
    }
}

/// 唯一在途物理请求；逻辑超时只撤销资格，迟到终态才释放该槽。
struct Pending {
    challenge: String,
    scope: String,
    expected: ExpectedSnapshot,
    started: Instant,
    expired: bool,
}

/// 在iPhone原单worker内拥有的回执状态；计数器不随LAN重连、前后台或换表复位。
#[derive(Default)]
pub(in crate::native) struct Controller {
    binding: Option<ProbeRequest>,
    due: bool,
    pending: Option<Pending>,
    sequence: u64,
    info: Option<WatchInfo>,
    foreground: Option<String>,
    available: bool,
    contact: Option<Observed>,
    sync: Option<Observed>,
    sync_key: Option<(String, String, String, String)>,
    retired: BTreeSet<(String, String)>,
}
impl Controller {
    /// 新LAN代次撤销旧证明但不复位序号；同租约新ping只替换一次性回传目标。
    pub(in crate::native) fn command(&mut self, command: Command) {
        match command {
            Command::Probe(request) => {
                if self.binding.as_ref().is_none_or(|old| {
                    old.scope != request.scope || old.source_id != request.source_id
                }) {
                    self.info = None;
                    self.foreground = None;
                    self.available = false;
                    self.contact = None;
                    self.sync = None;
                    self.sync_key = None;
                    self.retired.clear();
                    if let Some(pending) = &mut self.pending {
                        pending.expired = true;
                    }
                }
                self.binding = Some(request);
                self.due = true;
            }
            Command::Cancel(scope) => {
                if self
                    .binding
                    .as_ref()
                    .is_some_and(|value| value.scope == scope)
                {
                    self.binding = None;
                    self.due = false;
                    self.available = false;
                    if let Some(pending) = &mut self.pending {
                        pending.expired = true;
                    }
                }
            }
        }
    }

    /// 只读当前有待探测的固定目录与来源；原生驱动必须再核对私有根及已落盘manifest。
    pub(in crate::native) fn target(&self) -> Option<(&std::path::Path, &str)> {
        if !self.due || self.pending.is_some() {
            return None;
        }
        let binding = self.binding.as_ref()?;
        Some((&binding.root, &binding.source_id))
    }

    /// 只有驱动确认可发送且manifest属于当前来源后才占物理槽，challenge每请求唯一。
    pub(in crate::native) fn start(
        &mut self,
        expected: ExpectedSnapshot,
        now: Instant,
    ) -> Option<(String, Vec<u8>)> {
        self.target()?;
        let binding = self.binding.as_ref()?;
        if binding.source_id != expected.source_id
            || !canonical_uuid(&expected.stream_id)
            || !canonical_uuid(&expected.source_id)
            || expected
                .revision
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .map(|value| value.to_string())
                .as_ref()
                != Some(&expected.revision)
        {
            return None;
        }
        let challenge = uuid::Uuid::new_v4().to_string();
        let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"type":"presenceProbe","challenge":challenge,
            "streamId":expected.stream_id,"revision":expected.revision,"sourceId":expected.source_id})).ok()?;
        self.pending = Some(Pending {
            challenge: challenge.clone(),
            scope: binding.scope.clone(),
            expected,
            started: now,
            expired: false,
        });
        self.due = false;
        Some((challenge, bytes))
    }

    /// OS状态只可撤销；paired/installed/reachable均不能生成contact或sync。
    pub(in crate::native) fn unavailable(&mut self, now: Instant) {
        self.due = false;
        if let Some(pending) = &mut self.pending {
            pending.expired = true;
        }
        let changed = self.available;
        self.available = false;
        if changed {
            self.emit(now);
        }
    }

    /// 每轮仅为逻辑8秒期限唤醒；保持物理槽直到WC终态，阻止超时后无界叠加请求。
    pub(in crate::native) fn deadline(&self) -> Option<Instant> {
        self.pending
            .as_ref()
            .filter(|pending| !pending.expired)
            .map(|pending| pending.started + PROBE_TIMEOUT)
    }
    /// 超时只撤销逻辑资格；原生终态回调仍拥有唯一物理槽。
    pub(in crate::native) fn expire(&mut self, now: Instant) {
        if self.deadline().is_some_and(|deadline| now >= deadline) {
            self.pending.as_mut().expect("存在到期物理槽").expired = true;
            self.available = false;
            self.emit(now);
        }
    }

    /// 唯一WC回调入口：严格JSON、请求challenge、当前LAN与前台代次全部满足才产生证明。
    pub(in crate::native) fn complete(
        &mut self,
        challenge: &str,
        bytes: Option<&[u8]>,
        now: Instant,
    ) {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| pending.challenge != challenge)
        {
            return;
        }
        let pending = self.pending.take().expect("已匹配唯一物理槽");
        if pending.expired
            || now
                .checked_duration_since(pending.started)
                .is_none_or(|age| age >= PROBE_TIMEOUT)
            || self.binding.as_ref().is_none_or(|binding| {
                binding.scope != pending.scope || binding.source_id != pending.expected.source_id
            })
        {
            return;
        }
        let reply = bytes
            .filter(|bytes| bytes.len() <= MAX_REPLY)
            .and_then(|bytes| serde_json::from_slice::<Reply>(bytes).ok());
        let Some(reply) = reply.filter(|reply| valid_reply(reply, &pending)) else {
            self.available = false;
            self.emit(now);
            return;
        };
        if reply.foreground_id.as_ref().is_some_and(|foreground| {
            self.retired
                .contains(&(reply.info.device_id.clone(), foreground.clone()))
        }) {
            return;
        }
        let Some(sequence) = self
            .sequence
            .checked_add(1)
            .filter(|value| *value <= MAX_SEQUENCE)
        else {
            self.available = false;
            self.emit(now);
            return;
        };
        self.sequence = sequence;
        if self
            .info
            .as_ref()
            .is_some_and(|info| info.device_id != reply.info.device_id)
        {
            self.sync = None;
            self.sync_key = None;
        }
        self.info = Some(reply.info);
        self.foreground = reply.foreground_id.clone();
        self.available = reply.active;
        let round_trip = now.duration_since(pending.started);
        self.contact = Some(Observed {
            sequence,
            received: now,
            initial_age: round_trip,
        });
        if let Some(sync) = reply.sync {
            let key = (
                reply.foreground_id.expect("有效sync属于真实前台"),
                sync.stream_id,
                sync.revision,
                sync.source_id,
            );
            if self.sync_key.as_ref() != Some(&key) {
                self.sync_key = Some(key);
                self.sync = Some(Observed {
                    sequence,
                    received: now,
                    initial_age: round_trip + Duration::from_millis(sync.age_ms),
                });
            }
        }
        self.emit(now);
    }

    /// 有界邮箱溢出意味着可能漏掉当前代次撤销，先失效整个在途资格再应用已保留条目。
    pub(in crate::native) fn revoke_batch(
        &mut self,
        entries: Vec<Vec<u8>>,
        overflow: bool,
        now: Instant,
    ) {
        if overflow {
            self.unavailable(now);
        }
        for bytes in entries {
            self.revoke(&bytes, now);
        }
    }

    /// Watch离开前台时尽力撤销；旧前台撤销不得影响新前台，迟到positive也不能复活旧代次。
    pub(in crate::native) fn revoke(&mut self, bytes: &[u8], now: Instant) {
        if bytes.len() > MAX_REPLY {
            return;
        }
        let Ok(value) = serde_json::from_slice::<Revoked>(bytes) else {
            return;
        };
        if value.version != 1
            || value.kind != "presenceRevoked"
            || !canonical_uuid(&value.device_id)
            || !canonical_uuid(&value.foreground_id)
        {
            return;
        }
        if self.retired.len() >= 16 {
            // 有界集合耗尽时撤销整个在途资格，不让被淘汰的旧代次回执重新上线。
            if let Some(pending) = &mut self.pending {
                pending.expired = true;
            }
            self.retired.clear();
        }
        self.retired
            .insert((value.device_id.clone(), value.foreground_id.clone()));
        if self
            .info
            .as_ref()
            .is_some_and(|info| info.device_id == value.device_id)
            && self.foreground.as_ref() == Some(&value.foreground_id)
        {
            self.available = false;
            self.emit(now);
        }
    }

    /// 发送前只计算保守年龄；无实际identity/contact时不凭空创建Watch或在线。
    fn emit(&self, now: Instant) {
        let (Some(binding), Some(info)) = (&self.binding, &self.info) else {
            return;
        };
        let contact = self.contact.and_then(|value| value.at(now));
        let sync = self.sync.and_then(|value| value.at(now)).filter(|sync| {
            contact.is_some_and(|contact| {
                sync.sequence <= contact.sequence && sync.age_ms >= contact.age_ms
            })
        });
        (binding.sink)(WireReport {
            info: info.clone(),
            reachable: self.available && contact.is_some_and(|contact| contact.age_ms < 45_000),
            contact,
            sync,
        });
    }
}

/// 安装与前台身份都用规范、非nil UUID，不接受设备名称推断身份。
fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
/// 回复只能由当前请求产生；sync必须匹配完整manifest身份，inactive绝不携带同步证明。
fn valid_reply(reply: &Reply, pending: &Pending) -> bool {
    let info = &reply.info;
    reply.version == 1
        && reply.kind == "presenceReply"
        && reply.challenge == pending.challenge
        && canonical_uuid(&info.device_id)
        && loki_metris_pairing::valid_device_name(&info.name)
        && !info.app_version.trim().is_empty()
        && info.app_version.len() <= 64
        && !info.app_version.chars().any(char::is_control)
        && if reply.active {
            reply.foreground_id.as_deref().is_some_and(canonical_uuid)
                && reply.sync.as_ref().is_none_or(|sync| {
                    sync.stream_id == pending.expected.stream_id
                        && sync.revision == pending.expected.revision
                        && sync.source_id == pending.expected.source_id
                        && sync.age_ms <= MAX_AGE_MS
                })
        } else {
            reply.foreground_id.is_none() && reply.sync.is_none()
        }
}

#[cfg(test)]
#[path = "presence_test.rs"]
mod tests;
