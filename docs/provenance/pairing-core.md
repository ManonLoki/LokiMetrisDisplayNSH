# 共用配对库来源

展示端与电脑采集客户端共用 `loki_metris_pairing` Rust 库。五平台 NSH 应用使用同一个 `src/native/connection/` owner，直接调用 `PairingWindow`、`ReceiverSession`、`DeviceSession` 和 `SecureChannel`；配对秘密只保存在 Rust 与系统安全存储之间。

| 项目 | 固定来源 |
| --- | --- |
| 上游 | [ManonLoki/LokiMetis](https://github.com/ManonLoki/LokiMetis) |
| Git revision | `28b73e602060d750197b2b7384bdb8f3e30f107f` |
| 该提交 workspace 版本 | `0.9.3` |
| 套件 | SPAKE2（`spake2 =0.4.0`）与 Ed25519 |

根 `Cargo.toml` 固定 Git revision，`Cargo.lock` 固定解析结果，[pairing-core.json](pairing-core.json) 保存来源记录。项目不保存协议源码副本，不依赖某台机器上的客户端目录。首次构建由 Cargo 获取固定提交，离线构建须预先具备对应 Git/crate 缓存。

## 升级与验证

升级时同步修改根依赖 revision 与锁文件，核对 package ID 唯一、凭据兼容、配对、认证恢复和会话加密。五平台 runner 引入同一个本地模块，仍应分别验证打包与实际网络配对；普通 NSH Storage 不保存秘密，不用本机路径覆盖混入另一版库。入口见 [测试说明](../development/test.md)，运行链路见 [连接协议](../architecture/protocol.md)。

原有上游许可证原样保留：[English](pairing-license.en.md)、[简体中文](pairing-license.zh-CN.md)。NSH 来源与应用适配见 [运行时说明](nsh-runtime.md)。

[mobile-origin.json](mobile-origin.json) 记录旧原生工程的导入路径、哈希和迁移历史；其中已退役的目录和工具是历史证据，当前维护结构以 [架构总览](../architecture/overview.md) 为准。
