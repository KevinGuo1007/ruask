/*
 * All copy lives here, zh + en side by side. Facts follow ruask/README.md
 * (2026-11-10 working tree, including the local tldr pages work).
 */
export type Lang = "zh" | "en"

export const REPO = "https://github.com/KevinGuo1007/ruask"

export const INSTALL = [
  "git clone https://github.com/KevinGuo1007/ruask",
  "cd ruask",
  "cargo install --path .",
]

/* ---------- terminal demo scenes ----------
 * Lines use ruask's own model line protocol, then a tiny renderer styles them
 * the way ruask's terminal renderer does:
 *   "# t"  title (bold purple)      "> t"  summary (default fg)
 *   "- t"  purpose (green, numbered) "`c`"  command (cyan, {{x}} italic+underline)
 *   "! t"  local danger warning (red) "~ t"  dim status line (stderr)
 */
export type Scene = { id: string; tab: string; cmd: string; wait?: string; lines: string[] }

const scenesZh: Scene[] = [
  {
    id: "local",
    tab: "本地直出",
    cmd: "ruask tar",
    lines: [
      "# tar",
      "> 归档工具，常与 gzip、bzip2 等压缩方式配合使用。",
      "- 创建归档并用 gzip 压缩：",
      "`tar czf {{目标.tar.gz}} {{文件1 文件2 ...}}`",
      "- 解压 gzip 归档到当前目录：",
      "`tar xzf {{源.tar.gz}}`",
      "- 解压到指定目录：",
      "`tar xf {{源.tar}} -C {{目录}}`",
      "- 列出归档内容：",
      "`tar tvf {{源.tar}}`",
      "~ tldr-pages · CC BY 4.0",
    ],
  },
  {
    id: "ask",
    tab: "自然语言",
    cmd: 'ruask "如何查看占用 8080 端口的进程"',
    wait: "⠋ 正在询问 your-model…",
    lines: [
      "# 查看端口占用",
      "> 用 `lsof` 按端口查找监听进程，再按 PID 结束它。",
      "- 查看正在监听 8080 端口的进程：",
      "`lsof -nP -iTCP:8080 -sTCP:LISTEN`",
      "- 按 PID 结束该进程：",
      "`kill {{pid}}`",
      "~ AI 生成，请核对 · your-model · 1.4 s · 参考 tldr：lsof",
    ],
  },
  {
    id: "pipe",
    tab: "管道报错",
    cmd: 'cat error.log | ruask cargo build -q "为什么失败"',
    wait: "⠋ 正在询问 your-model…",
    lines: [
      "# cargo build 失败原因",
      "> 编译器报 `E0425`：`src/main.rs:12` 使用了未定义的变量 `confg`，附近有名称相近的 `config`。",
      "- 把拼写改正后重新编译：",
      "`cargo build`",
      "- 查看这个错误码的详细说明：",
      "`rustc --explain E0425`",
      "~ AI 生成，请核对 · your-model · 2.1 s",
    ],
  },
  {
    id: "danger",
    tab: "危险命令",
    cmd: 'ruask git -q "如何丢弃所有本地改动"',
    wait: "⠋ 正在询问 your-model…",
    lines: [
      "# 丢弃本地改动",
      "> 先确认要丢弃的内容；已暂存和未暂存的修改都会消失。",
      "- 查看将被丢弃的改动：",
      "`git status --short`",
      "- 把工作区和暂存区重置到最近一次提交：",
      "`git reset --hard`",
      "! ⚠ 不可逆操作，执行前请确认",
      "~ AI 生成，请核对 · your-model · 1.6 s · 参考 tldr：git reset",
    ],
  },
]

