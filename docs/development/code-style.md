# 代码约定

共享 NSH 界面负责展示与交互，Rust 负责规则、设置用例、连接和平台适配。一个文件承担一个明确职责；第一方手写实现与测试每文件最多 500 行，按业务领域和资源归属拆分。

## 模块边界

| 位置 | 职责 |
| --- | --- |
| `App.tsx`、`src/app/` | 入口、装配、导航、设置用例与运行时连接 |
| `src/features/` | connection、usage、hooks、settings、dashboard 各业务 |
| `src/shared/` | 跨业务契约、领域规则和 UI 组件 |
| `src/native/` | NSH 导出、连接 owner、凭据、生命周期、图片及 Watch 同步 |
| `src/assets/` | 图标与品牌资源 |
| `platform/<平台>/` | 平台原生补充工程、资源和对应测试 |
| `src/tests/` | 跨模块宿主、构建、像素及协议联调测试 |
| `tools/build/`、`tools/test.py` | 构建和统一测试入口 |

feature 经公开 `index.ts` 供其他业务使用，不反向导入 app；shared 不依赖 feature 或 app。TSX 保存布局、导航、草稿、详情和动效状态；校验、展示规则、默认值和持久化决策放在所属 Rust 模块。TS 适配层只传参及解析结果。

## 状态与资源

- 配对、会话序号和图片缓存由一个 Rust 连接 owner 串行修改；系统适配执行平台 API，不复制业务规则。
- Token 在公开边界使用十进制字符串，Rust 校验完整 `u64`；等待期限用单调时钟，接收时间用墙钟并保留原值。
- 配对秘密只流经 Rust 与系统安全存储。普通 NSH Storage 保存公开设置；图片通过校验后才能发布本地 URI。
- 保存失败保留原状态。异步确认期间保留继续编辑的草稿，旧请求只回滚仍由自己占有的记录。
- 订阅、定时器和动画随所属作用域清理。平台回调遵守线程和对象生命周期；Watch 临时文件在回调返回前转入应用目录。

## 格式与测试

TypeScript 使用两格缩进和分号，Rust 使用 2024 edition 与 rustfmt。注释说明线程归属、存储提交、失败恢复、资源释放等不直观约束；修改行为时同步维护注释和文档。

Rust 单元测试放在所属模块旁的 `*_test.rs`，用私有测试子模块引用，不为测试扩大生产 API。Linux 凭据测试紧邻适配实现，Watch 测试位于 `platform/watchos/Tests/`，跨模块及构建测试放在 `src/tests/`。

新增测试验证可观察行为和边界条件，避免只匹配源码字符串或重复实现。日常入口为 `python3 tools/test.py`，平台与真机范围见 [测试说明](test.md)。架构和状态归属见 [架构总览](../architecture/overview.md)。
