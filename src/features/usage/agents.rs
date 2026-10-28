// 引入规范代理键
use crate::shared::domain::agents::canonical_agent_key;
// 引入序列化
use serde::Serialize;
// 引入集合
use std::collections::HashSet;

// 代理对应的额度供应商键，只有四家有额度
pub fn provider_quota_key(agent: &str) -> Option<String> {
    // 规范化代理键
    let key = canonical_agent_key(agent);
    // 只有四种支持额度的代理才返回
    matches!(key.as_str(), "codex" | "claude" | "cursor" | "grok").then_some(key)
}

// 代理是否启用；没有启用列表表示全部启用
pub fn is_enabled(agent: &str, enabled: Option<&[String]>) -> bool {
    // 没有启用名单则全部显示
    enabled.is_none_or(|items| {
        // 规范化代理键
        let key = canonical_agent_key(agent);
        // 按规范键比较
        items.iter().any(|item| canonical_agent_key(item) == key)
    })
}

// 展示顺序：Codex、Claude、Cursor、Grok、其他、WorkBuddy
pub fn display_rank(agent: &str) -> u8 {
    // 按规范键排序权重
    match canonical_agent_key(agent).as_str() {
        // Codex 排第一
        "codex" => 0,
        // Claude 排第二
        "claude" => 1,
        // Cursor 排第三
        "cursor" => 2,
        // Grok 排第四
        "grok" => 3,
        // WorkBuddy 排在最后
        "workbuddy" => 5,
        // 其余排在中间
        _ => 4,
    }
}

// 取出启用代理的下标
pub fn visible_indices(names: &[String], enabled: Option<&[String]>) -> Vec<usize> {
    // 名称列表
    names
        // 遍历
        .iter()
        // 带下标
        .enumerate()
        // 只保留启用的下标
        .filter_map(|(index, name)| is_enabled(name, enabled).then_some(index))
        // 收集为列表
        .collect()
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
// 一行的数据来源：Token 下标与额度下标，-1 表示没有
pub struct UsageRowRef {
    // Token 项下标
    pub token_index: isize,
    // 额度项下标
    pub quota_index: isize,
}

// 用量行引用的方法
impl UsageRowRef {
    // 取这一行对应的代理名，两者都没有则为 Cursor 的默认行
    fn source<'a>(&self, tokens: &'a [String], quotas: &'a [String]) -> &'a str {
        // 有 Token 项
        if self.token_index >= 0 {
            // 返回其名称
            &tokens[self.token_index as usize]
        // 否则有额度项
        } else if self.quota_index >= 0 {
            // 否则返回额度项名称
            &quotas[self.quota_index as usize]
        // 否则分支
        } else {
            // 都没有则为 cursor
            "cursor"
        }
    }
}

// 合并 Token 与额度，得到最终的行
pub fn project_rows(
    // 是否有 Token 数据
    tokens_present: bool,
    // 代理名列表
    token_names: &[String],
    // 供应商列表
    quota_providers: &[String],
    // 启用名单
    enabled: Option<&[String]>,
    // 返回行列表
) -> Vec<UsageRowRef> {
    // 只保留启用的代理
    let tokens = visible_indices(token_names, enabled);
    // 可见的额度供应商下标
    let quotas = visible_indices(quota_providers, enabled);
    // 已被 Token 行匹配的供应商
    let matched: HashSet<String> = tokens
        // 遍历
        .iter()
        // 取各自的额度供应商键
        .filter_map(|index| provider_quota_key(&token_names[*index]))
        // 收集为集合
        .collect();
    // 每个 Token 行尝试匹配同一供应商的额度
    let mut rows: Vec<_> = tokens
        // 转为迭代器
        .into_iter()
        // 构造行
        .map(|index| UsageRowRef {
            // Token 项下标
            token_index: index as isize,
            // 额度项下标
            quota_index: quotas
                // 遍历
                .iter()
                // 找到同名的
                .find(|quota| {
                    // 对应的额度供应商键
                    provider_quota_key(&token_names[index]).as_deref()
                        // 且与额度供应商同名
                        == Some(quota_providers[**quota].as_str())
                })
                // 没有对应额度则记为 -1
                .map_or(-1, |quota| *quota as isize),
        })
        // 收集为列表
        .collect();
    // 没有 Token 行的额度单独成行
    for quota in quotas {
        // 额度项还没被匹配
        if !matched.contains(&quota_providers[quota]) {
            // 追加仅有额度的行
            rows.push(UsageRowRef {
                // 无 Token 项
                token_index: -1,
                // 额度项下标
                quota_index: quota as isize,
            });
        }
    }
    // Cursor 默认的零值只是一个展示行,绝不是伪造的快照。
    // Cursor 的默认零值只是展示行，不伪造快照
    let show_cursor = (tokens_present || enabled.is_some()) && is_enabled("cursor", enabled);
    // 已有 Cursor 行则不再补
    if show_cursor
        // 并且所有行
        && rows.iter().all(|row| {
            // 对应的额度供应商键
            provider_quota_key(row.source(token_names, quota_providers)).as_deref()
                // 都不是 cursor
                != Some("cursor")
        })
    {
        // 追加缺失的行
        rows.push(UsageRowRef {
            // 无 Token 项
            token_index: -1,
            // 无额度项
            quota_index: -1,
        });
    }
    // 稳定排序让同等排名的智能体保持服务端给出的顺序。
    // 稳定排序，同级保持服务端顺序
    rows.sort_by_key(|row| display_rank(row.source(token_names, quota_providers)));
    // 返回所有行
    rows
}

// Cursor 没有数值时显示 0，其余原样返回
pub fn token_text(source: &str, text: Option<&str>, numeric: bool) -> Option<String> {
    // Cursor 没有数值时显示 0
    if provider_quota_key(source).as_deref() == Some("cursor") && !numeric {
        // 返回 0
        Some("0".to_owned())
    // 否则分支
    } else {
        // 否则沿用原文本
        text.map(str::to_owned)
    }
}

// 测试模块
#[cfg(test)]
#[path = "agents_test.rs"]
mod agents_test;
