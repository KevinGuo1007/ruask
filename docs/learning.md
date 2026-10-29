# 从第一条命令查询理解 ruask

## 先理解程序要做什么

把一次执行想成一条直线：

```text
命令行参数 + stdin
        ↓
    组成命令查询与附带问题
        ↓
    读取模型配置
        ↓
    请求模型并解析回答
        ↓
    把回答写入 stdout
```

任何一步失败，错误都返回到入口，由入口写入 stderr 并返回失败退出码。
`--help` 或 `-h` 会在参数解析后提前输出帮助，不读取 stdin，也不需要配置。
无参数且没有 stdin 内容时也会成功显示帮助；显式空白查询则报错。

当前增量只做单次命令查询，支持已知命令、子命令、自然语言任务与 `-q` / `--question` 附带问题。使用同步 HTTP 请求，让代码的阅读顺序就是执行顺序。
文件读取、重定向、文本筛选交给 Shell 提供的工具完成。

## 源码如何分工

`src/main.rs` 处理 Shell 能观察到的行为，包括输入、输出、错误和退出码。
阅读顺序是：

1. `main()`：成功返回 `0`，失败显示错误并返回非零值。
2. `run()`：解析参数、查看帮助，或依次读取查询、读取配置、调用模型、输出回答。
3. `Arguments::parse()`：识别问题选项与 `--` 分隔符，拒绝未知选项与缺少的值。
4. `read_query()`：拒绝显式空白查询，读取管道，构造查询或表示应显示帮助的 `None`。
5. `write_output()`：输出文本，处理下游已经关闭的情况。

`src/query.rs` 的 `Query` 分开保存命令查询与附带问题。

`src/model.rs` 集中处理模型服务相关的知识：

1. `Config::from_env()`：从三个环境变量构造配置。
2. `required_env()`：检查变量存在且不为空，错误中指出变量名。
3. `ask()`：附加命令行领域与简洁回答指令，发送请求，把响应转成回答字符串。
4. `ChatResponse`、`Choice`、`Message`：描述要从 JSON 中读取的字段。

主流程只需要知道这两个入口：

```rust
Config::from_env()
ask(&config, &query)
```

配置的字段和 JSON 响应结构保持在模块内部。暂时没有需要多种实现切换的地方，
因此用具体函数即可。

## 读代码前要认识的 Rust 写法

### `String`、借用和可变借用

`String` 拥有一段文本，离开作用域时会释放它占用的内存。
`&str` 是对文本的借用；`&Config` 是对配置的借用。

```rust
let answer = model::ask(&config, &query)?;
```

这里没有把配置和提问的所有权交给 `ask()`。它在调用期间读取它们，返回一个自己
拥有的 `String`。`&String` 可以自动转换为函数需要的 `&str`。

```rust
let mut input = String::new();
stdin.read_to_string(&mut input)?;
```

`mut` 允许修改变量，`&mut input` 允许被调用的函数修改这段字符串。
`read_to_string` 会一直读到 EOF，也就是输入结束。管道上游退出或文件读完会产生 EOF。
程序先用 `is_terminal()` 判断输入是否来自终端，避免没有管道时等你手动输入。

### `Vec<String>`、切片和 `join`

`Vec<String>` 是一组可变长度的字符串。`std::env::args()` 包含程序名，
所以 `skip(1)` 跳过它；解析器用 `next()` 逐个读取剩余参数。

`Arguments` 将位置参数保存在 `Vec<String>` 中，问题选项单独保存。
`args.command.join(" ")` 用空格连接位置参数，生成一个新的 `String`。

当前命令查询和管道内容都有有效文本时，`format!("{command}\n\n{input}")`
生成组合后的提问。`trim()` 只用于判空，原文仍保留。

### `Option` 和 `Result` 有什么区别

`Option<T>` 表示“有值 `Some(T)`”或“没有值 `None`”。
例如模型的 `choices` 列表可能为空，取第一个元素时得到 `Option<Choice>`。

`Result<T, E>` 表示“成功 `Ok(T)`”或“失败 `Err(E)`”，失败时还携带原因。

```rust
fn ask(config: &Config, query: &Query) -> Result<String, Box<dyn Error>>
```

这行可以先读成：“读取配置和提问；成功返回回答文本，失败返回错误。”
`Box<dyn Error>` 是能容纳不同错误类型的统一错误容器。现在知道它可以承接网络、
输入输出等错误就够了，之后再深入 `Box`、trait 和动态分发。

`Result<(), ...>` 中的 `()` 表示成功时没有额外数据需要返回。

### `?`、`map_err` 和 `ok_or`

```rust
let config = model::Config::from_env()?;
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
`struct Config` 把三个配置字段放在一起，`impl Config` 为它定义相关函数。

`json!` 构造请求内容，负责正确转义引号和换行：

```json
{
  "model": "你配置的模型名",
  "messages": [
    {"role": "system", "content": "命令行用途与简洁纯文本回答指令"},
    {"role": "user", "content": "{\"command_query\":\"git commit\",\"question\":null}"}
  ]
}
```

`#[derive(Deserialize)]` 让 serde 为结构体生成从 JSON 读取数据的代码。
响应中我们只关心 `choices[0].message.content`；额外字段会被忽略。
`into_iter().next()` 取得第一个回答，并接收它的所有权；如果没有回答或内容为空，
程序返回错误。

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
| `serde` | 把响应 JSON 转成 Rust 结构体 |
| `serde_json` | 用 `json!` 构造请求 JSON |
| `mockito` | 仅用于测试，启动本地 HTTP 服务 |

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

1. 准备：本地服务只接受指定的问题、模型名和认证头，返回固定 JSON。
2. 执行：通过 `Command` 启动真正的 ruask。
3. 断言：stdout 恰好是固定回答，stderr 为空，退出成功。

`#[test]` 声明测试函数，`assert_eq!` 比较实际值和预先写定的期望值。
`env!("CARGO_BIN_EXE_ruask")` 是 Cargo 提供的可执行文件路径。
测试助手的 `env_clear()` 只清空子进程的环境，避免读到你的真实配置。

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

## 官方参考

- [Rust：所有权](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
- [Rust：引用与借用](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
- [Rust：使用 Result 处理错误](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
- [Cargo：测试与可执行文件路径](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [ureq：同步 HTTP 与 JSON](https://docs.rs/ureq/latest/ureq/)
- [mockito：本地 HTTP 测试服务](https://docs.rs/mockito/latest/mockito/)
