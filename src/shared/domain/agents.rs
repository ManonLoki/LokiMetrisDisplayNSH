//! Agent 身份归一在 Rust 执行，显示和筛选不会各维护一张别名表。
use super::text::{comparison_whitespace, normalize_comparison_text};

pub fn canonical_agent_key(raw: &str) -> String {
    let lower = normalize_comparison_text(&raw.to_lowercase());
    let key: String = lower
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !comparison_whitespace(*ch))
        .collect();
    match key.as_str() {
        "claude" | "claudecode" => "claude".into(),
        "grok" | "grokbuild" => "grok".into(),
        "gemini" | "geminicli" => "gemini".into(),
        "githubcopilot" | "githubcopilotcli" => "githubcopilot".into(),
        _ => key,
    }
}

pub fn agent_display_name(raw: &str) -> String {
    match canonical_agent_key(raw).as_str() {
        "codex" => "Codex",
        "claude" => "Claude Code",
        "grok" => "Grok",
        "workbuddy" => "WorkBuddy",
        "cursor" => "Cursor",
        "opencode" => "OpenCode",
        "hermes" => "Hermes",
        "openclaw" => "OpenClaw",
        "codebuddy" => "CodeBuddy",
        "qwencode" => "Qwen Code",
        "kimicode" => "Kimi Code",
        "qoder" => "Qoder",
        "gemini" => "Gemini CLI",
        "githubcopilot" => "GitHub Copilot",
        _ => raw,
    }
    .to_owned()
}

#[cfg(test)]
#[path = "agents_test.rs"]
mod agents_test;
