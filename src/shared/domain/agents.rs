//! Agent 身份归一在 Rust 执行，显示和筛选不会各维护一张别名表。
use icu_normalizer::ComposingNormalizer;

pub fn normalize_comparison_text(value: &str) -> String {
    ComposingNormalizer::new_nfc().normalize(value).into_owned()
}

/// ECMAScript 的空白定义也用于旧偏好/设备名称，避免迁移改变既有身份。
pub fn comparison_whitespace(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

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
