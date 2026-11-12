//! Agent 身份归一在 Rust 执行，显示和筛选不会各维护一张别名表。
use super::text::{comparison_whitespace, normalize_comparison_text};

// 代理名归一：转小写、做 NFC 归一、去掉下划线连字符与空白，再合并别名
pub fn canonical_agent_key(raw: &str) -> String {
    let lower = normalize_comparison_text(&raw.to_lowercase());
    // 只保留字母数字
    let key: String = lower
        .chars()
        // 去掉下划线、连字符和空白
        .filter(|ch| *ch != '_' && *ch != '-' && !comparison_whitespace(*ch))
        .collect();
    // 别名合并为同一身份
    match key.as_str() {
        // Claude 的两种写法统一
        "claude" | "claudecode" => "claude".into(),
        // Grok 的两种写法
        "grok" | "grokbuild" => "grok".into(),
        // Gemini 的两种写法
        "gemini" | "geminicli" => "gemini".into(),
        // GitHub Copilot 的两种写法
        "githubcopilot" | "githubcopilotcli" => "githubcopilot".into(),
        _ => key,
    }
}

// 代理显示名；不认识的名称原样返回
pub fn agent_display_name(raw: &str) -> String {
    match canonical_agent_key(raw).as_str() {
        // Codex 智能体
        "codex" => "Codex",
        // Claude Code 智能体
        "claude" => "Claude Code",
        "grok" => "Grok",
        // WorkBuddy 智能体
        "workbuddy" => "WorkBuddy",
        // Cursor 智能体
        "cursor" => "Cursor",
        // OpenCode 智能体
        "opencode" => "OpenCode",
        // Hermes 的写法
        "hermes" => "Hermes",
        // OpenClaw 的写法
        "openclaw" => "OpenClaw",
        // CodeBuddy 的写法
        "codebuddy" => "CodeBuddy",
        // Qwen Code 的写法
        "qwencode" => "Qwen Code",
        // Kimi Code 的写法
        "kimicode" => "Kimi Code",
        // Qoder 的写法
        "qoder" => "Qoder",
        // Gemini CLI 的写法
        "gemini" => "Gemini CLI",
        // GitHub Copilot 的写法
        "githubcopilot" => "GitHub Copilot",
        _ => raw,
    }
    .to_owned()
}

#[cfg(test)]
#[path = "agents_test.rs"]
mod agents_test;
