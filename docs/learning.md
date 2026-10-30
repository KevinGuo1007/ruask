# 从第一条命令查询理解 ruask

## 先理解程序要做什么

把一次执行想成一条直线：

```text
命令行参数 + stdin
        ↓
    保留命令查询、附带问题与原始输出上下文
        ↓
    读取模型配置
        ↓
    选择结果语言与目标平台
        ↓
    请求模型并逐步解析 SSE 正文
        ↓
    完整行写入 stdout 并刷新
```

任何一步失败，错误都返回到入口，由入口写入 stderr 并返回失败退出码。
`--help` 或 `-h` 会在参数解析后提前输出帮助，不读取 stdin，也不需要配置。
`--init-config` 创建用户配置模板并显示位置，保留已有文件，同样不要求模型凭据。
无参数且没有 stdin 内容时也会成功显示帮助；显式空白查询则报错。

当前增量支持单次命令查询与有限管道输出解读，支持已知命令、子命令、自然语言任务与 `-q` / `--question` 附带问题。使用同步 HTTP 请求，让代码的阅读顺序就是执行顺序。
文件读取、重定向、文本筛选交给 Shell 提供的工具完成。

## 源码如何分工

`src/main.rs` 处理 Shell 能观察到的行为，包括输入、输出、错误和退出码。
阅读顺序是：

1. `main()`：成功返回 `0`，失败显示错误并返回非零值。
2. `run()`：解析参数、查看帮助，或依次读取查询、读取配置、调用模型、输出回答。
3. `Arguments::parse()`：识别问题、语言、平台、颜色选项与 `--` 分隔符，拒绝未知选项与缺少的值。
4. `read_query()`：拒绝显式空白查询，读取最多 65,537 字节以检查 65,536 字节上限，构造查询或表示应显示帮助的 `None`。
5. `write_output()`：输出帮助和初始化位置；模型正文交给 `output::Lines`。
6. `main()` 同样将模型输出的 Broken pipe 视为正常结束。

`src/query.rs` 的 `Query` 分开保存命令查询、附带问题与原始输出上下文；`Language` 和 `Platform`
限定首版支持的选择，`Language::from_locale()` 按 LANG/LANGUAGE 规则选择语言。
`run()` 依次考虑 CLI 语言、文件偏好与 locale，并选择 CLI 平台或当前主机平台。

`src/config.rs` 负责用户配置：

1. `initialize()`：定位 XDG 文件，创建仅用户可读写的模板，保留已有文件。
2. `load()`：读取用户 TOML，选择协议，再逐字段应用对应的环境变量覆盖。
3. `setting()`：要求地址、密钥、模型非空，错误中只指出设置名称和配置位置。

`src/model.rs` 集中处理模型服务相关的知识：

1. `ask()`：附加共同的命令行领域指令，按显式配置选择协议；OpenAI 将指令放在 system 消息，Anthropic 放在顶层 system。
2. `model/sse.rs` 按空行读取完整 SSE 事件，两类协议各自解释 data JSON，不直接显示协议内容。
3. OpenAI 要求 `stop` 完成原因与 `[DONE]`；`model/anthropic.rs` 处理 named SSE 的消息、文本块与结束状态，并可解析普通 JSON 的文本块。异常 EOF、截断、流内错误返回不完整错误。
4. `read_response()` 区分 HTTP 错误、普通 JSON 与 SSE；完整 JSON 直接复用并写 stderr 提醒。
5. `model/compatibility.rs` 只把明确的 stream 能力拒绝变成可回退错误。`ask()` 在尚未展示正文时最多切换一次 stream=false，模型、地址和查询保持一致；普通错误和部分输出后失败不重试。
6. Anthropic 使用 Bearer 认证、固定 API 版本 `2026-10-21` 和必要的 max_tokens；非单 workspace 密钥另由 ANTHROPIC_WORKSPACE_ID 提供元数据。

