# 应用架构图维护

[打开交互图](lokimetris.html) · [应用架构](../../docs/architecture/overview.md)

本图使用 archify 3.2.0 的 architecture 模式，覆盖 NSH 主应用、连接 owner、系统安全存储、图片处理、iOS 小组件通道和 Watch 配套数据流。查看器支持主题切换、Live／Still 和导出。项目只保留这一份架构图。

## 文件

| 文件 | 内容 |
| --- | --- |
| [candidate.json](candidate.json) | 12 个组件、13 条关系及对应源码范围 |
| [lokimetris.html](lokimetris.html) | 包含 SVG 与查看器的独立 HTML |
| [lokimetris.delivery.json](lokimetris.delivery.json) | 规范／产物 SHA-256 和交付回执 |
| [review-2/](review-2/) | 当前 HTML 的 finalize 与浏览器检查回执 |

## 源码证据

`meta.repository` 固定为 `https://github.com/ManonLoki/LokiMetrisDisplayNSH.git` 的提交 `ef845dc0d37c35bb302affffcf65b3270d662e4e`，源码引用按该提交的已提交内容校验，并生成 GitHub 源码链接。运行时代码改变后，先提交代码，再更新 candidate 的 revision 和行号并重新生成。

## 重新生成

从项目根目录执行，`ARCHIFY_SKILL` 指向已安装的 archify 技能目录：

```sh
node "$ARCHIFY_SKILL/bin/archify.mjs" finalize architecture \
  .archify/architecture-lokimetris-20261027-212950/candidate.json \
  .archify/architecture-lokimetris-20261027-212950/lokimetris.html \
  --repo-root . --quality showcase --out-dir <新的检查目录> --json
```

候选改变后为 `--out-dir` 使用新的检查目录，保留已有回执。

## 验证记录

validate、deliver、严格 check 和 browser-check 全部通过。自动检查报告 `Link.poll 公开投影` 与 `校验后的资源` 在连接 owner 右侧有一处交叉；调整一次节点位置后仍保留，未做人工视觉复核。
