# 开发指南

ruask 是用于命令速查和有限命令输出解读的 Rust CLI。先阅读 [README](../README.zh-CN.md) 和[行为参考](reference.md)，再从 `src/main.rs` 的 `main()`、`run()` 进入调用链。需要支持 Rust 2024 edition 的工具链；`Cargo.lock` 应随应用仓库一起维护。

## 构建与检查

```sh
cargo build --locked --offline
cargo fmt --check
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
```

离线构建要求依赖已在本机缓存。首次准备依赖时需要联网，产品测试只使用本机 HTTP 替身与假密钥，详见[测试说明](testing.md)。

## 模块职责

| 模块 | 职责 |
| --- | --- |
| `src/main.rs` | 参数（含 `--opt=值`、`-Lzh` 拆分与 `--version`）、有限 stdin（超过 1 s 提示一次）、语言和平台选择、执行顺序、stdout/stderr 与退出码 |
| `src/query.rs` | 查询语义、语言和 locale 归一、目标平台 |
| `src/config.rs` | XDG 用户文件定位、初始化、协议选择、环境覆盖和字段校验 |
| `src/model.rs` | 领域指令、协议选择、两协议共用的请求设置（建连 8 s 超时后安全重连一次、整体 60 s、不接受压缩）与传输错误提示、OpenAI 回答和有条件的一次非流式回退 |
| `src/model/sse.rs` | SSE 事件分帧，区分网络片段与完整事件 |
| `src/model/anthropic.rs` | 原生 Messages 请求、文本块及完成状态 |
| `src/model/compatibility.rs` | 明确的 streaming 能力拒绝证据与错误分类 |
| `src/output.rs` | 未完成行缓冲、控制序列过滤、语义角色、缩进、本地颜色、占位符与助记符样式、危险命令标注；第一次写出前清除等待提示 |
| `src/waiting.rs` | stderr 是终端时的等待提示线程；120 ms 后开始绘制，`Drop` 时清行 |
| `src/tldr.rs` | 本地 tldr 页面：定位、别名跟随、直出、跨平台提示、拼写纠正、参考选择、`--update-pages` 与索引 |
| `src/environment.rs` | 目标平台为本机时的 shell、包管理器与命令安装状态（只检查 PATH） |
| `src/trace.rs` | 默认关闭的单调计时与数值观测 |

执行顺序是参数与 stdin → 平台 → tldr 路由（命令名完整命中则本地直出并结束）→ 配置 → 语言 → 参考与环境 → 模型响应 → 逐行（说明与用途逐字）展示 → stderr 来源标识。
帮助和配置初始化不需要模型凭据；stdin 最多 65,536 字节，检测时多读一字节，超限在请求前失败。输入原文作为数据保留，不执行其中内容。

模型模块交付文本增量。输出模块对说明（`> `）和用途（`- `，后面不是反引号）行在攒够 8 个字符后开始逐字写出（`stream_pending`），未配对的反引号、行尾空白和折行时未完的单词暂缓；其余行等完整行再写。最终字节与整行渲染一致。只有正常完成才输出尾行。OpenAI 需要正常完成原因及 `[DONE]`，Anthropic 需要正常完成原因和消息结束事件；HTTP 成功或 EOF 本身不足以判定完整回答。

只有明确拒绝 stream 且尚未展示正文时，才允许用相同协议、服务和模型回退一次。鉴权、限流、普通参数及网络错误不触发回退，部分输出后不重放。Broken pipe 视为正常结束，其他错误从入口写 stderr，并按阶段给出退出码（`main.rs` 中的 `Failure`：2 用法/输入、3 配置、4 连接或服务错误、5 回答不完整、1 其他）。

渲染按 `#` 标题、`>` 简介/解释、`-` 用途和整行反引号命令识别角色；未知文本保守显示，不从缩进推断命令。颜色与布局由本地添加；stdout 是终端时按宽度折行说明与用途（东亚宽字符计两列）并给用途编号，命令从不折行。缺少独立命令的用途只提醒一次，不额外请求模型。

## 配置设计

macOS/Linux 统一读取 `$XDG_CONFIG_HOME/ruask/config.toml`，默认 `$HOME/.config/ruask/config.toml`，使升级、安装渠道和工作目录不影响配置。`--init-config` 创建权限为 `0600` 的模板并保留已有文件。程序不加载 `.env` 或当前目录配置。

协议通过 `RUASK_API_TYPE` → 文件 `api_type` → `openai` 选择；按所选协议逐字段合并环境变量和文件，不向另一组借用配置。显式空白值在请求前报错。结果语言依次采用 CLI、用户文件、locale 和英文兜底；目标平台默认主机，可用 CLI 覆盖。完整用法见[行为参考的用户配置](reference.md#用户配置)。

`thinking` 是显式可选的 OpenAI 兼容扩展，未设置就省略，原生 Anthropic 忽略；`reasoning_effort` 不是当前 CLI 的配置字段。配置错误不应打印凭据或原始配置内容。

## 术语

| 术语 | 含义 |
| --- | --- |
| 命令查询 | 查询已知命令、子命令，或用自然语言描述命令行任务的一次请求 |
| 附带问题 | 同一次查询或输出解读中限定需求的问题，不表示多轮会话 |
| 速查结果 | 简短说明及用法示例，采用类 tldr 风格 |
| 输出解读 | 以用户提供的命令输出为上下文，解释含义、错误和相关建议 |
| 协议 | OpenAI Chat Completions 或 Anthropic Messages 的适配方式，不由模型名或域名推断 |
| 首个可见文字 | `first_stdout_text`：第一次向 stdout 写出回答内容，说明行可能尚未结束；`first_stdout_line` 为第一行写完 |
| 命令候选 | 已按格式形成完整行的命令；正确性仍需独立核对 |

## 修改与回归

用一个具体调用说明输入、输出和失败条件；增加或修改功能时优先通过真实 CLI 与本机模型替身验证可观察契约。可先运行单个测试，再跑完整检查。固定回答验证客户端行为，模型知识、平台适用性与答案安全性需要单独验收。

## 打包检查

```sh
cargo package --list --allow-dirty --offline
cargo package --allow-dirty --locked --offline
```

`Cargo.toml` 的 `include` 白名单只收录包元数据、README、许可证文件 `LICENSE`（MIT）、Rust 源码和测试、贡献者文档与行为参考（`docs/*.md` 中列出的四份）；`site/` 官网源码不进入 Cargo 包。增加必要资源时同步更新白名单并检查实际清单；不要把本地凭据、研究数据或缓存放入包。Cargo 会额外生成规范化元数据及原始 manifest 副本。

正式开源发布前需确定许可证并加入许可证文件及对应 Cargo 字段；当前没有选择许可证。还需确定发行版本、验证目标主机并决定发布渠道。包检查成功不表示已发布或完成这些决定。
