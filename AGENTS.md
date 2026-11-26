# Rust 结构检查

修改 Rust 模块边界、跨模块依赖或公开接口后，从仓库根目录运行 `deslop-please` 并检查报告。它按当前 Cargo 目标和特性扫描工作区。需要核对其他平台时，使用 `--target <triple>`。

首次安装时，使用当前 Rust 工具链的 Rust Analyzer，并通过 Cargo 全局安装固定版本的检查器。

```sh
rustup component add rust-analyzer
cargo install --locked --version 0.1.6 deslop-please
```

- 遇到 `module-cycle` 或 `tangle`，先确认模块的职责和依赖方向，再调整边界。
- 遇到 `field-leak` 或 `shallow-module`，检查调用方的需求，用少量面向调用方的操作封装状态和实现。
- 涉及多个模块的结构调整时，用简图说明当前与拟议的依赖关系。

[deslop-please 上游文档](https://github.com/deterministic-machines/deslop-please)