`src/output.rs` 的 `Lines` 接收正文片段，只缓冲尚未完成的行；每行写出后立即刷新。
正常结束才输出无换行尾行；失败时未完成的尾行不冒充完整内容。兼容回退只能清除尚未展示的片段，已展示内容禁止重放。
`Color` 解析 auto/always/never，auto 依据 stdout 的 `is_terminal()` 和非空 `NO_COLOR` 选择；
always 显式覆盖环境偏好。渲染器先去掉模型终端控制序列，再按行过滤围栏；
普通用途说明保持常规样式，缩进或围栏内的命令为青色，`{{name}}` 占位符为黄色。
这些样式由本地添加，纯文本模式保留实际命令和占位符字符。

主流程只需要知道这两个入口：

```rust
config::load()
ask(&config, &query, language, platform, &mut output)
```

配置解析由配置模块负责，协议数据保持在模型模块内部，正文的完整行展示由输出模块负责。

## 读代码前要认识的 Rust 写法

### `String`、借用和可变借用

`String` 拥有一段文本，离开作用域时会释放它占用的内存。
`&str` 是对文本的借用；`&Config` 是对配置的借用。

```rust
model::ask(&config, &query, language, platform, &mut output)?;
```

这里没有把配置和提问的所有权交给 `ask()`。它借用配置和查询，同时可变借用输出器，
让解析到的正文逐步进入输出器。`&String` 可以自动转换为函数需要的 `&str`。

```rust
let mut bytes = Vec::new();
stdin.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
```

`mut` 允许修改变量，`&mut bytes` 允许被调用的函数填充字节。
`take` 限制最多读取上限加一字节；`read_to_end` 在读到 EOF 或达到该数量时返回。
管道上游退出或文件读完会产生 EOF。多出的一个字节用于检测超限，检查通过后再解码 UTF-8。
程序先用 `is_terminal()` 判断输入是否来自终端，避免没有管道时等你手动输入。

### `Vec<String>`、切片和 `join`

`Vec<String>` 是一组可变长度的字符串。`std::env::args()` 包含程序名，
所以 `skip(1)` 跳过它；解析器用 `next()` 逐个读取剩余参数。

`Arguments` 将位置参数保存在 `Vec<String>` 中，问题选项单独保存。
`args.command.join(" ")` 用空格连接位置参数，生成一个新的 `String`。

`Query` 将命令查询、附带问题和管道内容分别传给模型，避免把输出上下文当成命令或用户指令。
`trim()` 只用于判空，原文仍保留。输出解读要求解释含义、明显错误与相关下一步命令，不凑示例数量。

### `Option` 和 `Result` 有什么区别

`Option<T>` 表示“有值 `Some(T)`”或“没有值 `None`”。
例如模型的 `choices` 列表可能为空，取第一个元素时得到 `Option<Choice>`。

`Result<T, E>` 表示“成功 `Ok(T)`”或“失败 `Err(E)`”，失败时还携带原因。

```rust
fn ask(config: &Config, query: &Query, language: Language, platform: Platform, output: &mut Lines<impl Write>) -> Result<(), Box<dyn Error>>
```

这行可以先读成：“读取配置和提问，逐步交付正文；成功返回空值，失败返回错误。”
`Box<dyn Error>` 是能容纳不同错误类型的统一错误容器。现在知道它可以承接网络、
输入输出等错误就够了，之后再深入 `Box`、trait 和动态分发。

`Result<(), ...>` 中的 `()` 表示成功时没有额外数据需要返回。

### `?`、`map_err` 和 `ok_or`

```rust
let config = config::load()?;
```

`?` 的含义是：如果成功，取出配置；如果失败，提前把错误返回给调用者。
它不会自动打印错误。本项目最终在 `main()` 中统一打印。

`map_err(...)` 给错误补充上下文，例如把 HTTP 错误变成“模型请求失败：HTTP 状态”。
`|error| ...` 是闭包，可以先理解成一个接收 `error` 的小函数。

`ok_or(...)` 把 `Option` 转成 `Result`：有值就是成功，没有值就使用指定错误。
所以从空的回答列表取第一个元素时，会正常报错。

生产代码在这些可预期失败处返回错误。测试中的 `unwrap()` 则表示：
如果连测试进程都启动不了，就立刻让测试失败并显示原因。

### 模块、结构体和 JSON

`mod model;` 加载同目录的 `model.rs`。`pub` 允许其他模块使用某个定义。
`struct Config` 把协议类型与地址、密钥、模型字段放在一起。

