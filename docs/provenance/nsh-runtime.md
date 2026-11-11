# NSH 运行时来源与应用适配

项目使用 crates.io 发布的官方 NSH 0.5.0，版本和来源由 `Cargo.toml`、`Cargo.lock` 固定。NSH CLI 与 Cargo 运行时使用同一版本，CLI 生成的 `.nsh/` 不进入版本控制。Apache-2.0 许可证保留在 [nsh-LICENSE-APACHE](nsh-LICENSE-APACHE)。

## 0.5.0 迁移

0.5.0 的 iOS crate 通过独立发布的 `nsh-platform-apple` 引用共享语音实现，项目恢复使用官方模板的 crates.io 来源。固定的 iOS 应用声明由 `nsh.json` 的 `ios.infoPlist` 交给 NSH 打包。

[NSH 0.5.0 发布说明](https://github.com/longbridge/nsh/releases/tag/v0.5.0)

## 官方运行时与应用源码

NSH 提供 TSX 组件、布局、渐变、动效、原生模块注册与生成 runner。系统接线、连接、设置规则和 GIF 播放属于本项目：

| 位置 | 应用职责 |
| --- | --- |
| `src/lib.rs`、`src/native/link.rs` | 注册本项目模块，连接 TSX 与 Rust |
| `src/native/lifecycle/` | iOS 连接可见性、启动与 scene 通知 |
| `src/native/companion/` | Watch 公开缓存与 WatchConnectivity 投递 |
| `src/native/bootstrap/android/context.rs` | 接收应用自己的 JavaVM/application context |
| `src/native/android/java/` | 自有 Application/Activity、电视确认键与官方 NshView 接线 |
| `src/native/image/` | 有界 GIF/LZW 解码、画布合成与异步 PNG 帧准备 |
| `src/shared/ui/image*.ts*` | 标准 Image 显示帧，各控件独立管理播放时钟 |

UI 通过 NSH 的 `lifecycle()` 和 `platform.idiom` 获取前后台与设备形态，Hooks 面板由 `nsh/base` 的 Carousel 管理吸附和翻页。Android 在 NSH 视图首次布局后启动应用，让设备形态在模块加载前可用。

iOS 在 Rust registry 入口注册观察者，收到应用启动通知后再访问 UIApplication 并启动 Watch 会话。前后台来自公开 UIScene 通知，聚合多窗口可见状态；短暂 inactive 不直接视为隐藏。Android 从 Application.onCreate 初始化应用上下文，不保存 Activity；可见性结合进程、亮屏及锁屏状态判断。

## Windows 重绘兼容

`src/shared/ui/windows-repaint.ts` 在 Windows 前台为全窗口背景绑定原生动画参数，使背景每帧参与重绘，绕开多个局部更新区域留下旧像素的候选路径。参数用于画布外的绘制操作，外观保持不变；静态背景命令仍可缓存，不逐帧执行 JS。切到后台或释放组件时停止，其他平台不启用。此处理增加前台重绘面积，是针对间歇花屏的应用侧兼容措施，上游根因尚未确认，未修改 SDK 或显卡设置。

生命周期回归见 `src/tests/desktop/windows-repaint.test.tsx`。无界面测试不能证明 Windows 实际呈现，需要额外检查真机动画、页面切换、缩放及恢复。

## Android SDK 输入

构建器默认依据生成 runner 的锁文件校验缓存中的官方 `.crate`，核对 Java 文件清单和内容后编译。单独构建 Java 桥时也可以用 `--aar` 显式提供 0.5.0 的官方嵌入 SDK，应用桥接类与 AAR 中的类合并时检查同名冲突。

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

标准 Image 显示静态 PNG，应用时钟决定切帧。NSH 0.5 的原生 GIF 播放没有公开的暂停和续播控制，当前实现继续在控件隐藏、应用退到后台或启用减少动态效果时暂停，恢复时从当前帧继续，有限循环停在末帧。输入、排队及输出分别有界，错误显式返回。测试入口和像素验证范围见 [测试说明](../development/test.md)。