const scenesEn: Scene[] = [
  {
    id: "local",
    tab: "local page",
    cmd: "ruask tar",
    lines: [
      "# tar",
      "> Archiving utility. Often combined with a compression method, such as gzip or bzip2.",
      "- Create an archive and compress it with gzip:",
      "`tar czf {{target.tar.gz}} {{file1 file2 ...}}`",
      "- Extract a gzipped archive into the current directory:",
      "`tar xzf {{source.tar.gz}}`",
      "- Extract into a specific directory:",
      "`tar xf {{source.tar}} -C {{directory}}`",
      "- List the contents of an archive:",
      "`tar tvf {{source.tar}}`",
      "~ tldr-pages · CC BY 4.0",
    ],
  },
  {
    id: "ask",
    tab: "plain language",
    cmd: 'ruask "which process is using port 8080"',
    wait: "⠋ Asking your-model…",
    lines: [
      "# Find what holds a port",
      "> Use `lsof` to find the listening process, then stop it by PID.",
      "- Show the process listening on port 8080:",
      "`lsof -nP -iTCP:8080 -sTCP:LISTEN`",
      "- Stop that process:",
      "`kill {{pid}}`",
      "~ AI-generated, please verify · your-model · 1.4 s · tldr reference: lsof",
    ],
  },
  {
    id: "pipe",
    tab: "piped error",
    cmd: 'cat error.log | ruask cargo build -q "why did it fail"',
    wait: "⠋ Asking your-model…",
    lines: [
      "# Why cargo build failed",
      "> `E0425`: `src/main.rs:12` uses an undefined variable `confg`; a similar name `config` exists.",
      "- Fix the typo and rebuild:",
      "`cargo build`",
      "- Read the full explanation of the error code:",
      "`rustc --explain E0425`",
      "~ AI-generated, please verify · your-model · 2.1 s",
    ],
  },
  {
    id: "danger",
    tab: "destructive",
    cmd: 'ruask git -q "discard all local changes"',
    wait: "⠋ Asking your-model…",
    lines: [
      "# Discard local changes",
      "> Check what you are about to lose first: staged and unstaged edits both go away.",
      "- See which changes will be discarded:",
      "`git status --short`",
      "- Reset the working tree and index to the last commit:",
      "`git reset --hard`",
      "! ⚠ Destructive: double-check before running",
      "~ AI-generated, please verify · your-model · 1.6 s · tldr reference: git reset",
    ],
  },
]

