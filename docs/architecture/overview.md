# 应用架构

Windows、macOS、Linux、Android 与 iOS/iPadOS 共用 `App.tsx` 和同一份 NSH TSX/Rust 实现。Apple Watch 是独立 SwiftUI 配套应用，接收 iPhone 已生成的公开缓存。

iOS WidgetKit 扩展为主屏幕、锁屏和待机显示提供系统界面。`crates/widget-core/` 复用当前快照与展示规则，执行共享摘要、租约和独立只读 HTTP 协议；`platform/ios/` 执行系统容器、网络传输、时间线刷新与绘制。下方交互图覆盖主应用、小组件与 Watch 三条通道。

[打开交互架构图](../../.archify/architecture-lokimetris-20261028-105048/lokimetris.html) · [连接协议](protocol.md) · [Watch 数据契约](watch-companion.md)

## 从入口到展示台

1. `App.tsx` 将 `UiLifecycle` 注入 `createApp`（连接由 Rust 的 `Shell` 驱动），装配展示台、设置与导航。
2. `src/app/runtime.ts` 调用 Rust `Shell.start` 恢复身份和偏好并创建连接实例，之后每 250ms 调用 `Shell.tick` 读取公开投影；TS 只做 UI 时钟和信号赋值。
3. `src/native/shell.rs` 是 Rust 控制器：启动、轮询、保存名称/面板、失败回滚和命令转发都在这里。它内部调用 `src/native/link.rs`（把连接实例绑定到 NSH `System.locals` 和应用私有目录，VM 关闭时释放）和设置用例。连接 owner 串行处理发现、配对、认证、快照与资源事件。
4. Rust 校验桌面输入并生成公开快照。TSX 根据窗口尺寸、面板偏好和导航状态渲染，纯展示规则通过 `#[module]` 类型化的 `Rules` 方法（`src/native/rules.rs`）直接调用。`Shell`、`ImageFrames` 同样是类型化的 `#[module]`，收发的面板、视图、快照、卡片投影等结构定义在 `src/shared/contracts/models.rs`（`#[derive(JsObject)]`），TS 类型由 `nsh check` 据此生成到 `.nsh/types`，TSX 不再手写这些类型，也不拼装或解析 JSON。

## 目录与职责

| 位置 | 职责 |
| --- | --- |
| `src/app/` | 应用工厂、导航与运行时（运行时只有 UI 时钟和信号赋值） |
| `src/features/dashboard/` | 展示台布局与详情装配 |
| `src/features/usage/` | Token、额度、排序和格式化规则 |
| `src/features/hooks/` | Hooks 卡片、分页、状态、正文和图片详情 |
| `src/features/connection/` | 公开连接视图、快照投影、设备身份与名称规则 |
| `src/features/settings/` | 设备与面板设置界面，以及身份恢复、名称确认和面板持久化用例 |
| `src/shared/` | Rust 公开数据形状（`contracts/models.rs`）、共用规则与 TS 共用 UI |
| `src/native/shell.rs`、`src/native/rules.rs` | 对 TSX 暴露的类型化 `#[module]`：控制器 `Shell`、展示规则 `Rules` |
| `src/native/fixture/` | 仅调试构建的测试夹具连接，按存储键扮演桌面端；产品构建不含 |
| `src/native/connection/` | 连接 owner、发现、认证会话与验证后的资源缓存 |
| `src/native/credentials/` | Keychain、Credential Manager、Secret Service 与 Android 密钥保护存储 |
| `src/native/lifecycle/`、`src/native/bootstrap/`（平台文件多时放入 `android/`、`ios/` 子目录） | 系统前台事实、身份初始化、JNI 与 UIKit 接线 |
| `src/native/android/` | Android Application／Activity 与电视确认键转换 |
| `src/native/image/` | 有界 GIF 解码、异步帧准备和 PNG 帧数据 |
| `src/native/companion/` | iPhone 公开缓存与进程级 WatchConnectivity 投递 |
| `platform/watchos/` | SwiftUI 界面、手表通信与本地缓存 |
| `platform/ios/`、`crates/widget-core/` | WidgetKit 五种尺寸、共享摘要、只读 HTTP 与系统刷新 |
| `src/tests/`、`platform/watchos/Tests/` | 共享应用与平台行为回归 |

## 状态归属

