# 应用架构图维护

[打开交互图](lokimetris.html) · [应用架构](../../docs/architecture/overview.md) · [上一版架构图](../architecture-lokimetris-20261027-183101/lokimetris.html)

本图使用全局安装的 archify 3.2.0，采用 architecture 模式整理 NSH 应用、连接 owner、系统安全存储、图片处理和 Watch 配套数据流。查看器支持主题切换、Live／Still 和导出。旧版图及回执保留。

## 文件与版本

| 文件 | 内容 |
| --- | --- |
| [candidate.json](candidate.json) | 9 个组件、9 条关系和 27 处源码范围 |
| [lokimetris.html](lokimetris.html) | 包含 SVG 与查看器的独立 HTML |
| [lokimetris.delivery.json](lokimetris.delivery.json) | 规范／产物 SHA-256 和交付回执 |
| [source-evidence.json](source-evidence.json) | 本地证据快照身份与 132 个源码 SHA-256 |
| [安装记录](../../output/archify-update-20261027-184140/installation.json) | 3.0.1 → 3.2.0 的版本、全局路径、备份及上游树哈希 |
| [发布包校验](../../output/archify-update-20261027-184140/archive-verification.json) | 官方 v3.2.0 发布归档与 GitHub 资产 SHA-256 比对 |

全局技能及入口分别为 `~/.agents/skills/archify/` 和该目录下的 `bin/archify.mjs`，使用 Node.js 执行。`package.json` 与 `skill-release.json` 均为 3.2.0，自检通过。没有安装额外运行依赖。

本版将生命周期箭头明确为“移动端前台事实”，Android 引用指向实际观察器；桌面窗口可见性仍属于 UI 动画状态。其他组件职责、来源与布局保持上一版的范围。

## 源码证据

原源码目录没有 Git 元数据，使用 `output/docs-rebuild-20261027-183101/source-evidence/` 中的已提交副本验证引用。revision 为 `9940073af8de38b77a8c27c6e1c3eb8269f0d5d1`；132 个文件在当前源码、证据副本及已提交内容中一致，27 处引用范围全部有效。

`https://local.invalid/loki-metris-mobile-display-source-snapshot.git` 仅是本地证据标识，没有对应远端。`link_mode: local-only` 避免生成虚构源码网页链接。该提交只表示本地快照，不代表上游历史；HTML 可独立阅读。运行时源码改变后，需更新副本、创建新快照提交并修改 candidate 的 revision 和引用，再重新生成。

## 重新生成

从项目根目录执行，`ARCHIFY_SKILL` 指向已安装的 archify 技能目录：

```sh
node "$ARCHIFY_SKILL/bin/archify.mjs" finalize architecture \
  .archify/architecture-lokimetris-20261027-184140/candidate.json \
  .archify/architecture-lokimetris-20261027-184140/lokimetris.html \
  --repo-root output/docs-rebuild-20261027-183101/source-evidence \
  --quality showcase --out-dir output/architecture-review-next --json
```

候选改变后为 `--out-dir` 使用新的验证目录，保留已有浏览器证据。新请求使用新的图目录。

## 验证记录

[finalize 回执](../../output/archify-update-20261027-184140/finalize/lokimetris.finalize-summary.json) 记录规范、交付、严格检查和浏览器检查结果。源码依据、输出字节和回执相互绑定。

[浏览器回执](../../output/archify-update-20261027-184140/finalize/lokimetris.browser-check.json) 验证四个桌面尺寸的浅色主题及两端尺寸的深色主题。[视觉捕获回执](../../output/archify-update-20261027-184140/visual-check/lokimetris.visual-check.json) 与 [联系表](../../output/archify-update-20261027-184140/visual-check/lokimetris.visual-check.html) 保存截图；截图自动检查与实际视觉复核分别记录于 [本轮摘要](../../output/archify-update-20261027-184140/summary.json)。
