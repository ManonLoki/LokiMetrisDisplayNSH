# NSH 运行时来源与应用适配

项目使用 crates.io 发布的官方 NSH 0.7.1，版本和来源由 `Cargo.toml`、`Cargo.lock` 固定。NSH CLI 与 Cargo 运行时使用同一版本，CLI 生成的 `.nsh/` 不进入版本控制。Apache-2.0 许可证保留在 [nsh-LICENSE-APACHE](nsh-LICENSE-APACHE)。

[NSH 0.7.1 发布说明](https://github.com/longbridge/nsh/releases/tag/v0.7.1)

## 0.7.1 迁移

- `nsh.json` 声明 Android 的局域网多播权限和明文传输设置。应用组装器核对 NSH 写入的声明，再组装自定义入口和小组件。生成的 `output/` 目录通过 NSH 的 `watch.ignore` 从开发监听中排除。
- `appInfo().version` 提供局域网设备握手所用的应用版本。平台引导直接构造 Rust 连接配置，iOS 的旧设备名称和钥匙串标识继续从系统读取。
- `Image` 根据布局尺寸更新解码，图片层随控件存活。GIF 帧结果以 NSH 的 `JsObject` 返回，PNG 编码器和官方宿主共用 0.18.1，移除了单独的 0.17 版。

[NSH 0.7 Android 打包选项](https://github.com/longbridge/nsh/blob/v0.7.1/website/docs/packaging.md#android) · [NSH Windows 图片解码](https://github.com/longbridge/nsh/blob/v0.7.1/crates/nsh-platform-windows/src/image.rs)

## 0.5.0 迁移

0.5.0 的 iOS crate 通过独立发布的 `nsh-platform-apple` 引用共享语音实现，项目恢复使用官方模板的 crates.io 来源。固定的 iOS 应用声明由 `nsh.json` 的 `ios.infoPlist` 交给 NSH 打包。

[NSH 0.5.0 发布说明](https://github.com/longbridge/nsh/releases/tag/v0.5.0)

## 官方运行时与应用源码

NSH 提供 TSX 组件、布局、渐变、动效、原生模块注册与生成 runner。系统接线、连接、设置规则和 GIF 播放属于本项目：

| 位置 | 应用职责 |
| --- | --- |
| `src/lib.rs`、`src/native/link.rs` | 注册本项目模块，连接 TSX 与 Rust |
| `src/native/lifecycle/` | iOS 启动通知（Watch 会话启动） |
| `src/native/companion/` | Watch 公开缓存与 WatchConnectivity 投递 |
| `src/native/bootstrap/android/context.rs` | 接收应用自己的 JavaVM/application context |
| `src/native/android/java/` | 自有 Application/Activity、电视确认键与官方 NshView 接线 |
| `src/native/image/` | 有界 GIF/LZW 解码、画布合成与异步 PNG 帧准备 |
| `src/shared/ui/image*.ts*` | 标准 Image 显示帧，各控件独立管理播放时钟 |

UI 通过 NSH 的 `lifecycle()` 和 `platform.idiom` 获取前后台与设备形态，移动端把前后台经 `active` 命令交给连接 owner，只有进入后台才暂停，短暂失焦仍视为可见。Android 的连接也使用 `platform.idiom`，按现有协议只上报手机和平板。iOS 同样用 `platform.idiom`（即 UIKit 的设备类型），遇到手机和平板之外的类型时省略该消息。Hooks 面板由 `nsh/base` 的 Carousel 管理吸附和翻页。Android 在 NSH 视图首次布局后加载 TSX bundle，让设备形态在模块加载前可用。

iOS 在 Rust registry 入口注册观察者，收到应用启动通知后再访问 UIApplication 并启动 Watch 会话；回到前台时由 `active` 命令唤醒 Watch 缓存重试。Android 从 Application.onCreate 初始化应用上下文，不保存 Activity；界面可见期间持有多播锁，随 `active` 命令获取与释放。

## Android SDK 输入

打包流程将生成的 `R.java` 传给桥接构建器。构建器从应用 `Cargo.lock` 读取 NSH 版本，依据生成 runner 的锁文件校验缓存中的官方 `.crate`，核对 Java 文件清单和内容后编译。通过 `--aar` 提供相同版本的官方嵌入 SDK 时，构建器在合并 Java 类时检查同名冲突。

当前 Android 打包器负责替换启动 Activity、接入进程级 Application 并生成小组件资源。0.7.1 的组件声明只能追加组件，官方包固定使用自己的启动 Activity，AAR 自定义 Application 会被拒绝。iOS 的 `extensions` 接收预先构建的扩展，当前 WidgetKit 与 Watch 继续由 Xcode 组合并检查共享授权。构建参数及平台要求见 [构建说明](../development/build.md)。

## GIF 与播放边界

解码保存原始画布、透明度、局部调色板、处置方式、交错和循环次数。专用工作线程逐帧合成并编码 PNG，不累计全部 RGBA 画布。当前边界由 `src/native/image/mod.rs` 固定：

| 内容 | 上限 |
| --- | --- |
| 原始来源 | 32 MiB |
| 画布 | 最长边 4096，最多 16 Mi 像素 |
| 动画 | 512 帧，累计 128 Mi 合成像素 |
| 编码清单 | 24 MiB |
| 共享缓存 | 32 MiB，最多 4 项 |

标准 Image 显示静态 PNG，应用时钟决定切帧。NSH 0.7.1 的原生 GIF 播放没有公开的逐帧暂停和续播控制。应用在控件隐藏、退到后台或启用减少动态效果时暂停，恢复时从当前帧继续，有限循环停在末帧。测试入口和像素验证范围见 [测试说明](../development/test.md)。

应用还负责局域网服务发现、认证会话及系统凭据存储。NSH 的 `network()` 提供网络状态，应用用 mDNS 或 Bonjour 找到可配对桌面。普通偏好由 NSH Storage 保存，配对材料使用系统凭据存储。配对协议见 [连接协议](../architecture/protocol.md)。
