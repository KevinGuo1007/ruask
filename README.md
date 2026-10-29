# ruask

在终端内查询命令、子命令或自然语言命令行任务，获得简短的速查结果。

## 开始使用

需要支持 Rust 2024 edition 的 Rust 工具链。

```sh
cargo build --release

# 换成你的服务地址、密钥和模型名。
export OPENAI_BASE_URL="https://your-provider.example/v1"
export OPENAI_API_KEY="你的密钥"
export OPENAI_MODEL="你的模型名"

./target/release/ruask git commit
```

`OPENAI_BASE_URL` 是基础地址，包含服务所需的路径前缀。程序在后面追加
`/chat/completions`，地址末尾有无 `/` 都可以。三个环境变量均为必需，程序直接读取
进程环境，不自动加载 `.env` 文件。

支持 OpenAI 兼容的 Chat Completions 文本接口，发送一次请求，完整接收回答后输出。
模型请求的总超时为 60 秒。

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
ruask "git commit --amend"
ruask -- git commit --amend
ruask git status > usage.txt
ruask --help
```

## 输入和输出约定

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

现有 stdin 路径继续可用：`cat error.log | ruask` 传递原文；
`cat error.log | ruask cargo build` 用一个空行组合位置参数与原文。
有效内容的空格和换行会保留。管道输出上下文的独立语义与输入上限由后续增量实现。

- stdout 只包含回答，并在末尾追加一个换行；`-h` / `--help` 输出帮助。
- 无参数且没有 stdin 内容时显示简短帮助，成功退出。帮助与无效输入不需要模型凭据，也不发请求。
- stderr 用于错误信息。
- 正常结束返回 `0`，输入、配置或模型请求失败返回非零退出码。
- 下游提前关闭输出时安静结束，例如 `ruask git | head -n 1`。

## 代码结构

```text
src/
  main.rs          参数、stdin、执行顺序、输出和退出码
  query.rs         命令查询与附带问题的数据结构
  model.rs         环境变量配置、模型请求和回答解析
tests/
  cli.rs           启动真实 CLI，用本地 HTTP 服务代替模型
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