`json!` 构造请求内容，负责正确转义引号和换行：

```json
{
  "model": "你配置的模型名",
  "messages": [
    {"role": "system", "content": "命令行用途与简洁纯文本回答指令"},
    {"role": "user", "content": "{\"command_query\":\"git commit\",\"question\":null,\"language\":\"zh\",\"platform\":\"osx\"}"}
  ]
}
```

配置中的 `#[derive(Deserialize)]` 让 serde 为结构体生成从 TOML 读取数据的代码。
流式响应以 SSE `data:` 事件交付 JSON，正文来自第零个 choice 的 `delta.content`。
角色、用量、工具与思考数据不作为正文；没有有效文本或缺少正常完成信号时返回错误。

### `Write` 和 Broken pipe

`Read`、`Write` 是提供读写方法的 trait。在这里，先把它们理解成导入后可以使用
`read_to_string()`、`write_all()` 等方法的能力。

`writeln!` 写文本并追加换行，返回 `Result`。如果下游已经关闭，会产生 Broken pipe。
本项目把这种情况当作正常结束；其他写入失败仍然向上返回。

## 最初 CLI 的 TDD 过程记录

先看 `Cargo.toml` 中各个依赖的用途：

| 依赖 | 用途 |
| --- | --- |
| `ureq` | 同步发送 HTTP 请求、接收响应 |
| `serde` | 读取配置结构、序列化语言与平台 |
| `serde_json` | 构造请求 JSON、解析 SSE 事件 JSON |
| `toml` | 读取用户配置 |
| `tempfile` | 仅用于测试，隔离用户目录和工作目录 |
| `mockito` | 仅用于测试，启动本地 HTTP 服务 |
| `libc` | 仅用于测试，在 macOS/Linux 创建伪终端验证自动颜色 |

`[dev-dependencies]` 下的依赖不参与正常应用构建。`Cargo.lock` 记录解析后的具体版本，
因此会比 `Cargo.toml` 中的直接依赖列表长很多；其中也包含测试依赖及其间接依赖。

我们约定的测试入口是实际 CLI 进程：参数、环境变量、stdin 是输入，stdout、stderr、
退出码是结果。模型服务是外部依赖，由本地 HTTP 服务替代。

因此测试会经过真实的输入读取、配置读取、HTTP 请求、JSON 解析和输出。
下表记录初始通用文本 CLI 的开发过程；当前已收敛为命令查询，空调用成功显示帮助，
旧的通用问答断言已替换，具体契约以 `tests/cli.rs` 为准。
测试不会直接调用 `read_query()` 或断言私有函数之间的调用顺序。

第一轮可以这样描述：

> 给定一个问题和模型配置，执行 ruask，得到模型返回的回答，退出成功且没有错误输出。

测试有三个部分：

1. 准备：本地服务只接受指定的问题、模型名和认证头，返回固定的 SSE 正文事件和完成信号。
2. 执行：通过 `Command` 启动真正的 ruask。
3. 断言：stdout 恰好是固定回答，stderr 为空，退出成功。

`#[test]` 声明测试函数，`assert_eq!` 比较实际值和预先写定的期望值。
`env!("CARGO_BIN_EXE_ruask")` 是 Cargo 提供的可执行文件路径。
测试助手的 `env_clear()` 只清空子进程的环境，避免读到你的真实配置。
配置测试另用临时 HOME、XDG 根和工作目录，验证文件权限、优先级及零请求失败行为。

随后按下面的顺序推进。每行都是先增加一项失败测试，再补足对应实现：

