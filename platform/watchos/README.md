# Apple Watch 配套应用

这是 NSH iPhone 应用的 SwiftUI 配套工程，最低支持 watchOS 10。手表从 WatchConnectivity 接收公开快照与图片，保存并显示自己的缓存；配对、桌面连接和秘密凭据由 iPhone 上的共享 Rust 连接负责。

## 界面与图片

主界面横向切换 Hooks、Token、账户额度，首次显示 Token，并保存最后选择的页面。Hooks 使用纵向卡片，详情通过 sheet 展示。Token 同时提供紧凑值与精确值，额度展示剩余条和套餐信息。

图片在后台解码，来源上限为 32 MiB，最多 512 帧；缩略图最长边 256px，全部解码帧合计预算为 2 MiB。仅当前可见 Hook 卡片播放，打开详情、离开页面、进入后台、Always On 和 Reduce Motion 都会影响相应播放时钟。超限或无效图片显示占位，不阻塞页面。

## 源码职责

| 文件 | 职责 |
| --- | --- |
| `Sources/LokiMetrisWatchApp.swift`、`Sources/WatchRuntime.swift`、`Sources/WatchModel.swift` | 进程启动、模型通知、前台恢复和后台任务 |
| `Sources/WatchSession.swift`、`Sources/BackgroundDelivery.swift` | WatchConnectivity 回调、串行写入和一次性完成 |
| `Sources/Contract.swift`、`Sources/SnapshotCache.swift` | v1 契约、顺序校验、文件验证和冷启动恢复 |
| `Sources/ContentView.swift`、`Sources/HookDetailView.swift` | 三页面与 Hook 详情 |
| `Sources/WatchImageAsset.swift`、`Sources/WatchHookImage.swift` | 有界解码与可见播放 |
| `Sources/AmountText.swift`、`Sources/DisplayNames.swift`、`Sources/BehaviorStyle.swift` | 精确数字、Agent 与状态名称（与 Rust 共用 `src/tests/fixtures/display-names.json` 校验）、状态配色 |

`WatchRuntime` 在 App 初始化时创建缓存及会话，包括纯后台启动。`.backgroundTask(.watchConnectivity)` 等待系统待收内容与串行磁盘队列，最多等待 20 秒，取消和正常完成共用一次性 continuation。

快照经 `updateApplicationContext` 的 `lokiCompanion` Data 发送，图片通过 `transferFile` 发送。文件回调返回前先移动临时文件。只有 context 能授权新 `streamId`；同一流的十进制 revision 单调递增，同 revision 仅接受相同内容。清空清单和已退役流持久保存，防止迟到文件恢复已忘记的来源。

图片核对摘要、MIME 签名和长度后才公开本地 URL。提前到达的图片有界暂存，稍后与清单匹配；仍被新清单引用的不可变图片允许完成旧 revision 的传输。冷启动保留实际 iPhone 接收时间，缓存超过五分钟显示可能过期。完整协议与两端职责见 [Watch 架构](../../docs/architecture/watch-companion.md)。

## 构建与验证

```sh
xcodebuild -project platform/watchos/LokiMetrisWatch.xcodeproj \
  -scheme LokiMetrisWatch -configuration Debug -sdk watchsimulator \
  -destination 'generic/platform=watchOS Simulator' \
  -derivedDataPath output/watch-build CODE_SIGNING_ALLOWED=NO build
python3 tools/test.py --suite watch
```

target、共享 scheme 和产物分别为 `LokiMetrisWatch`、`LokiMetrisWatch`、`LokiMetrisWatch.app`。bundle ID 为 `$(LOKI_COMPANION_BUNDLE_IDENTIFIER).watchkitapp`，默认 companion ID 与现有 iPhone 标识一致。组合包必须对齐版本、build、团队、标识和 provisioning；单独构建 Watch 不会生成 iPhone 组合归档，正式入口见 [构建说明](../../docs/development/build.md)。

本地测试在 macOS 编译实际 Foundation/CryptoKit 缓存、展示和图片实现，覆盖清空、乱序图片、revision、完整 UInt64、冷恢复与后台取消。已有 Rust 导出夹具时可运行：

```sh
python3 platform/watchos/Tests/run_tests.py --interop <包含manifest.json与assets的目录>
```

Debug 可使用规范 UUID 的 `LOKI_WATCH_VALIDATION_ID` 隔离模拟器缓存，用 `LOKI_WATCH_FIXTURE_DIR` 指向含 `manifest.json` 与 `assets/<hash>` 的绝对目录。`LOKI_WATCH_DISABLE_TRANSPORT=1` 仅在有效 validation ID 下关闭传输。Release 不包含这些注入路径。

本地套件和模拟器图像验证缓存及界面，跨设备文件传输、实际安装及系统后台投递仍需配对真机验证，范围见 [测试说明](../../docs/development/test.md)。
