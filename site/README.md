# ruask 官网

单页官网，Vite + React + Tailwind CSS v4 + shadcn/ui（按设计系统定制）+ motion。
设计系统「Blueprint Terminal」的来源、token、组件定制与动效规则见 [DESIGN.md](DESIGN.md)。

- 中文为主，右上角可切换英文；跟随系统深浅色，也可手动切换（偏好只存在 `localStorage`）
- 无追踪脚本；字体通过 `@fontsource` 自托管，不请求外部字体服务
- 文案依据仓库根目录的 `README.md`

```text
site/
  index.html              入口（首屏前应用主题，避免闪烁）
  DESIGN.md               设计系统说明
  src/
    index.css             设计 token + shadcn 语义变量映射
    content.ts            全部中英文案与终端演示场景
    App.tsx               各段落
    hooks/use-prefs.tsx   语言 / 主题
    components/ui/        shadcn 组件（已按设计系统改写）
    components/site/      终端演示、ASCII 字标、原理图、视频框、导航等
  public/assets/          发布视频、封面帧、演示 GIF、favicon（构建时原样复制）
```

## 本地开发

需要 Node.js 20+。

```sh
cd site
npm install
npm run dev        # http://127.0.0.1:4173
npm run build      # 类型检查 + 产出 dist/
npm run preview    # 预览 dist/
```

## 发布视频

`public/assets/` 中的视频与 GIF 是实体文件（不要用软链）：

| 文件 | 说明 |
| --- | --- |
| `ruask-launch.{zh,en}.mp4` | 中文 / 英文动画版成片，1920×1080，H.264 + AAC，zh 约 47s、en 约 48s，优先加载；随页面语言切换 |
| `ruask-launch.{zh,en}.webm` | 同一成片的 VP9 + Opus 版本，备用源 |
| `ruask-launch-poster.{zh,en}.jpg` | 封面帧（logo 与两行文案，取自 9.6s），1280×720 |
| `ruask-demo.{en,zh}.gif` | 终端演示动图，分别由 `README.md` / `README.zh-CN.md` 引用 |

视频带配乐和音效（素材均为 CC0），默认静音循环，观众可在播放控件里打开声音；滚动到一半以上可见时自动播放，离开时暂停；手动暂停后不再自动播放；开启「减少动态效果」时不自动播放。

重新制作后替换上述文件（成片体积较大，先转码成网页用的版本），并重新生成封面帧：

```sh
for l in zh en; do
  ffmpeg -y -i SRC.$l.mp4 -c:v libx264 -preset slow -crf 24 -pix_fmt yuv420p -c:a aac -b:a 160k -movflags +faststart public/assets/ruask-launch.$l.mp4
  ffmpeg -y -i SRC.$l.mp4 -c:v libvpx-vp9 -crf 34 -b:v 0 -c:a libopus -b:a 128k public/assets/ruask-launch.$l.webm
  ffmpeg -y -ss 9.6 -i public/assets/ruask-launch.$l.mp4 -frames:v 1 -vf scale=1280:-1 -q:v 4 public/assets/ruask-launch-poster.$l.jpg
done
```

## 部署（Vercel）

站点部署在 Vercel，项目连接 GitHub 仓库 `KevinGuo1007/ruask`，Root Directory 为 `site`，
框架预设 Vite（构建命令 `npm run build`，输出目录 `dist`）。推送到 `main` 自动发布生产环境，
其他分支和 PR 生成预览地址。

手动部署（需要先 `vercel login`）：

```sh
cd site
vercel deploy --prod
```

`vite.config.ts` 设置了 `base: "./"`，产物全部使用相对路径，也可以部署到任意静态托管的子路径。

## 维护说明

- 功能变化时改 `src/content.ts`（中英两份）；终端演示场景也在其中，使用 ruask 的行格式（`#` 标题、`>` 简介、`-` 用途、整行反引号命令；站点额外用 `!` 表示危险警告、`~` 表示暗色状态行）。
- 加 shadcn 组件：`npx shadcn@latest add <name>`，然后按 DESIGN.md 的规则改写样式（等宽标签、4px 圆角、1px 线、无投影）。CLI 偶尔会把 `cn` 的导入写成 `from "cn"`，需改回 `@/lib/utils`。
- 许可证文案在 `src/content.ts` 的 `footer.license`，与仓库根目录的 `LICENSE`（MIT）保持一致。
