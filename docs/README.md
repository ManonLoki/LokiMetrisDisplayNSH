# 文档目录

文档描述当前 NSH TSX/Rust 应用、iOS WidgetKit 扩展与 SwiftUI Watch 配套工程。先从 [项目 README](../README.md) 了解功能，再按使用或开发目的选择入口。

## 使用与安装

| 文档 | 内容 |
| --- | --- |
| [移动使用说明](mobile/usage.md) | 首次配对、展示台、平台差异、安装和问题排查 |
| [Android 签名](../signing/android/README.md) | 证书、配置与覆盖安装 |
| [Apple 身份与签名](../signing/apple/README.md) | iPhone／Watch 标识、开发团队与真机签名 |

## 架构

| 文档 | 内容 |
| --- | --- |
| [应用架构](architecture/overview.md) | 模块职责、调用边界、状态归属与源码索引 |
| [交互架构图](../.archify/architecture-lokimetris-20261027-212950/lokimetris.html) | archify 生成的主应用、小组件与 Watch 组件关系，可切换主题并导出 |
| [局域网协议](architecture/protocol.md) | 发现、配对、认证会话、凭据和资源校验 |
| [Watch 配套架构](architecture/watch-companion.md) | 缓存契约、跨设备同步、乱序处理与配套打包 |
| [平台源码](../platform/README.md) | 原生补充工程的放置规则 |
| [Watch 工程](../platform/watchos/README.md) | 独立 SwiftUI 工程与构建测试入口 |

## 开发与维护

| 文档 | 内容 |
| --- | --- |
| [构建说明](development/build.md) | 环境准备、各端开发、打包和签名命令 |
| [测试说明](development/test.md) | 统一测试入口、覆盖范围与宿主要求 |
| [代码约定](development/code-style.md) | TSX／Rust 职责、依赖方向与文件组织 |
| [工具说明](../tools/README.md) | 保留脚本、输入与产物位置 |

## 来源与许可

| 文档 | 内容 |
| --- | --- |
| [NSH 运行时](provenance/nsh-runtime.md) | 官方依赖、应用适配与 [Apache 许可](provenance/nsh-LICENSE-APACHE) |
| [配对核心](provenance/pairing-core.md) | 固定提交、复用范围与 [来源元数据](provenance/pairing-core.json) |
| [配对库中文许可](provenance/pairing-license.zh-CN.md) / [英文许可](provenance/pairing-license.en.md) | 上游授权原文，重建文档时保留 |
| [旧原生工程来源](provenance/mobile-origin.json) | 历史导入路径与校验记录，不是当前源码清单 |

`.nsh/`、`target/` 与 `output/` 保存生成文件和历史结果。旧构建包、截图与测试日志仅证明当时的构建，不作为当前版本已经验收的声明。