| 状态 | 维护者 | 边界 |
| --- | --- | --- |
| 页码、草稿、窗口尺寸、导航、详情和动画时钟 | TSX | 界面局部状态 |
| 身份恢复、名称保存、面板保存和失败处理 | Rust `Shell` 与设置用例 | 普通偏好通过 NSH Storage 保存 |
| 配对、会话序号、心跳、可信快照和资源接收 | Rust 连接 owner | 同一事件循环串行修改 |
| 配对秘密与身份安全记录 | Rust 与系统安全存储 | 秘密不进入 TSX 或普通 Storage；设备 UUID 和名称可公开 |
| Watch 快照、revision、退役 stream 与图片 | iPhone 发布器／Watch 缓存各自维护 | 跨设备传副本，不共享目录 |

feature 通过公开入口互相依赖，不导入 app；shared 不导入 feature 或 app。TS 类型由 Rust 数据形状生成，Rust 执行规则。目录组织与导入约定见 [代码约定](../development/code-style.md)。

## 生命周期、缓存与图片

移动连接根据真实系统前台事实运行，未知状态不视为前台。Android 通过 JNI 获取 Application，读取进程可见性、屏幕和锁屏状态，iOS 通过 UIKit scene 通知推进状态；移动 owner 不接受 JS 伪造的前台许可。退到后台后暂停桌面连接。

Android 配置 `keep_snapshot = false`，重启后等待新快照；iOS 默认恢复最后接收的缓存，显示原同步时间。受验证的图片资源使用私有磁盘缓存；忘记或切换来源时清理相应状态。

GIF 在 Rust 后台工作线程解码为完整画布 PNG 帧。TSX 播放时钟选择帧，官方 NSH `Image` 显示静态图；后台、离开当前页或减少动态效果时暂停。独立 Watch 使用自己的缓存与播放实现。

小组件通过 App Group 读取最近摘要。认证会话收到 `widgetHttpV1` 与只读端点后，只在原生私有路径记录权限，以实际已认证 TCP 对端为主机地址；端点和密钥不进入 TSX 或 Watch 公共投影。收到认证快照时发布、心跳时续租，断线停止续租并保留摘要；切换或忘记来源撤销相应配置。跨进程锁和请求代次避免旧 HTTP 响应覆盖新来源。WidgetKit 时间线与 BGAppRefreshTask 都申请约 15 分钟后再次刷新，实际执行时机由 iOS 决定。Swift 只经 `loki_core_call` 调用 `store.load`、`view.describe`、`widget.begin/finish` 等少量操作；主应用连接 owner 直接使用 widget-core 的 Rust API。

## 构建与源码证据

`nsh.json` 声明应用信息，`src/lib.rs` 注册 Rust 模块，官方 NSH 生成 `.nsh/` runner。`tools/build/` 补齐移动权限、应用接线、平台身份与签名，并校验最终包格式。独立 Watch 工程通过包装器与 iPhone 组成配套归档。

| 关系 | 源码入口 |
| --- | --- |
| UI 与原生模块注入 | [App.tsx](../../App.tsx)、[运行时](../../src/app/runtime.ts)、[模块注册](../../src/lib.rs)、[公开数据形状](../../src/shared/contracts/models.rs) |
| VM 到连接 owner | [Link 操作](../../src/native/link.rs)、[连接实例](../../src/native/connection/mod.rs)、[事件循环](../../src/native/connection/owner.rs) |
| 规则、控制器与设置存储 | [Rules](../../src/native/rules.rs)、[Shell](../../src/native/shell.rs)、[设置用例](../../src/features/settings/mod.rs)、[测试夹具](../../src/native/fixture/mod.rs) |
| 移动前台与缓存差异 | [生命周期推进](../../src/native/connection/app_visibility.rs)、[Android 初始化](../../src/native/bootstrap/android/mod.rs) |
| 资源验证与播放 | [资源接收](../../src/native/connection/assets.rs)、[图片模块](../../src/native/image/mod.rs)、[界面图片](../../src/shared/ui/image.tsx) |
| 小组件通道 | [摘要发布与撤销](../../src/native/connection/widget.rs)、[共享存储](../../crates/widget-core/src/store.rs)、[Swift 入口](../../crates/widget-core/src/ffi.rs)、[系统调度](../../platform/ios/bridge/WidgetHostBridge.swift) |
| iPhone 到 Watch | [缓存发布](../../src/native/companion/mod.rs)、[系统投递](../../src/native/companion/ios/session.rs)、[手表缓存](../../platform/watchos/Sources/SnapshotCache.swift) |

交互图由 archify 的 architecture 模式生成，[candidate.json](../../.archify/architecture-lokimetris-20261028-105048/candidate.json) 保存组件、关系与源码行号，证据固定在本仓库的一次提交。生成命令与检查结果见图目录的 [维护说明](../../.archify/architecture-lokimetris-20261028-105048/README.md)。

开发命令见 [构建说明](../development/build.md)，自动测试与设备验收边界见 [测试说明](../development/test.md)。
