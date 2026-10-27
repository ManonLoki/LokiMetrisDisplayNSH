# 平台原生补充实现

`platform/<平台>/` 保存共享 NSH 应用之外的原生工程、资源和对应测试。当前仅有 [watchOS](watchos/README.md)：随 iPhone 打包的 SwiftUI 配套应用，接收公开快照及图片并维护自己的离线缓存。

共享界面、业务规则和 NSH 注册模块位于 `src/`；Rust 平台 API 适配也按能力放在 `src/native/`。未来需要独立原生工程的补充实现放入对应平台子目录。

| 位置 | 内容 |
| --- | --- |
| `watchos/Sources/` | SwiftUI 页面、WatchConnectivity、协议与本地缓存 |
| `watchos/Assets.xcassets/`、`watchos/Info.plist` | 图标和应用元数据 |
| `watchos/LokiMetrisWatch.xcodeproj/` | target、scheme、身份和部署配置 |
| `watchos/Tests/` | 缓存、展示、图片及 Rust 数据互操作测试 |

NSH 生成工程留在 `.nsh/`，构建与测试入口位于 `tools/`，结果写入 `output/`。工程归属见 [架构总览](../docs/architecture/overview.md)，使用方式见 [构建说明](../docs/development/build.md) 和 [测试说明](../docs/development/test.md)。
