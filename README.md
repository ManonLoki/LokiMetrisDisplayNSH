# 诡秘神谕 · LokiMetris Display

**桌面客户端：[LokiMetris · 诡秘神谕](https://github.com/ManonLoki/LokiMetis)** —— 电脑采集用量与活动，展示端通过局域网读取。

在 Windows、macOS、Linux、Android、iPhone、iPad 和 Apple Watch 上查看 AI 编程工具的 Token 用量、账户额度、Agent 活动与图片。首次使用四位码配对，之后恢复已保存的认证连接。

[下载安装包](https://github.com/ManonLoki/LokiMetrisDisplayNSH/releases/latest) · [使用说明](docs/mobile/usage.md) · [交互架构图](.archify/architecture-lokimetris-20261028-105048/lokimetris.html) · [从源码运行](#从源码运行) · [文档目录](docs/README.md)

## 能看到什么

- **Token 用量**：查看桌面统计日期对应的总量和各 Agent 明细，支持精简与详细模式，保留完整整数。
- **账户额度**：显示来源提供的额度周期、使用比例与套餐信息；明细跟随桌面启用的 Agent。
- **Agent 活动**：展示空闲、运行、询问与出错状态，支持 Hooks 正文、静态图片和 GIF 详情。
- **自定义展示台**：独立开关用量与 Hooks，选择 1×1／2×2 卡片布局，适配窄窗口、横屏与宽屏侧栏。
- **Apple Watch**：读取 iPhone 同步的用量、额度、活动与图片，保留本地缓存和真实同步时间。
- **iPhone 小组件与待机显示**：主屏幕小／中组件、三种锁屏组件与待机显示共用旧版 WidgetKit 标识，显示最近摘要和真实同步时间。

展示端读取电脑提供的快照，不独立采集 AI 会话或重新计算总量。普通应用在前台连接电脑；iOS 小组件由系统调度，使用桌面授予的独立只读权限短时拉取摘要；每次申请约 15 分钟后刷新，实际唤醒由系统决定。Watch 可由系统继续接收 iPhone 已排队的缓存。Android 系统小组件、悬浮窗和后台常驻同步尚未包含。

## 支持的设备

| 设备 | 实现与运行条件 |
| --- | --- |
| Windows | NSH 共享界面；原生开发使用 MSVC，macOS 可交叉打包 |
| macOS | NSH 共享界面；使用 macOS 与 Apple 开发工具 |
| Linux | NSH 共享界面；需要 GTK 4.14+、会话 D-Bus 与 Secret Service |
| Android 手机、平板、电视 | NSH 共享界面与应用 Java 接线；电视支持方向键导航 |
| iPhone / iPad | NSH 共享界面；WidgetKit 小组件、待机显示及系统共享容器 |
| Apple Watch | 独立 SwiftUI 配套应用，需与运行本应用的 iPhone 配对 |

平台支持表示源码提供对应实现；构建通过、模拟器运行与真机验收分别记录。具体工具、系统与签名条件见 [构建说明](docs/development/build.md)。

## 第一次连接电脑

1. 启动桌面客户端，让电脑与展示设备处于同一局域网。
2. 打开展示端“设置”，允许系统提示的本地网络访问，选择发现的电脑。
3. 保持展示端在前台，在电脑配对窗口输入展示端给出的四位码；有效期为 60 秒。
4. 连接成功后返回展示台。以后启动应用时恢复已保存的配对；可在设置中重试、改名或忘记来源。

四位码通过 SPAKE2 建立信任，随后使用认证加密 TCP 会话。配对秘密留在 Rust 与系统安全存储中。Android 重启后需要重新收到快照；iOS 默认可先展示最后同步的缓存。详情见 [连接协议](docs/architecture/protocol.md)。

## 从源码运行

所有命令在项目根目录执行。准备 Rust/Cargo、Python 3.11+、NSH 0.3.0 与目标平台工具链；本项目没有 Node.js 项目依赖。

```sh
nsh check
nsh run --target macos
```

桌面目标还可选择 `windows` 或 `linux`。移动端使用项目包装器，以包含系统声明、应用接线与签名配置。

```sh
python3 tools/build/package_app.py --target macos --out output/packages/new-build
python3 tools/build/package_app.py --target android --out output/packages/new-build
python3 tools/build/package_app.py --target ios --with-watch --out output/packages/new-build
```

同一输出根目录可放不同目标，已有目标包不会被覆盖。iOS 默认包含 WidgetKit 扩展并生成模拟器应用；`--ios-device` 构建待签名真机应用，`--with-watch` 再包含 Watch。签名和安装步骤见 [移动使用说明](docs/mobile/usage.md)。

## 测试

```sh
python3 tools/test.py
python3 tools/test.py --suite android
python3 tools/test.py --suite watch
```

默认套件覆盖 Rust、打包、凭据边界与 NSH 界面回归。像素与真实协议联调可分别选择 `--suite visual`、`--suite integration`，完整条件见 [测试说明](docs/development/test.md)。NSH 测试与打包共用 `.nsh/`，应串行运行。

## 项目结构

| 位置 | 内容 |
| --- | --- |
| `App.tsx`、`src/app/` | 共享入口、导航与运行时 |
| `src/features/`、`src/shared/` | 展示台、用量、Hooks、连接、设置与公开契约 |
| `src/native/` | Rust 连接 owner、凭据、生命周期、图片与 iPhone 缓存同步 |
| `platform/watchos/` | SwiftUI Watch 应用与对应测试 |
| `platform/ios/`、`crates/widget-core/` | WidgetKit 界面、系统刷新接线与纯 Rust 摘要／HTTP 核心 |
| `src/tests/` | 界面、打包、Android 入口、像素测试与协议夹具 |
| `tools/` | 构建脚本与统一测试入口 |
| `signing/` | 移动身份配置和本机签名材料 |
| `docs/`、`.archify/` | 文档、来源记录与可交互架构图 |
| `.nsh/`、`target/`、`output/` | 生成工程、编译缓存、包与验证结果 |

五个平台共用一个 NSH TSX/Rust 应用，Watch 使用独立 SwiftUI 工程。TSX 只处理布局与交互；Rust 的 Shell 控制器处理启动、轮询、设置保存，并驱动发现、配对、会话和资源。继续阅读 [应用架构](docs/architecture/overview.md)、[代码约定](docs/development/code-style.md)、[NSH 来源](docs/provenance/nsh-runtime.md) 与 [配对库来源](docs/provenance/pairing-core.md)。
