# 应用架构图维护

[打开交互图](lokimetris.html) · [应用架构](../../docs/architecture/overview.md)

本图使用 archify 3.2.0 的 architecture 模式，覆盖 NSH 主应用（TSX 只管渲染与交互）、Rust Shell 控制器、连接 owner、系统安全存储、图片处理、iOS 小组件通道、Watch 配套数据流和仅调试构建的 Rust 测试夹具。查看器支持主题切换、Live／Still 和导出。项目只保留这一份架构图。

## 文件

| 文件 | 内容 |
| --- | --- |
| [candidate.json](candidate.json) | 13 个组件、15 条关系及对应源码范围 |
| [lokimetris.html](lokimetris.html) | 包含 SVG 与查看器的独立 HTML |
| [lokimetris.delivery.json](lokimetris.delivery.json) | 规范／产物 SHA-256 和交付回执 |
| [review-3/lokimetris.finalize.json](review-3/lokimetris.finalize.json)、[review-3/lokimetris.browser-check.json](review-3/lokimetris.browser-check.json) | finalize 与浏览器检查回执 |

## 源码证据

`meta.repository` 固定为 `https://github.com/ManonLoki/LokiMetrisDisplayNSH.git` 的提交 `5e1dae33ab5df6de69d404f416f2cdf04aeddab2`，源码引用按该提交的已提交内容校验，并生成 GitHub 源码链接。运行时代码改变后，先提交代码，再更新 candidate 的 revision 和行号并重新生成。

## 重新生成

从项目根目录执行，`ARCHIFY_SKILL` 指向已安装的 archify 技能目录：

```sh
node "$ARCHIFY_SKILL/bin/archify.mjs" finalize architecture \
  .archify/architecture-lokimetris-20261028-105048/candidate.json \
  .archify/architecture-lokimetris-20261028-105048/lokimetris.html \
  --repo-root . --quality showcase --out-dir <新的检查目录> --json
```

候选改变后为 `--out-dir` 使用新的检查目录，保留已有回执。

## 验证记录

validate、deliver、严格 check 和 browser-check 全部通过。自动检查报告 2 处路由交叉（`Link::poll 已校验视图` 与 `iOS 可信状态提交` 等），未做人工视觉复核。
