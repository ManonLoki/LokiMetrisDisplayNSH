use crate::shared::domain::agents::canonical_agent_key;
use serde::Serialize;
use std::collections::HashSet;

pub fn provider_quota_key(agent: &str) -> Option<String> {
    let key = canonical_agent_key(agent);
    matches!(key.as_str(), "codex" | "claude" | "cursor" | "grok").then_some(key)
}

pub fn is_enabled(agent: &str, enabled: Option<&[String]>) -> bool {
    enabled.is_none_or(|items| {
        let key = canonical_agent_key(agent);
        items.iter().any(|item| canonical_agent_key(item) == key)
    })
}

pub fn display_rank(agent: &str) -> u8 {
    match canonical_agent_key(agent).as_str() {
        "codex" => 0,
        "claude" => 1,
        "cursor" => 2,
        "grok" => 3,
        "workbuddy" => 5,
        _ => 4,
    }
}

pub fn visible_indices(names: &[String], enabled: Option<&[String]>) -> Vec<usize> {
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| is_enabled(name, enabled).then_some(index))
        .collect()
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageRowRef {
    pub token_index: isize,
    pub quota_index: isize,
}

impl UsageRowRef {
    fn source<'a>(&self, tokens: &'a [String], quotas: &'a [String]) -> &'a str {
        if self.token_index >= 0 {
            &tokens[self.token_index as usize]
        } else if self.quota_index >= 0 {
            &quotas[self.quota_index as usize]
        } else {
            "cursor"
        }
    }
}

pub fn project_rows(
    tokens_present: bool,
    token_names: &[String],
    quota_providers: &[String],
    enabled: Option<&[String]>,
) -> Vec<UsageRowRef> {
    let tokens = visible_indices(token_names, enabled);
    let quotas = visible_indices(quota_providers, enabled);
    let matched: HashSet<String> = tokens
        .iter()
        .filter_map(|index| provider_quota_key(&token_names[*index]))
        .collect();
    let mut rows: Vec<_> = tokens
        .into_iter()
        .map(|index| UsageRowRef {
            token_index: index as isize,
            quota_index: quotas
                .iter()
                .find(|quota| {
                    provider_quota_key(&token_names[index]).as_deref()
                        == Some(quota_providers[**quota].as_str())
                })
                .map_or(-1, |quota| *quota as isize),
        })
        .collect();
    for quota in quotas {
        if !matched.contains(&quota_providers[quota]) {
            rows.push(UsageRowRef {
                token_index: -1,
                quota_index: quota as isize,
            });
        }
    }
    // Cursor's default zero is a presentation row, never a fabricated snapshot.
    let show_cursor = (tokens_present || enabled.is_some()) && is_enabled("cursor", enabled);
    if show_cursor
        && rows.iter().all(|row| {
            provider_quota_key(row.source(token_names, quota_providers)).as_deref()
                != Some("cursor")
        })
    {
        rows.push(UsageRowRef {
            token_index: -1,
            quota_index: -1,
        });
    }
    // Stable sorting keeps the server's order among equally ranked agents.
    rows.sort_by_key(|row| display_rank(row.source(token_names, quota_providers)));
    rows
}

pub fn token_text(source: &str, text: Option<&str>, numeric: bool) -> Option<String> {
    if provider_quota_key(source).as_deref() == Some("cursor") && !numeric {
        Some("0".to_owned())
    } else {
        text.map(str::to_owned)
    }
}

#[cfg(test)]
#[path = "agents_test.rs"]
mod agents_test;
