# NSH 运行时来源与应用适配

项目直接使用 crates.io 的 NSH 0.3.0，依赖版本、来源与校验值由 `Cargo.lock` 固定。根 `Cargo.toml` 声明 `nsh-core`，不保留修改后的 SDK，也不使用 Cargo patch 或本机 SDK 路径覆盖。Apache-2.0 许可证保留在 [nsh-LICENSE-APACHE](nsh-LICENSE-APACHE)。

## 官方运行时与应用源码

NSH 提供 TSX 组件、布局、渐变、动效、原生模块注册与生成 runner。系统接线、连接、设置规则和 GIF 播放属于本项目：

| 位置 | 应用职责 |
| --- | --- |
| `src/lib.rs`、`src/native/link.rs` | 注册本项目模块，连接 TSX 与 Rust |
| `src/native/lifecycle/` | 平台可见状态、iOS 启动与 scene 通知 |
| `src/native/companion/` | Watch 公开缓存与 WatchConnectivity 投递 |
| `src/native/bootstrap/android/context.rs` | 接收应用自己的 JavaVM/application context |
| `src/native/android/java/` | 自有 Application/Activity、电视确认键与官方 NshView 接线 |
| `src/native/image/` | 有界 GIF/LZW 解码、画布合成与异步 PNG 帧准备 |
| `src/shared/ui/image*.ts*` | 标准 Image 显示帧，各控件独立管理播放时钟 |

iOS 在 Rust registry 入口注册观察者，收到应用启动通知后再访问 UIApplication 并启动 Watch 会话。前后台来自公开 UIScene 通知，聚合多窗口可见状态；短暂 inactive 不直接视为隐藏。Android 从 Application.onCreate 初始化应用上下文，不保存 Activity；可见性结合进程、亮屏及锁屏状态判断。

## Windows 重绘兼容

`src/shared/ui/windows-repaint.ts` 在 Windows 前台为全窗口背景绑定原生动画参数，使背景每帧参与重绘，绕开多个局部更新区域留下旧像素的候选路径。参数用于画布外的绘制操作，外观保持不变；静态背景命令仍可缓存，不逐帧执行 JS。切到后台或释放组件时停止，其他平台不启用。此处理增加前台重绘面积，是针对间歇花屏的应用侧兼容措施，上游根因尚未确认，未修改 SDK 或显卡设置。

生命周期回归见 `src/tests/desktop/windows-repaint.test.tsx`。无界面测试不能证明 Windows 实际呈现，需要额外检查真机动画、页面切换、缩放及恢复。

## Android SDK 输入

构建器优先读取已安装官方 `nsh.aar`，核对 SDK Java 类未被覆盖，只加入应用桥接类。没有 AAR 时，依据锁文件校验缓存中的原始 `.crate`，核对 Java 文件清单和内容后编译。两条路径都不修改 Cargo 缓存或保存另一份 SDK。

包标识、清单权限、应用入口和签名是本项目的组装职责。构建脚本位于 `tools/build/`，参数及平台要求见 [构建说明](../development/build.md)。

## GIF 与播放边界

解码保存原始画布、透明度、局部调色板、处置方式、交错和循环次数。专用工作线程逐帧合成并编码 PNG，不累计全部 RGBA 画布。当前边界由 `src/native/image/mod.rs` 固定：

| 内容 | 上限 |
| --- | --- |
| 原始来源 | 32 MiB |
| 画布 | 最长边 4096，最多 16 Mi 像素 |
| 动画 | 512 帧，累计 128 Mi 合成像素 |
| 编码清单 | 24 MiB |
| 共享缓存 | 32 MiB，最多 4 项 |

标准 Image 显示静态 PNG，应用时钟决定切帧；控件隐藏、后台或减少动态效果时暂停，恢复时从当前帧继续，有限循环停在末帧。输入、排队及输出分别有界，错误显式返回。测试入口和像素验证范围见 [测试说明](../development/test.md)。