| 行为 | red 时观察到的问题 | green 所需的变化 |
| --- | --- | --- |
| 直接提问 | 仍输出“准备发送的问题” | 配置、HTTP 请求和回答解析 |
| 纯管道输入 | 提示没有问题 | 读取非终端 stdin |
| 指令搭配管道 | 请求中只有指令 | 先读取两种输入，再组合 |
| 空白输入 | 继续读取配置 | 对输入进行空白检查 |
| 缺失或空白配置 | 提示无法指出变量 | 读取配置时检查并说明变量名 |
| HTTP 请求失败 | 错误缺少操作上下文 | 为请求错误补充说明 |
| 不可用的模型响应 | 解析错误不清楚，空文本可被接受 | 补充解析错误说明并拒绝空回答 |
| 基础地址末尾有斜杠 | 请求路径出现双斜杠 | 拼接前去掉末尾斜杠 |
| 多个提问参数 | 只保留第一个词 | 收集全部参数并连接 |
| 帮助 | 被当作提问，要求配置 | 提前识别独立的帮助参数 |
| 下游关闭输出 | 打印时 panic | 处理 Broken pipe |

下载失败、编译错误、测试服务无法监听端口，都需要先解决，才能观察有效的 red。
red 应该说明缺少哪项行为，而不只是命令返回了失败。

## 以后怎样增加一个功能

先用一个具体命令描述期望行为，说明输入、输出和失败条件，然后只写这一项测试。

```sh
# 先只运行这一项，确认它因为缺少目标行为而失败。
cargo test --test cli 你的测试函数名

# 补上最少的实现，再运行同一项。
cargo test --test cli 你的测试函数名

# 通过后检查已有行为。
cargo test
```

不要先写完一大批测试，再一次性实现全部功能。每一轮的结果会帮助你决定下一步。
这次按照指定的 TDD 规则，没有额外加入重构阶段；后续需要调整结构时单独评审。

建议你第一次阅读时，先跑 `cargo test --test cli command_query_is_sent_to_model_and_answer_is_written_to_stdout`，
读懂这一项，再对照 `run()` 和 `ask()`。测试中的固定回答来自本地服务；实际模型效果
需要你配置真实服务后自行体验。

## 流式展示的验证

`tests/streaming.rs` 的首行测试让服务先发送可形成完整行的字节片段，随后等待测试读到
CLI stdout 首行才继续发送尾部和完成信号。这个握手证明客户端没有等待整份回答。
逐字节写入覆盖 Unicode 与 SSE 字段边界；补充场景检查空事件、CRLF、多行 data、
截断、异常 EOF、流内错误，以及下游关闭后不再等待服务完成。
`tests/rendering.rs` 另用真实伪终端检查自动颜色，用普通 stdout 管道检查纯文本与显式模式；
PTY slave 在读取缓冲输出前保持打开，防止 macOS 在最终关闭时丢弃未读输出。
渲染首行同样通过服务/stdout 握手验证，覆盖跨围栏、占位符和 Unicode 的片段。
这些测试只证明客户端契约，不证明真实模型质量或延迟。

## 组合工作流的验证

`tests/config.rs` 从真实 `--init-config` 生成的模板开始，只编辑一次两组独立配置，然后切换工作目录。
组合场景依次验证 OpenAI 命令查询、Anthropic 独立问题的 JSON 直接复用，以及中文、Linux 目标、
命令、附带问题和 Unicode 输出上下文同时存在时的解读。固定回答只验证展示，请求中的字段才验证输入语义。

另一场景对每种协议连续运行两次相同查询，把 stdout 直接重定向到文件。每次都观察到新的
`stream=true` 请求与明确不支持后的一次 `stream=false` 请求；文件只有去围栏的纯文本回答，
stderr 各有一条提醒。这同时覆盖配置持久性、无答案/能力缓存和 shell 通道约定。
这些组合已经由前序功能支持，新增测试直接通过不构成新的红→绿证据；本票实际红→绿是完整帮助的缺口。

真实模型质量与速度需要另一套证据。[验收清单](acceptance.md)给出四类任务、两语言、两目标平台和
两协议的矩阵，以及首个正文、首条可用命令和完整回答的计时定义。尚未执行的真实服务项目保留未执行状态。

## 官方参考

- [Rust：所有权](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
- [Rust：引用与借用](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
- [Rust：使用 Result 处理错误](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
- [Cargo：测试与可执行文件路径](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [OpenAI Chat Completions 流式事件](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events)
- [OpenAI Chat Completions 请求](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
- [ureq：同步 HTTP 与 JSON](https://docs.rs/ureq/latest/ureq/)
- [mockito：本地 HTTP 测试服务](https://docs.rs/mockito/latest/mockito/)
