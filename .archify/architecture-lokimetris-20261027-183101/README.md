# 应用架构图维护

[打开交互图](lokimetris.html) · [应用架构](../../docs/architecture/overview.md) · [文档目录](../../docs/README.md)

使用 archify 的 `architecture` 模式整理当前 NSH 应用、连接 owner、系统凭据、图片处理和 Watch 配套数据流。默认静态展示，内置主题切换与图片／SVG 导出。

## 文件

| 文件 | 内容 |
| --- | --- |
| [candidate.json](candidate.json) | 组件、连接、27 处源码范围及布局 |
| [lokimetris.html](lokimetris.html) | 可独立打开的 HTML，内含 SVG 与查看器 |
| [lokimetris.delivery.json](lokimetris.delivery.json) | 规范／产物 SHA-256 和交付回执 |
| [source-evidence.json](source-evidence.json) | 本地证据快照身份与源码 SHA-256 |

## 源码证据

源目录没有 Git 元数据。本次只将应用源码和清单复制到 `output/docs-rebuild-20261027-183101/source-evidence/`，在副本中建立本地 Git 快照，原目录没有初始化 Git。

`9940073af8de38b77a8c27c6e1c3eb8269f0d5d1` 只标识这个证据副本。`https://local.invalid/loki-metris-mobile-display-source-snapshot.git` 使用保留域名，作为本地证据的标识，没有对应远端；`link_mode: local-only` 避免生成虚构的源码网页链接。每个节点的源路径和行号与快照中的已提交字节核对。来源目录或公开上游的真实提交历史没有被推断。

修改运行时源码后，先按 `source-evidence.json` 的文件清单更新证据副本，创建新的本地提交，再同步 candidate 的 revision、引用和布局，重新生成。文档或测试的修改不改变本图引用的运行时文件。证据副本删掉后，HTML 仍可独立阅读；重新生成时需重建证据副本并使用新 revision。

## 重新生成

在项目根目录执行，`ARCHIFY_SKILL` 指向本机已安装的 archify 技能目录。这里不安装或升级工具。

```sh
node "$ARCHIFY_SKILL/bin/archify.mjs" finalize architecture \
  .archify/architecture-lokimetris-20261027-183101/candidate.json \
  .archify/architecture-lokimetris-20261027-183101/lokimetris.html \
  --repo-root output/docs-rebuild-20261027-183101/source-evidence \
  --quality showcase --out-dir output/architecture-review-next --json
```

候选有改动时，为 `--out-dir` 使用新的验证目录，保留已有浏览器证据。生成后可通过 `visual-check` 捕获截图，但截图捕获通过不等于人工视觉检查。

## 本次检查

- showcase 的 9 项规范检查、产物交付、严格 provenance 检查及浏览器检查通过，无错误或警告。
- 浏览器验证四个桌面尺寸的浅色主题及两端尺寸的深色主题，没有横向溢出。
- [视觉捕获回执](../../output/docs-rebuild-20261027-183101/visual-final/lokimetris.visual-check.json) 通过；已查看 2048×1320 浅色与 1440×900 深色截图，检查组件、关系标签和滚动显示。
- [规范与产物回执](../../output/docs-rebuild-20261027-183101/archify-review-2/lokimetris.finalize-summary.json) 与 [浏览器回执](../../output/docs-rebuild-20261027-183101/archify-review-2/lokimetris.browser-check.json) 保存在本次输出目录。
