# ruask

在终端内查询命令、子命令或自然语言命令行任务，获得简短的速查结果。

## 开始使用

需要支持 Rust 2024 edition 的 Rust 工具链。

```sh
cargo build --release

./target/release/ruask --init-config
# 编辑上一步显示的配置文件，填入模型地址、密钥和名称。
./target/release/ruask git commit
```

`OPENAI_BASE_URL` / `ANTHROPIC_BASE_URL` 是对应协议的基础地址，包含服务所需的路径前缀。程序在后面追加
`/chat/completions`（OpenAI 兼容协议）或 `/messages`（Anthropic 原生协议），
地址末尾有无 `/` 都可以；网关中间路径保持不变，程序不自动补 `/v1`。地址、密钥和模型名称合并后均为必需。

支持 OpenAI 兼容的 Chat Completions 与 Anthropic 原生 Messages 文本接口，默认发送一次流式请求。
正文每形成完整行就显示并刷新，正常结束时补齐没有换行的最后一行。
模型请求的总超时为 60 秒。Anthropic 只展示文本内容块，忽略思考、工具与心跳数据；
要求正常结束原因及消息结束事件，生成截断、异常 EOF 和流内错误均不能算成功。

希望直接使用 `ruask` 命令时，可以安装到 Cargo 的二进制目录：

```sh
cargo install --path .
```

确保 Cargo 的二进制目录（通常是 `~/.cargo/bin`）在 `PATH` 中，然后使用：

```sh
ruask cargo
ruask git commit
ruask git commit -q "如何修改上一次提交的信息"
ruask "如何查看占用 8080 端口的进程"
ruask --question "如何查找大于 100 MB 的文件"
ruask -L zh -p linux "如何查看端口占用"
ruask --language en --platform macos launchctl
ruask "git commit --amend"
ruask -- git commit --amend
cat error.log | ruask cargo build -q "为什么失败，下一步如何排查"
ruask git status > usage.txt
ruask --help
```

## 用户配置

macOS 和 Linux 均读取 `$XDG_CONFIG_HOME/ruask/config.toml`。
`XDG_CONFIG_HOME` 未设置或为空时使用 `$HOME/.config/ruask/config.toml`；路径必须是绝对路径。
切换工作目录、安装渠道或升级二进制不会改变这份配置。

`ruask --init-config` 显示配置位置并创建模板，不要求模型凭据，也不发送请求。
已有文件保持原样；新文件权限为 `0600`，仅当前用户可读写。编辑其中的 OpenAI 配置即可开始查询：

```toml
api_type = "openai"
# language = "zh" # en / zh（简体中文）；未设置时使用 locale，再回退英文

[openai]
base_url = "https://your-provider.example/v1"
model = "your-model"
api_key = "YOUR_API_KEY"

[anthropic]
base_url = "https://your-anthropic-provider.example/v1"
model = "your-model"
api_key = "YOUR_API_KEY"
```

协议优先级为 `RUASK_API_TYPE` → 文件 `api_type` → `openai`。
`openai` 表示 OpenAI 兼容协议，不根据模型名或服务域名猜测协议。
`anthropic` 使用原生 Messages，请求在顶层发送 `system` 指令、用户消息与 `max_tokens=4096`。
两类协议均默认流式，复用完整行展示；不会因为模型名含 `claude` 改变所选协议。

