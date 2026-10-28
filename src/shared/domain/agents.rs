//! Agent 身份归一在 Rust 执行，显示和筛选不会各维护一张别名表。
// 引入比较用的文本规则
use super::text::{comparison_whitespace, normalize_comparison_text};

// 代理名归一：转小写、做 NFC 归一、去掉下划线连字符与空白，再合并别名
pub fn canonical_agent_key(raw: &str) -> String {
    // 小写后做规范化
    let lower = normalize_comparison_text(&raw.to_lowercase());
    // 只保留字母数字
    let key: String = lower
        // 逐字符
        .chars()
        // 去掉下划线、连字符和空白
        .filter(|ch| *ch != '_' && *ch != '-' && !comparison_whitespace(*ch))
        // 收集为字符串
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
        // 其余原样
        _ => key,
    }
}

// 代理显示名；不认识的名称原样返回
pub fn agent_display_name(raw: &str) -> String {
    // 按规范键取显示名
    match canonical_agent_key(raw).as_str() {
        // Codex
        "codex" => "Codex",
        // Claude Code
        "claude" => "Claude Code",
        // Grok
        "grok" => "Grok",
        // WorkBuddy
        "workbuddy" => "WorkBuddy",
        // Cursor
        "cursor" => "Cursor",
        // OpenCode
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
        // 未知代理不改写
        _ => raw,
    }
    // 转为自有字符串
    .to_owned()
}

// 测试模块
#[cfg(test)]
#[path = "agents_test.rs"]
mod agents_test;
