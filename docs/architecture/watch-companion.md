# Apple Watch 配套架构

[应用架构](overview.md) · [局域网协议](protocol.md) · [Watch 工程](../../platform/watchos/README.md)

Watch 使用独立 SwiftUI 工程，随 NSH iPhone 应用组成配套产品。iPhone 从电脑接收认证数据，再向 Watch 投递公开缓存；Watch 不发现电脑、不保存配对密钥、不独立查询桌面。

## 数据流与所有权

```text
桌面认证快照 → iPhone Rust owner → companion-v1/manifest.json
                                     ↓ WatchConnectivity
                              Watch 本地快照与图片 → SwiftUI
```

iPhone 的连接 owner 在可信状态提交时调用 `src/native/companion/`。该模块复用 `src/features/connection/snapshot.rs` 的白名单投影，移除手机图片路径，将公开 manifest 原子写入：

```text
Library/Application Support/loki-link-v1/companion-v1/
├── manifest.json
└── assets/<SHA-256>
```

图片从已验证的私有缓存复制到 companion 目录。Watch 使用自己设备上的 `companion-cache-v1`，接收的是跨设备副本，两端不共用文件目录，也不依赖 App Group。

进程级 WatchConnectivity 适配器由 UIKit 启动通知安装，在独立工作线程中读取已落盘缓存。它不依赖 TSX 页面、`Link.poll` 或 VM，也不延长桌面连接 owner 的生命。手机退后台后仍暂停桌面连接，系统可以继续投递已排队的公开快照和图片。

## 同步契约

`manifest.json` 版本为 1，最大 48 KiB：

| 字段 | 内容 |
| --- | --- |
| `version` | 契约版本 `1` |
| `streamId` | 手机首次建立缓存时生成并持久化的 UUID |
| `revision` | 内容变化时持久递增的十进制 UInt64 字符串 |
| `sourceId` | 当前受信任桌面的 UUID；清空时为 `null` |
| `receivedAtMs` | 手机实际接收快照的毫秒时间；清空时为 `null` |
| `snapshot` | 用量、额度、Hooks 的公开投影；Token 数量保留十进制字符串；清空时为 `null` |
| `assets` | 引用图片的 `hash`、`mime`、`length`；清空时为空数组 |

`updateApplicationContext` 以 `lokiCompanion` 为键传递完整 JSON Data，系统合并更新时保留最新状态。图片通过 `transferFile` 传递，metadata 包含 `kind=asset`、`streamId`、`revision`、`hash`。本地缓存、待传输数量和重试次数均有边界；session 激活、重新前台或 Watch 请求最新缓存时可重试。

## 顺序、清空与图片

Watch 的 `SnapshotCache` 在串行边界内校验并落盘：

- 仅 application context 可以引入新 stream，替换的 stream 被持久标记为退役。
- 同一 stream 的旧 revision 不能覆盖新状态，等值 revision 必须内容一致。
- 切换或忘记来源时，手机写入递增 revision 的清空快照；Watch 先落盘清空状态，再清理资源。离线 Watch 要等系统投递后才能反映清空。
- 图片与快照允许乱序到达。较早 revision 的图片只可补充同一 stream 当前仍引用的相同 hash，图片本身不能恢复旧快照或改变来源。
- 图片须通过长度、MIME 魔数及 SHA-256 校验；早到图片有独立数量与空间限制。回调返回前先搬走 WatchConnectivity 的临时文件，再交给串行缓存处理。

## Watch 运行时与界面

工程位于 `platform/watchos/LokiMetrisWatch.xcodeproj`，只有一个 SwiftUI Watch App target。`WatchRuntime` 在 App 初始化时创建 `SnapshotCache` 和 `WatchSession`；`WatchModel` 订阅缓存变更，SwiftUI 显示 Token、Agent、额度、Hook、图片和最后同步时间。

`.backgroundTask(.watchConnectivity)` 等待 session 激活、`hasContentPending` 清空和本地队列落盘完成，支持取消与有界超时。Watch 前台可以请求手机重发已有缓存，此请求与后台接收都不会启动桌面查询。

没有数据时提示先在 iPhone 查询；离线时显示最后接收时间。当前范围不含表盘 complication、Watch 独立网络连接或手机后台持续查询。运行时实现见 `WatchRuntime.swift`、`WatchSession.swift`、`SnapshotCache.swift`。

## 配套归档与安装

Watch bundle ID 为 iPhone ID 追加 `.watchkitapp`，`WKCompanionAppBundleIdentifier` 指向 iPhone 应用；两者使用相同开发团队、版本和 build。Watch 依赖 companion 安装。

NSH CLI 生成 iPhone runner。`tools/build/package_apple_companion.py` 创建临时装配工程，接纳已链接的 NSH 可执行文件与资源，通过 Xcode target dependency、Watch 嵌入和 archive 生成配套产品。包装器核验实际 Mach-O 平台、companion 标识、版本及可执行代码段；iOS 与 watchOS 的真机／模拟器产物分别匹配，不能仅凭 arm64 架构混装。

安装 iPhone 配套应用后，由系统或 iPhone 的 Watch App 安装兼容 Watch 应用。模拟器 ad-hoc 签名不代表真机签名或可分发 IPA；构建、签名和归档步骤见 [构建说明](../development/build.md)。

## 验证边界

Rust 回归覆盖公开投影、UInt64 精度、revision 持久化、可信来源、清空和图片。Swift 回归使用 Rust 生成的 manifest、清空记录和 PNG，验证互通、乱序、资源校验、旧状态拒绝与冷启动恢复。

```sh
python3 tools/test.py --suite watch
```

模拟器可验证编译、UI 与离线缓存。`transferFile` 的真实投递须在已配对的 iPhone／Watch 上验收；模拟器注入图片只证明缓存接收与展示。参见 [Apple 文件传输要求](https://developer.apple.com/documentation/watchconnectivity/wcsession/transferfile(_:metadata:))。其余测试入口和设备要求见 [测试说明](../development/test.md)。
