// 引入 Foundation
import Foundation

/// 公开名称与比较规则同 src/shared/domain/agents.rs;
/// src/tests/fixtures/display-names.json 保证两张表完全一致。
// 代理显示名：与 src/shared/domain/agents.rs 使用同样的名称和比较规则，共享夹具保证两张表一致
func agentTitle(_ agent: String) -> String {
    // 归一化键：小写、NFC，去掉下划线、连字符与 ECMAScript 空白
    let key = String(String.UnicodeScalarView(agent.lowercased()
        // 规范化后逐个字符
        .precomposedStringWithCanonicalMapping.unicodeScalars.filter { scalar in
            // 字符值
            let value = scalar.value
            // 空白字符集合
            let whitespace = (0x0009...0x000D).contains(value) || value == 0x0020
                // 各种空白字符
                || value == 0x00A0 || value == 0x1680 || (0x2000...0x200A).contains(value)
                // 行分隔与段分隔等
                || value == 0x2028 || value == 0x2029 || value == 0x202F
                // 其余空白字符
                || value == 0x205F || value == 0x3000 || value == 0xFEFF
            // 保留非下划线、非连字符、非空白的字符
            return value != 0x005F && value != 0x002D && !whitespace
        }))
    // 按归一化键映射显示名，别名合并
    switch key {
    // Codex 智能体
    case "codex": return "Codex"
    // Claude 的两种写法
    case "claude", "claudecode": return "Claude Code"
    // Grok 的两种写法
    case "grok", "grokbuild": return "Grok"
    // WorkBuddy 智能体
    case "workbuddy": return "WorkBuddy"
    // Cursor 智能体
    case "cursor": return "Cursor"
    // OpenCode 智能体
    case "opencode": return "OpenCode"
    // Hermes 智能体
    case "hermes": return "Hermes"
    // OpenClaw 的显示名
    case "openclaw": return "OpenClaw"
    // CodeBuddy 的显示名
    case "codebuddy": return "CodeBuddy"
    // Qwen Code 的显示名
    case "qwencode": return "Qwen Code"
    // Kimi Code 的显示名
    case "kimicode": return "Kimi Code"
    // Qoder 的显示名
    case "qoder": return "Qoder"
    // Gemini CLI 的两种写法共用一个名称
    case "gemini", "geminicli": return "Gemini CLI"
    // GitHub Copilot 的两种写法共用一个名称
    case "githubcopilot", "githubcopilotcli": return "GitHub Copilot"
    // 未知代理原样显示
    default: return agent
    }
}

// 行为状态的中文文案；未知按“出错”处理
func behaviorTitle(_ kind: String) -> String {
    // 按行为种类取标题
    switch kind {
    // 空闲
    case "idle": return "空闲"
    // 运行
    case "running": return "运行"
    // 询问
    case "asking": return "询问"
    // 其余为出错
    default: return "出错"
    }
}
