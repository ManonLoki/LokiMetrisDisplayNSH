# 测试

从项目根目录执行。统一入口是 `tools/test.py`，使用项目现有的 Rust、NSH 和平台工具链；它不会安装或升级依赖。

## 日常回归

```sh
python3 tools/test.py
```

默认 `unit` 套件依次执行：

1. `cargo fmt --all --check`、Clippy 和当前宿主的 Rust 全目标测试。
2. Python 打包回归，检查 Android Manifest、官方 Java 字节码合并、移动身份、Apple 签名、Watch 嵌入和桌面包格式。
3. POSIX 上的 Linux 凭据适配回归，使用隔离的 `secret-tool` 替身验证大凭据、回滚和进程超时。
4. macOS 上编译并执行 Widget 后台执行器与取消／提交竞态的 Swift 回归。
5. `nsh check` 及四个 NSH 用例文件，检查应用交互、移动偏好迁移、安全区、电视遥控和图片播放时钟。

Rust workspace 测试同时覆盖主应用和 `crates/widget-core/`，负责格式化、快照校验、配对认证、缓存、GIF 解码、生命周期和设置状态机；小组件另覆盖租约、读取凭据、加密 HTTP、来源撤销及旧响应竞态。NSH 测试负责页面和交互，避免重复穷举 Rust 已覆盖的纯业务规则。

| 测试位置 | 验证内容 |
| --- | --- |
| `src/**/**_test.rs` | 同模块生产实现的 Rust 回归 |
| `src/tests/packaging/` | 构建与签名工具的输入、输出和失败行为 |
| `src/tests/desktop/app.test.tsx` | 导航、配对阶段、设置持久化、缓存、翻页和详情 |
| `src/tests/desktop/app-recovery.test.tsx` | 缓存快照不改写同步时间、损坏的持久化数据、输入中的名称与已移除 Hook 详情 |
| `src/tests/desktop/mobile-app.test.tsx` | 移动身份与偏好迁移、安全区和遥控输入 |
| `src/tests/desktop/image-playback.test.ts` | 播放时钟、暂停恢复、有限循环和独立播放头 |
| `src/native/fixture/` | 仅调试构建的 Rust 夹具连接：按存储键（`loki.inject`、`test.staticView`）扮演桌面端的配对、快照、中断和固定投影，时间全部由调用方传入虚拟时钟；产品构建不包含它 |

可直接运行某组测试：

```sh
cargo test --workspace --all-targets --locked
python3 -m unittest discover -s src/tests/packaging -p 'test_*.py'
python3 src/native/credentials/test_linux.py
nsh test src/tests/desktop/app.test.tsx
nsh test src/tests/desktop/mobile-app.test.tsx
nsh test src/tests/desktop/image-playback.test.ts
```

## 平台与像素回归

| 套件 | 命令 | 环境与范围 |
| --- | --- | --- |
| Android | `python3 tools/test.py --suite android` | JDK；在 JVM 中执行生产 Activity，Android／NSH API 使用替身 |
| iOS 小组件后台执行器 | `python3 tools/test.py --suite ios` | macOS、Swift；取消早于启动、等待取消清理、只完成一次；默认 unit 亦包含 |
| Watch | `python3 tools/test.py --suite watch` | macOS、Swift/Xcode；编译实际缓存、展示和图片实现 |
| 原生像素 | `python3 tools/test.py --suite visual` | macOS 原生 NSH 宿主、Swift；验证卡片动效门控和 GIF 显示帧 |
| 协议联调 | `python3 tools/test.py --suite integration` | macOS；启动隔离协议夹具，配对、传输 GIF 并检查原生显示帧 |

结果默认写入 `output/tests/<套件>/`。`--output` 改变结果根目录，入口仍会追加套件名称：

```sh
python3 tools/test.py --suite watch --output output/my-run
python3 src/tests/visual/run.py --suite cards --output output/cards
python3 src/tests/visual/run.py --suite images --output output/images
```

Watch 可额外读取 Rust 生成的 `manifest.json`、`clear.json` 和哈希命名图片，验证跨语言互通：

```sh
python3 platform/watchos/Tests/run_tests.py --interop <夹具目录> --report output/watch/cache.json
```

Linux 与 Windows 的原生 NSH 界面测试需在对应系统运行：

```sh
nsh test src/tests/desktop/app.test.tsx --target linux
nsh test src/tests/desktop/app.test.tsx --target windows
```

## 设备验收

NSH 检查、测试和打包共用 `.nsh/` 工作区，应串行运行。Rust 测试只编译当前宿主支持的条件分支；无界面 NSH 测试不证明真实像素、系统权限、签名或设备安装。JVM 替身不证明 Android 安装运行，凭据替身不证明真实 Linux Secret Service。

按 [构建说明](build.md) 安装后，验证局域网发现、四位码配对、认证快照、图片、设置保存、前后台恢复和进程冷启动。使用独立测试来源，保留应用标识与签名身份；磁盘缓存不能作为收到新认证快照的证据。

Watch 的系统安装、真实 WatchConnectivity 文件交付和后台投递，需要配对的 iPhone／Watch。详见 [Watch 架构](../architecture/watch-companion.md)。
