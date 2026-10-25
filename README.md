# ruask

接收文本，询问模型，输出文本。一个用于 Shell 工作流的 Rust 命令行工具。

## 开始使用

需要支持 Rust 2024 edition 的 Rust 工具链。

```sh
cargo build --release

# 换成你的服务地址、密钥和模型名。
export OPENAI_BASE_URL="https://your-provider.example/v1"
export OPENAI_API_KEY="你的密钥"
export OPENAI_MODEL="你的模型名"

./target/release/ruask "解释 Rust 的所有权"
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
ruask "解释 Rust 的所有权"
cat question.txt | ruask
cat error.log | ruask "分析错误原因"
git diff | ruask "用中文总结这些修改" > summary.md
ruask --help
```

## 输入和输出约定

| 输入 | 发送给模型的内容 |
| --- | --- |
| `ruask "问题"` | 问题 |
| `cat file \| ruask` | 文件原文 |
| `cat file \| ruask "总结"` | 指令 + 一个空行 + 文件原文 |

多个提问参数用空格连接；需要保留准确的空格或 Shell 特殊字符时，用引号括起提问。
空白输入会报错。有效输入的空格、换行会保留。

- stdout 只包含回答，并在末尾追加一个换行；`--help` 单独使用时输出帮助。
- stderr 用于错误信息。
- 正常结束返回 `0`，输入、配置或模型请求失败返回非零退出码。
- 下游提前关闭输出时安静结束，例如 `ruask "生成一些文本" | head -n 1`。

## 代码结构

```text
src/
  main.rs          参数、stdin、执行顺序、输出和退出码
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
