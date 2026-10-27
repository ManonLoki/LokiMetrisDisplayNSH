# 构建与打包

主应用使用 NSH 0.3.0、共享 TSX 与 Rust，支持 macOS、Windows、Linux、Android 和 iOS/iPadOS。iOS 默认包含独立 WidgetKit 扩展，Apple Watch 通过选项配套打包。小组件的纯 Rust 核心只复用项目已锁定的依赖，无需 KMP。

所有命令在项目根目录执行。先准备已安装的 `nsh`、Rust/Cargo、Python 3.11+ 与目标平台工具链。脚本复用现有工具，Rust 依赖由 `Cargo.toml` 和 `Cargo.lock` 管理。

## 日常开发

```sh
nsh check
nsh run --target macos
python3 tools/test.py
```

根 `nsh.json` 的 `rust: "."` 使 NSH runner 引入本项目 Rust 模块。`.nsh/` 保存生成工程与缓存，`output/` 保存包、归档和验证结果。NSH 测试与打包共用 `.nsh/`，应串行执行。

## 统一打包

```sh
python3 tools/build/package_app.py --target macos --out output/packages/build-001
```

`--out` 指定结果根目录，脚本在其中创建目标子目录，拒绝覆盖已有目标。每次交付使用新的批次目录。

| 目标 | 结果子目录 | 平台要求 |
| --- | --- | --- |
| macOS | `macos/` | macOS |
| Linux | `linux/` | Linux |
| Windows | `windows/` | Windows / MSVC |
| Windows（macOS 交叉构建） | `windows-cross/` | cargo-xwin、Xcode/libclang、windres |
| Android | `android/` | JDK、Android SDK/NDK、原签名文件 |
| iOS 模拟器 | `ios/` | macOS、Xcode、对应 Rust target |
| iOS 真机 | `ios-device/` | 加 `--ios-device`，安装前完成开发签名 |

每个目标附带 `platform-verification.json`，记录包结构、可执行格式和组装检查，`runtimeVerified` 为 `false`。启动、配对和设备行为按 [测试说明](test.md) 另行验收。

## Android

设置 `ANDROID_HOME` 或 `ANDROID_SDK_ROOT`，准备 API 37 的 `android.jar` 与带 D8、zipalign、apksigner 的 build-tools。NSH 安装需包含 ARM64、x86-64 Android runtime。签名文件见 [Android 签名说明](../../signing/android/README.md)。

```sh
python3 tools/build/package_app.py --target android --out output/packages/build-android-001
adb -s <设备序列号> install -r <生成的APK路径>
```

包装器为两个 64 位 ABI 构建应用 runtime，核对 JNI 导出，保留官方 NSH Java 类，再编译自有 Application/Activity 接线，经 D8、对齐和原证书签名后生成 APK。清单补入网络、多播权限与应用入口。编译 SDK 与目标 SDK 分别核对；目前目标 SDK 达到 37 时会因缺少本地网络动态权限支持而拒绝出包。

移动标识可用 `--id` 覆盖。已有安装须沿用原标识与证书，才能覆盖升级并保留身份及配对。移动包使用项目入口完成组装。

## iOS / iPadOS、小组件与 Watch

默认构建包含 WidgetKit 扩展的模拟器 `.app`。包装器先构建纯 Rust 小组件静态库和 Swift 系统接线，再生成 NSH 主应用；`--with-watch` 增加 Watch 应用，由 Xcode 装配、嵌入和归档。

Widget 扩展沿用旧版 iOS 18.2 最低版本；主应用的部署版本由 SDK 和包装器配置确定。

```sh
python3 tools/build/package_app.py --target ios --with-watch --out output/packages/build-ios-001
xcrun simctl install <模拟器UDID> <生成的.app路径>
xcrun simctl launch <模拟器UDID> <包元数据中的applicationId>
```

包装器加入本地网络用途、`_loki-host._tcp` Bonjour 声明及后台刷新任务声明。手机与 Widget 共用 `LokiWidgetAppGroup`；模拟器 runner 的 `__TEXT,__entitlements` 携带匹配的 application-identifier 和共享组。身份值来自 [Apple 身份配置](../../signing/apple/README.md)，自定义 `--id` 时也必须有匹配配置。

```sh
python3 tools/build/package_app.py --target ios --ios-device --with-watch --out output/packages/build-device-001
```

真机模式生成待开发签名的设备 `.app`。最低系统版本由所选 SDK 和部署配置共同确定。归档包含组合 `.app`、`NSHCompanion.xcarchive` 和元数据；包装器核对 NSH 代码段、bundle、Widget／Watch 源码摘要、版本和 Mach-O 平台。真机签名时须提供 Widget profile 与手机、扩展共同获授权的原 App Group；已有组件升级还须保留 `.widgets` 扩展标识和 `LokiMetrisDisplay.Summary` kind。

签名工具默认写计划，加 `--execute` 后用已有证书及 profiles 创建签名副本。完整参数、安装命令和兼容规则见 [Apple 签名说明](../../signing/apple/README.md)。Watch 单独构建和缓存验证见 [Watch 工程](../../platform/watchos/README.md)。

## 桌面

```sh
python3 tools/build/package_app.py --target linux --out output/packages/build-linux-001
python3 tools/build/package_app.py --target windows --out output/packages/build-windows-001
```

原生打包在对应系统执行。macOS 上指定 Windows 时自动转交 `package_windows_cross.py`，使用 `cargo-xwin`、Xcode/libclang 和 `x86_64-w64-mingw32-windres` 构建真实 x86-64 MSVC PE，再重建 ZIP。结果仍需在 Windows 验证启动、WebView2、Credential Manager 和配对。Windows 包须保留可执行文件旁的 `nsh` 目录。

移动构建日志位于 `output/build/<目标>/package.log`。工具职责见 [tools](../../tools/README.md)，安装与使用见 [移动说明](../mobile/usage.md)。
