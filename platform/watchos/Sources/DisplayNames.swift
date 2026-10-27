import Foundation

/// Same public names and comparison rules as src/shared/domain/agents.rs;
/// src/tests/fixtures/display-names.json keeps both tables identical.
func agentTitle(_ agent: String) -> String {
    let key = String(String.UnicodeScalarView(agent.lowercased()
        .precomposedStringWithCanonicalMapping.unicodeScalars.filter { scalar in
            let value = scalar.value
            let whitespace = (0x0009...0x000D).contains(value) || value == 0x0020
                || value == 0x00A0 || value == 0x1680 || (0x2000...0x200A).contains(value)
                || value == 0x2028 || value == 0x2029 || value == 0x202F
                || value == 0x205F || value == 0x3000 || value == 0xFEFF
            return value != 0x005F && value != 0x002D && !whitespace
        }))
    switch key {
    case "codex": return "Codex"
    case "claude", "claudecode": return "Claude Code"
    case "grok", "grokbuild": return "Grok"
    case "workbuddy": return "WorkBuddy"
    case "cursor": return "Cursor"
    case "opencode": return "OpenCode"
    case "hermes": return "Hermes"
    case "openclaw": return "OpenClaw"
    case "codebuddy": return "CodeBuddy"
    case "qwencode": return "Qwen Code"
    case "kimicode": return "Kimi Code"
    case "qoder": return "Qoder"
    case "gemini", "geminicli": return "Gemini CLI"
    case "githubcopilot", "githubcopilotcli": return "GitHub Copilot"
    default: return agent
    }
}

func behaviorTitle(_ kind: String) -> String {
    switch kind {
    case "idle": return "空闲"
    case "running": return "运行"
    case "asking": return "询问"
    default: return "出错"
    }
}
