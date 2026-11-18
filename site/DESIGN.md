# ruask 设计系统 · Blueprint Terminal

目标：避开「深色 + 紫色渐变 + 大圆角卡片 + Inter」这类 AI 模板感，让官网看起来像一份**工程图纸上的终端说明书**。

## 参考来源与提取要点

| 参考 | 取了什么 | 没取什么 |
| --- | --- | --- |
| [zed.dev](https://zed.dev) | 衬线细体标题（IBM Plex Serif 300）+ 等宽/正文分层；虚线纵向栏线框住内容区；栏线与横线交点的十字角标；单一蓝色强调；4px 小圆角；按钮内的快捷键提示 | 大段客户证言、产品截图 |
| [atuin.sh](https://atuin.sh) · [ghostty.org](https://ghostty.org) | `[ 括号 ]` 等宽导航；`── 小标题` 段落标签；`01 / 02` 编号；六边形终端按钮；ASCII 字符动画 | Atuin 的绿色主色、Ghostty 的整屏 ASCII 首屏 |
| [astral.sh](https://astral.sh) · [charm.land](https://charm.land) | 仅作对照（撞色与插画路线），未采用 | — |

方向由用户在 A（Zed 编辑蓝图）、B（Astral 酸性撞色）、C（终端原生）、D（Charm 趣味）中选定 **A+C 混合**。

## 1. Token（`src/index.css`）

两层结构：第一层是原始 token，第二层把 shadcn 的语义变量（`--primary`、`--border`…）映射到第一层，所以 shadcn 组件不需要逐个覆写颜色。

### 颜色

| Token | 亮色（制图纸） | 暗色（墨黑制图台） | 用途 |
| --- | --- | --- | --- |
| `--paper` | `#f3f0e8` | `#0f1012` | 页面底色，暖色纸张而非纯白/蓝黑 |
| `--paper-raised` / `--paper-sunken` | `#fbf9f4` / `#e9e5da` | `#15161a` / `#1b1d21` | 面板 / 凹陷区（交替段落背景） |
| `--ink` / `--ink-muted` / `--ink-faint` | `#1b1c1f` / `#5d5b55` / `#8f8c84` | `#ebe8e1` / `#a19e96` / `#6c6a64` | 正文 / 次要 / 注释与尺寸标注 |
| `--rule` / `--rule-strong` | `#d6d1c4` / `#b9b3a3` | `#26282d` / `#3a3d44` | 1px 分割线 / 角标与边框 |
| `--blueprint` | `#2346c8` | `#9db6ff` | **唯一品牌强调色**：衬线强调句、主按钮、激活态、进度线 |
| `--grid` | ink 6% | ink 5% | 24px 制图网格 |

终端色只在终端、代码块、示意图里出现，对应 ruask 渲染器的语义角色：

| Token | 值 | ruask 中的角色 |
| --- | --- | --- |
| `--term-title` | `#c4a7ff` | 标题（加粗紫） |
| `--term-use` | `#8fd694` | 示例用途（绿） |
| `--term-cmd` | `#7fd3e3` | 命令（青），占位符斜体下划线 |
| `--term-code` | `#e8c372` | 行内代码（黄斜体） |
| `--term-danger` | `#ff7b72` | 只用于危险命令警告 |
| `--term-prompt` | `#8fd694` | `❯` / `$` 提示符、本地路径、段落小标题 |
| `--term-dim` | `#6f6d67` | 编号、等待提示、来源标识 |

### 字体（全部自托管，`@fontsource`，无外部请求）

| 角色 | 字体 | 规则 |
| --- | --- | --- |
| Display | IBM Plex Serif 300 / 300 italic，中文回退宋体 | 标题字重 300；强调句用斜体 + `--blueprint`。全局 `font-synthesis: none`，中文不会被伪斜体 |
| Body | IBM Plex Sans，中文回退 PingFang SC | 15px / 1.7 |
| Mono | IBM Plex Mono | 导航、按钮、标签、编号、注释、终端 |

字号：11（注释）· 12–13（等宽标签）· 15（正文）· 17（步骤标题）· `clamp(1.8rem,3.2vw,2.6rem)`（段落标题）· `clamp(2.2rem,4.4vw,3.5rem)`（首屏）。

### 形状与空间

- 圆角 `--radius: 4px`；徽标 3px；不使用胶囊和大圆角卡片。
- 无投影；层级只靠 1px 线和底色差。
- 内容列 1180px，左右各一条虚线栏线；每个段落顶部横线与栏线交点放 `+` 十字角标（`Section` / `Cross`）。
- 段落纵向留白 80–96px。

## 2. 组件（shadcn 定制，`src/components/ui/`）

| 组件 | 定制 |
| --- | --- |
| `Button` | 等宽标签；新增 `bracket`（`[ 链接 ]`，悬停时括号外扩）与 `terminal`（六边形 `$` 按钮）变体；`Kbd` 子组件做快捷键提示 |
| `Badge` | 方角等宽小标签；新增 `blueprint` / `terminal` / `danger` 变体 |
| `Card` | 扁平面板 + 四角十字标，悬停时角标变长并变为 blueprint 色 |
| `Tabs` | `default` 为终端标签条；`line` 为蓝图下划线，激活时从左向右画出 |
| `Accordion` | 分割线列表，`+` 图标展开时旋转 45° 成 `×` |
| `ToggleGroup` | 中 / EN 切换，激活项反色 |
| `Tooltip` / `Sonner` | 等宽小字；复制成功的 toast 是一行终端状态信息 |

站点级组件（`src/components/site/`）：`Kicker`（`── 03 / 特性`）、`Display`、`Section`、`Reveal`、`Terminal`、`CodeBlock`、`AsciiWordmark`、`FlowDiagram`、`VideoFrame`（带 1920 × 1080 尺寸标注的视频框）。

## 3. 动效（motion）

统一缓动 `cubic-bezier(0.2, 0.7, 0.1, 1)`（「落笔」：快起慢收），时长 0.2–0.9s。

| 位置 | 效果 |
| --- | --- |
| 首屏标题 | 每行从遮罩下方升起，第二行延迟 0.13s |
| 小标题 | `──` 横线从左向右画出 |
| 段落内容 | 进入视口时淡入并上移 14px，同组元素错开 60–80ms |
| 终端演示 | 逐字输入命令 → 等待提示 spinner → 按行流式输出；离开视口暂停；切换场景淡入淡出 |
| ASCII 字标 | 字符密度波纹，约 15fps，离开视口暂停 |
| 原理图 | 连线按顺序画出，光点沿路径循环移动 |
| 导航 | 顶部 1px 阅读进度线；当前段落的括号变为 blueprint 色 |
| 卡片 / 按钮 | 角标伸长、箭头右移、主题图标旋转切换、复制图标形变为对勾 |

`prefers-reduced-motion: reduce` 时：所有入场动画直接显示终态，终端直接显示完整输出，ASCII 静止，视频不自动播放，光点不渲染。

## 4. Logo（`public/brand/`）

终端提示符图标：深色圆角底板（`#141824 → #0b0e14`，1px 6% 白色描边），`>` 为粉 → 紫 → 蓝渐变（`#f78cc6 → #c792ea → #82aaff`），光标条为青 → 绿渐变（`#7fdbca → #c3e88d`）。这是全站唯一使用渐变的地方，其余界面仍只用 `--blueprint` 一种强调色。

| 文件 | 用途 |
| --- | --- |
| `ruask-logo.svg` | 图标（也是 `assets/favicon.svg`），64 网格，纯路径 |
| `ruask-wordmark-light.svg` / `ruask-wordmark-dark.svg` | 图标 + 文字，文字为 IBM Plex Mono 600 转曲；分别用于浅色 / 深色背景 |
| `ruask-logo-512.png` / `apple-touch-icon.png` | PNG 版本（512 / 180） |
| `ruask-social.png` | 1280×640 社交分享图（og:image，也可上传为 GitHub Social preview） |

最小使用尺寸 16px；图标周围至少留出图标宽度 1/4 的空白；不要改动渐变方向或给文字加渐变。
