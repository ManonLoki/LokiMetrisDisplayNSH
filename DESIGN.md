# 设计与动效

本应用的界面规范和动效清单。颜色、动效参数和共用控件都有唯一来源，修改时先改来源，再改使用处。

## 目标与形态

- 目标：手机、平板、电视（Android／iOS）以及 Windows、macOS、Linux 桌面，同一套 NSH TSX 界面。
- 外观固定为深色（`nsh.json` 的 `appearance`），没有浅色方案。
- 窄窗口用底部导航，宽窗口用侧栏（`src/app/navigation-layout.ts`）；电视由遥控方向键移动焦点（`src/shared/ui/remote-focus.tsx`）。

## 颜色

颜色的唯一来源是 `nsh.json` 的 `theme.cssVars.dark`。界面代码不写十六进制或 `rgb()`，通过 `src/shared/ui/theme.ts` 的 `cyberColors` 取语义别名，需要透明度时用 `cyberAlpha`。

| 角色 | token | 用途 |
| --- | --- | --- |
| 画布、卡片、卡片容器 | `background`、`card`、`surface-container` | 页面底色与卡片层级 |
| 主文字、次要文字 | `foreground`、`muted-foreground` | 正文与说明 |
| 主色、次色 | `primary`、`secondary` 及各自的 `-foreground`、`-container` | 青色强调与紫色强调 |
| 边线 | `border`、`outline` | 卡片描边与分隔 |
| 错误、断开 | `destructive`、`on-error`、`disconnected` | 错误文案与未连接状态 |
| 额度等级 | `success`、`warning`、`danger` | 额度条绿、黄、红 |
| 用量卡片 | `usage-surface`、`usage-muted` | 用量卡片底色与说明色 |
| Hook 状态、开关 | `asking`、`switch-off`、`switch-on-thumb` | 询问色、开关关闭轨道与打开滑块 |

新增颜色先在 `nsh.json` 里补 token，再在 `theme.ts` 加别名。

## 共用控件

`src/shared/ui/` 是应用自有的样式层：

- `effects.tsx`：背景、扫光、连接指示点、入场、按压和数字切换等装饰与动效。
- `settings-components.tsx`：设置页共用的卡片、分区、分隔线、图标和按钮样式。
- `image.tsx`、`image-playback.ts`：图片与 GIF 显示，帧时钟由应用控制。
- `remote-focus.tsx`：电视遥控焦点。

功能代码使用这一层，不直接拼装基础控件的外观。

## 动效清单

参数集中在 `theme.ts` 的 `cyberMotion`。所有动效都受三道门控制：应用在前台（`CyberMotionScope`）、所属元素处于活跃状态（`active`）、系统没有开启“减少动态效果”；任一不满足时停在静态画面。

| 位置 | 变化 | 参数 |
| --- | --- | --- |
| `CyberBackdrop` | 深色底、青紫径向光晕和网格，静态 | 网格间距 24、线宽 0.5 |
| `CyberSurfaceScan` | 卡片边缘光带绕行，不可见或减少动态时保留静态光边 | 周期 8000ms、描边 1、圆角 18 |
| `CyberConnectionDot` | 已连接时指示点外圈呼吸 | 周期 1800ms、尺寸 9 |
| `CyberArrival` | 内容淡入、上移并放大到位，按 `order` 错峰（最多 6 档），只播放一次 | 420ms、错峰 65ms、偏移 18、起始缩放 0.98 |
| `CyberPressable` | 按压缩放，保留原生按钮的语义、键盘与禁用状态 | 120ms、缩放 0.96 |
| `CyberNumber` | 数字切换，新值淡入并滑入，旧值淡出并滑出 | 淡入 240ms／滑入 300ms，淡出 150ms／滑出 220ms |

GIF 图片的播放由 `image-playback.ts` 的时钟驱动，在控件隐藏、应用退到后台或减少动态效果时暂停，恢复时从当前帧继续。

## 验证

- 界面行为：`nsh test`（见 [测试说明](docs/development/test.md)）。
- 卡片与图片的真实像素：`python3 tools/test.py --suite visual`。
- 改动颜色或动效后，用视觉套件与模拟器各看一遍；仅无界面测试不能证明外观。