export const copy = {
  zh: {
    nav: { how: "原理", features: "特性", start: "快速开始", examples: "示例" },
    skip: "跳到正文",
    hero: {
      kicker: "v0.1 · Rust CLI · macOS / Linux",
      title1: "忘了命令怎么写？",
      title2: "在终端里问一句。",
      lead: "ruask 先查本机的 tldr 页面，命中就毫秒级直出；查不到或带着问题时，再把相关页面作为参考交给你配置的模型，流式逐行回答。也能把报错日志用管道交给它解读。",
      install: "从源码安装",
      installHint: "需要支持 Rust 2024 edition 的工具链；暂未发布到 crates.io。",
      start: "快速开始",
      github: "GitHub",
      termNote: "示意输出：本地页面内容来自 tldr-pages，模型回答由你配置的服务生成，配色与布局由 ruask 本地渲染。",
    },
    stats: [
      { value: "ms", label: "本地 tldr 页面直出，无需网络和模型" },
      { value: "2", label: "种协议：OpenAI 兼容与 Anthropic" },
      { value: "64 KiB", label: "管道输入上限，超限报错不截断" },
      { value: "0", label: "追踪、对话历史与答案缓存" },
    ],
    video: {
      kicker: "演示",
      title: "20 秒看懂 ruask",
      caption: "命令速查、自然语言提问、管道解读报错。终端由 VHS 录制。",
    },
    how: {
      kicker: "原理",
      title: "先查本地，再问模型",
      lead: "安装 tldr 页面后，ruask 按查询形态选择路径。没有安装页面时，所有查询走纯模型路径。",
      nodes: {
        query: "你的查询",
        queryNote: "命令 / 问题 / 管道",
        check: "只有命令名？",
        checkNote: "无 -q、无 stdin",
        local: "本地页面直出",
        localNote: "毫秒级 · 离线",
        ref: "附上参考页",
        refNote: "命令页 + 最相关两页",
        model: "模型流式回答",
        modelNote: "OpenAI 兼容 / Anthropic",
        render: "本地渲染",
        renderNote: "着色 · 折行 · 危险标注",
      },
      yes: "是",
      no: "否",
    },
    features: {
      kicker: "特性",
      title: "小而专注的命令行助手",
      items: [
        { t: "本地 tldr 直出", d: "ruask --update-pages 安装官方页面并校验。只查命令名时直接显示，平台不符会注明，首词拼错会自动纠正。" },
        { t: "自然语言提问", d: "直接描述任务，或用 -q 给命令附带问题；-p linux|macos 指定目标平台，优先给出可用命令。" },
        { t: "管道解读报错", d: "cat error.log | ruask 解释输出、明显错误和下一步命令。stdin 上限 64 KiB，超限报错，不截断。" },
        { t: "两种协议", d: "OpenAI 兼容 Chat Completions 与 Anthropic 原生 Messages，显式选择，不按模型名猜测。" },
        { t: "流式逐字显示", d: "说明随到随显示，命令整行显示；等待首行时有提示，断流会保留已显示内容并以退出码 5 结束。" },
        { t: "危险命令标注", d: "递归 rm、git reset --hard、git push --force 等不可逆命令，无论来自页面还是模型，都在下方本地警告。" },
        { t: "了解你的环境", d: "目标平台是本机时，附带 shell、可用包管理器与命令是否已安装；只检查 PATH，不运行任何程序。" },
        { t: "Unix 友好", d: "stdout 只含回答，提醒写 stderr；重定向不着色、不编号、不折行；退出码 0–5 区分错误类型。" },
      ],
    },
    start: {
      kicker: "快速开始",
      title: "三分钟配好",
      steps: [
        { t: "从源码安装", d: "确保 ~/.cargo/bin 在 PATH 中。" },
        { t: "安装本地 tldr 页面（推荐）", d: "下载官方 tldr.zip 并与校验文件核对，只保留英文与简体中文页面。离线可用 --from 指定已下载的归档。" },
        { t: "配置模型", d: "--init-config 创建 0600 权限的模板，不需要凭据、不发请求。程序在 base_url 后追加 /chat/completions 或 /messages，不会自动补 /v1。" },
        { t: "开始提问", d: "只查命令名时走本地页面，其余查询交给模型。" },
      ],
      cfgFile: "config.toml",
      cfgEnv: "环境变量",
      envNote: "环境变量按字段覆盖配置文件；Anthropic 使用 ANTHROPIC_* 并设置 RUASK_API_TYPE=anthropic。",
    },
    examples: {
      kicker: "示例",
      title: "常用写法",
      items: [
        { c: "ruask tar", d: "只有命令名：本地页面直出" },
        { c: "ruask git commit", d: "多个位置参数组成的子命令" },
        { c: 'ruask "如何查看占用 8080 端口的进程"', d: "自然语言命令行任务" },
        { c: 'ruask git commit -q "如何修改上一次提交的信息"', d: "命令查询加附带问题" },
        { c: "ruask -- git log -p", d: "目标选项与 ruask 选项重名时用 --" },
        { c: 'ruask -L zh -p linux "如何查看端口占用"', d: "指定结果语言和目标平台" },
        { c: 'cat error.log | ruask cargo build -q "为什么失败"', d: "带命令目标与问题解读管道输出" },
        { c: 'tail -n 100 error.log | ruask -q "如何排查这些错误"', d: "先筛选，保持在 64 KiB 以内" },
        { c: "ruask git status > usage.txt", d: "重定向得到纯文本" },
        { c: "ruask --version", d: "版本与本地页面状态" },
      ],
      copied: "已复制到剪贴板",
      copy: "复制",
    },
    faq: {
      kicker: "边界",
      title: "它做什么，不做什么",
      items: [
        { q: "支持哪些平台和语言？", a: "首版支持 macOS 与 Linux，结果语言为英文和简体中文（-L en|zh，也可写入配置或跟随 locale）。暂不适配 Windows。" },
        { q: "会执行它建议的命令吗？", a: "不会。ruask 只显示命令，不执行；也不保留跨调用的对话历史，没有答案缓存，不加载项目目录里的 .env。" },
        { q: "本地页面从哪里来？", a: "来自 tldr-pages，按 CC BY 4.0 使用。ruask 不随包分发页面，由你运行 --update-pages 下载并校验。" },
        { q: "退出码代表什么？", a: "0 成功，1 其他错误，2 用法或输入错误，3 配置错误，4 无法连接或服务返回错误，5 回答不完整。" },
        { q: "不做的事", a: "持续日志分析、命令执行、跨调用历史、联网检索文档、其他云平台专属协议和 Windows 适配。" },
      ],
    },
    footer: {
      license: "以 MIT 许可证开源。本站不使用追踪脚本，字体随站点自托管。",
      tldr: "演示中的本地页面内容来自 tldr-pages（CC BY 4.0）。",
    },
    toggles: { theme: "切换主题", lang: "切换语言" },
  },
  en: {
    nav: { how: "How", features: "Features", start: "Quick start", examples: "Examples" },
    skip: "Skip to content",
    hero: {
      kicker: "v0.1 · Rust CLI · macOS / Linux",
      title1: "Forgot the flags?",
      title2: "Just ask your terminal.",
      lead: "ruask checks your local tldr pages first and prints a hit in milliseconds. Otherwise it hands the relevant pages to your model as reference and streams a short answer. Pipe in an error log and it explains what went wrong.",
      install: "Install from source",
      installHint: "Needs a Rust toolchain with 2024 edition support. Not on crates.io yet.",
      start: "Get started",
      github: "GitHub",
      termNote: "Illustrative output: local pages come from tldr-pages, model answers from the service you configure; colors and layout are rendered locally by ruask.",
    },
    stats: [
      { value: "ms", label: "local tldr pages, no network or model" },
      { value: "2", label: "protocols: OpenAI-compatible and Anthropic" },
      { value: "64 KiB", label: "stdin cap, rejected rather than truncated" },
      { value: "0", label: "tracking, chat history, or answer cache" },
    ],
    video: {
      kicker: "Demo",
      title: "ruask in 20 seconds",
      caption: "Command lookup, plain-language questions, piped error explanation. Recorded with VHS.",
    },
    how: {
      kicker: "How it works",
      title: "Local first, model second",
      lead: "With tldr pages installed, ruask picks a path by the shape of your query. Without them, every query goes to the model.",
      nodes: {
        query: "Your query",
        queryNote: "command / question / pipe",
        check: "Command only?",
        checkNote: "no -q, no stdin",
        local: "Print local page",
        localNote: "milliseconds · offline",
        ref: "Attach reference",
        refNote: "command page + 2 closest",
        model: "Model streams",
        modelNote: "OpenAI-compatible / Anthropic",
        render: "Local renderer",
        renderNote: "color · wrap · warnings",
      },
      yes: "yes",
      no: "no",
    },
    features: {
      kicker: "Features",
      title: "A small, focused command-line helper",
      items: [
        { t: "Local tldr pages", d: "ruask --update-pages installs and verifies the official pages. Command-only queries print them directly, note other-platform pages, and fix near-miss typos." },
        { t: "Ask in plain language", d: "Describe the task, or attach a question with -q. Use -p linux|macos to target a platform." },
        { t: "Explain piped output", d: "cat error.log | ruask explains the output, obvious errors, and next commands. stdin is capped at 64 KiB — rejected, never truncated." },
        { t: "Two protocols", d: "OpenAI-compatible Chat Completions and native Anthropic Messages, chosen explicitly — never guessed from the model name." },
        { t: "Streams as it goes", d: "Prose appears as it arrives, commands a full line at a time. A hint shows while waiting; broken streams keep what was shown and exit 5." },
        { t: "Destructive-command warnings", d: "Recursive rm, git reset --hard, git push --force and friends get a local warning, whether they come from a page or the model." },
        { t: "Knows your environment", d: "For the local platform it sends your shell, available package managers, and whether the command is installed — by checking PATH only, never running anything." },
        { t: "Plays well with Unix", d: "stdout holds only the answer; notices go to stderr. Redirects stay plain, unnumbered, unwrapped. Exit codes 0–5 tell errors apart." },
      ],
    },
    start: {
      kicker: "Quick start",
      title: "Set up in three minutes",
      steps: [
        { t: "Install from source", d: "Make sure ~/.cargo/bin is on your PATH." },
        { t: "Install local tldr pages (recommended)", d: "Downloads the official tldr.zip, verifies it, and keeps English and Simplified Chinese pages. Offline? Pass an archive with --from." },
        { t: "Configure a model", d: "--init-config writes a 0600 template — no credentials or requests needed. ruask appends /chat/completions or /messages to base_url and never adds /v1." },
        { t: "Ask away", d: "Command-only queries use local pages; everything else goes to the model." },
      ],
      cfgFile: "config.toml",
      cfgEnv: "Environment",
      envNote: "Env vars override the file field by field. For Anthropic use ANTHROPIC_* with RUASK_API_TYPE=anthropic.",
    },
    examples: {
      kicker: "Examples",
      title: "Common invocations",
      items: [
        { c: "ruask tar", d: "Command only: printed from local pages" },
        { c: "ruask git commit", d: "Subcommand from positional args" },
        { c: 'ruask "which process is using port 8080"', d: "A plain-language shell task" },
        { c: 'ruask git commit -q "how do I edit the last commit message"', d: "Command plus a question" },
        { c: "ruask -- git log -p", d: "Use -- when target flags clash with ruask's" },
        { c: 'ruask -L en -p linux "how to check port usage"', d: "Set answer language and platform" },
        { c: 'cat error.log | ruask cargo build -q "why did it fail"', d: "Explain piped output with context" },
        { c: 'tail -n 100 error.log | ruask -q "how to troubleshoot these errors"', d: "Trim input to stay under 64 KiB" },
        { c: "ruask git status > usage.txt", d: "Redirects produce plain text" },
        { c: "ruask --version", d: "Version and local page status" },
      ],
      copied: "Copied to clipboard",
      copy: "Copy",
    },
    faq: {
      kicker: "Scope",
      title: "What it does, and doesn't",
      items: [
        { q: "Which platforms and languages?", a: "macOS and Linux, with answers in English or Simplified Chinese (-L en|zh, the config file, or your locale). No Windows support yet." },
        { q: "Does it run the commands it suggests?", a: "No. ruask only prints commands. It keeps no history between calls, caches no answers, and ignores project .env files." },
        { q: "Where do local pages come from?", a: "tldr-pages, used under CC BY 4.0. ruask doesn't bundle them; --update-pages downloads and verifies them." },
        { q: "What do the exit codes mean?", a: "0 success, 1 other error, 2 usage or input error, 3 config error, 4 connection or service error, 5 incomplete answer." },
        { q: "Out of scope", a: "Continuous log analysis, command execution, cross-call history, web document retrieval, cloud-specific protocols, and Windows." },
      ],
    },
    footer: {
      license: "MIT licensed. No tracking; fonts are self-hosted.",
      tldr: "Local page content in the demo comes from tldr-pages (CC BY 4.0).",
    },
    toggles: { theme: "Toggle theme", lang: "Switch language" },
  },
} as const

export type Copy = (typeof copy)[Lang]
export const scenes: Record<Lang, Scene[]> = { zh: scenesZh, en: scenesEn }
