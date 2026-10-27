# 开发工具

此目录只维护构建和统一测试入口。应用实现与行为测试分别归入 `src/` 和 `platform/`，生成工程位于 `.nsh/`，包、日志和测试结果写入 `output/`。

| 入口 | 职责 |
| --- | --- |
| `build/package_app.py` | 五平台统一打包；移动标识和 Watch 配套选项 |
| `build/package_desktop.py` | 对应桌面系统上的原生包与可执行格式核验 |
| `build/package_windows_cross.py` | macOS 上的 Windows x86-64 MSVC 包与 ZIP |
| `build/package_apple_companion.py` | 用预链接 iPhone `.app` 装配 Widget 与可选 Watch 归档 |
| `build/sign_apple_device.py` | 验证手机／Widget 共享组与各自 profiles，生成签名计划或副本 |
| `build/build_android_bridge.py` | 对照官方 NSH API 编译应用 Java 接线 |
| `build/android_manifest.py` | Android 二进制清单、应用入口和局域网权限 |
| `build/package_android.py` | APK Java/D8 组装、JNI 核验、对齐和签名 |
| `test.py` | 按套件顺序执行当前项目测试 |

```sh
python3 tools/test.py
python3 tools/test.py --suite watch
python3 tools/build/package_app.py --target android --out output/packages/build-001
```

Python 工具使用标准库，复用已有 NSH、编译器与 SDK。Android 私钥配置位于 `signing/android/`，Apple 身份位于 `signing/apple/`。输出目标必须是新目录；包格式检查与设备运行验收分别记录。

NSH 测试和打包共用 `.nsh/`，应串行执行。完整参数和验证范围见 [构建说明](../docs/development/build.md) 与 [测试说明](../docs/development/test.md)。