Anthropic 发送 `Authorization: Bearer <api_key>` 和 `anthropic-version: 2026-10-21`。
使用未绑定单个 workspace 的密钥时，另设 `ANTHROPIC_WORKSPACE_ID`，作为
`anthropic-workspace-id` 请求头；绑定单 workspace 的密钥可省略。该变量只在选中
`anthropic` 时使用，显式空白值在本地报错。只接受旧 `x-api-key` 的网关需要调整其认证兼容性，
程序不会自动猜测另一种认证方式。认证与版本依据
[官方认证说明](https://platform.claude.com/docs/en/manage-claude/authentication)和
[API 版本说明](https://platform.claude.com/docs/en/api/versioning)。

所选协议按字段合并环境变量与文件配置，环境变量优先：

| 协议 | 环境变量 |
| --- | --- |
| `openai` | `OPENAI_BASE_URL`、`OPENAI_API_KEY`、`OPENAI_MODEL` |
| `anthropic` | `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`、`ANTHROPIC_MODEL` |

仅使用所选协议的字段，不向另一组借用。显式空白环境值报错，不回退到文件。
没有用户文件时，原有的三个 OpenAI 环境变量仍可独立使用：

```sh
export OPENAI_BASE_URL="https://your-provider.example/v1"
export OPENAI_API_KEY="你的密钥"
export OPENAI_MODEL="你的模型名"
ruask cargo
```

程序不自动加载当前目录或父目录的 `.env` 或项目配置，也不修改 shell 启动文件。
无效 TOML 和缺失配置在请求前报错，错误不显示凭据或配置原文。

## 输入和输出约定

结果语言支持 `en`（英文）和 `zh`（简体中文）。优先级为 `-L` / `--language`
→ 用户配置文件的 `language` → locale → 英文。CLI 和文件中显式不支持的语言在本地报错，
不发送模型请求；CLI 的有效选择可以覆盖文件中的语言偏好。

自动选择时，`LANG` 未设置就直接使用英文；否则依次尝试 `LANGUAGE` 的冒号列表、
`LANG`、英文，跳过 C/POSIX 和不支持的候选。编码、区域和修饰信息归一为基础语言，
例如 `en_US.UTF-8` → `en`，`zh_TW.UTF-8` → `zh`；中文统一要求简体。
不使用 `LC_ALL` 或 `LC_MESSAGES` 选择结果语言。

目标平台默认为当前主机，`-p` / `--platform` 接受 `linux`、`osx` 和 `macos`（归一为 `osx`），
无效值在本地报错。请求携带最终语言和平台，并要求模型优先为自然语言任务提供目标平台可用命令。
明确查询的命令若不适用于目标平台，则要求简短注明适用平台后继续解释原命令，不自动替换工具。
本地 HTTP 替身测试只验证请求约束及固定回答展示；真实模型的语言遵从和跨平台命令质量由
[#10](https://github.com/KevinGuo1007/ruask/issues/10) 单独验收。

| 输入 | 查询语义 |
| --- | --- |
| `ruask cargo` | 已知命令 |
| `ruask git commit` | 多个位置参数组成的子命令 |
| `ruask "如何查看端口占用"` | 自然语言命令行任务 |
| `ruask git commit -q "如何撤销提交"` | 命令查询与附带问题 |
| `ruask --question "如何查看端口占用"` | 独立的具体问题 |
| `ruask "git commit --amend"` 或 `ruask -- git commit --amend` | 含目标命令选项的查询 |

多个位置参数用空格连接；需要保留准确的空格或 Shell 特殊字符时，用引号括起查询。
`-q` 与 `--question` 各接收一个字符串，每次调用只能提供一次。未知 ruask 选项、
问题选项缺值和显式空白查询会在本地报错。`--` 后所有参数都视为查询内容。

普通命令查询要求模型提供一句简介和约五个常用示例；具体问题要求一至三个相关示例
及简短解释。每个示例包含用途说明与命令，使用类 tldr 的纯文本格式。
模型指令限定为命令行用法与输出解读，不提供通用聊天；程序不检索文档、不执行建议命令，
不保留跨调用的对话历史。测试验证请求契约，实际模型的回答质量取决于所用服务。

`cat error.log | ruask` 默认解释输出含义和明显错误，并给出相关下一步命令；
`cat error.log | ruask cargo build -q "为什么失败"` 同时保留命令目标、附带问题和原始输出上下文，
重点回答问题。输出上下文作为数据传递，保留有效 UTF-8 文本的空格、换行与 Unicode，
输出解读不强行套用五例结构。

有限管道读到 EOF 后发起一次查询，不持续分析日志。stdin 上限为 **65,536 字节（64 KiB）**，
恰好达到允许；多字节文本按 UTF-8 字节数计量。超限在请求前报错，不截断、不自动分段，
也不提供上限调整选项。先用筛选工具或 `tail` 缩小输入再重试，例如：

```sh
tail -n 100 error.log | ruask -q "如何排查这些错误"
```

按行筛选后仍需保证总字节数不超限；没有内容的 stdin 不会产生额外查询。

- stdout 只包含按行展示的回答，正常结束的最后一行没有换行时补上换行；帮助与配置初始化分别输出帮助和配置位置。
- 无参数且没有 stdin 内容时显示简短帮助，成功退出。帮助与无效输入不需要模型凭据，也不发请求。
- stderr 用于错误信息；已展示部分回答后断流、截断或流内错误时，保留完整行、提示结果不完整并非零退出，不重发另一份回答。
- 角色、空内容、工具和思考数据等非正文不会展示；HTTP 成功本身不代表回答已正常完成。
- 正常结束返回 `0`，输入、配置或模型请求失败返回非零退出码。
- 下游提前关闭输出时安静结束，例如 `ruask git | head -n 1`。

## 代码结构

```text
src/
  main.rs          参数、stdin、执行顺序、输出和退出码
  query.rs         命令查询、语言与 locale 解析、目标平台
  config.rs        用户配置定位、初始化、环境覆盖与字段校验
  model.rs         查询指令、协议选择与 OpenAI 流式请求
  model/anthropic.rs  原生 Messages 请求、文本块与完成状态
  model/sse.rs     两类协议共用的 SSE 事件帧读取
  output.rs        缓冲未完成文本行，逐行输出与刷新
tests/
  cli.rs           启动真实 CLI，用本地 HTTP 服务代替模型
  config.rs        隔离用户目录，验证配置文件与真实查询
  streaming.rs     受控服务握手、拆片、断流与下游关闭
docs/
  learning.md      Rust 前置知识、阅读顺序和 TDD 开发过程
```

建议从 [学习指南](docs/learning.md) 开始，再沿着 `main → run` 阅读源码。

## 验证

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

测试不需要真实模型密钥，会监听本机随机端口。
