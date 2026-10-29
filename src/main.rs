mod config;
mod model;
mod query;

use std::error::Error;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

const HELP: &str = "用法：ruask [命令或命令行任务...] [-q \"附带问题\"]
      ruask -q \"命令行问题\"
      ruask -- 命令 [目标命令的选项...]
      cat file | ruask [命令...]

查询命令、子命令或自然语言命令行任务，输出简短的纯文本速查结果。
例如：ruask git commit -q \"如何修改上一次提交的信息\"
目标命令自带选项时，请整体加引号或放在 -- 后。
目前参数与 stdin 同时存在时以空行组合。

用户配置：$XDG_CONFIG_HOME/ruask/config.toml（默认 $HOME/.config/ruask/config.toml）
模型环境变量（可覆盖用户配置）：
  OPENAI_BASE_URL  接口基础地址（包含服务所需路径，例如 /v1）
  OPENAI_API_KEY   接口密钥
  OPENAI_MODEL     模型名称
  RUASK_API_TYPE   openai / anthropic（当前支持 OpenAI 兼容请求）

选项：
  -q, --question <字符串>  附带问题，也可独立使用
  --init-config           创建用户配置模板，保留已有文件
  -h, --help               显示帮助";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ruask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = Arguments::parse()?;
    if args.help {
        return write_output(HELP);
    }
    if args.init_config {
        return write_output(&config::initialize()?);
    }

    let Some(query) = read_query(args)? else {
        return write_output(HELP);
    };
    let config = config::load()?;
    let answer = model::ask(&config, &query)?;
    write_output(&answer)
}

fn write_output(text: &str) -> Result<(), Box<dyn Error>> {
    if let Err(error) = writeln!(io::stdout().lock(), "{text}") {
        // 例如下游 head 已经退出；这是一条正常结束的管道。
        if error.kind() != io::ErrorKind::BrokenPipe {
            return Err(error.into());
        }
    }
    Ok(())
}

#[derive(Default)]
struct Arguments {
    command: Vec<String>,
    question: Option<String>,
    help: bool,
    init_config: bool,
}

impl Arguments {
    fn parse() -> Result<Self, Box<dyn Error>> {
        let mut parsed = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--" => {
                    parsed.command.extend(args);
                    break;
                }
                "-h" | "--help" => parsed.help = true,
                "--init-config" => parsed.init_config = true,
                "-q" | "--question" => {
                    let value = args
                        .next()
                        .filter(|value| {
                            !matches!(
                                value.as_str(),
                                "--" | "-q" | "--question" | "-h" | "--help" | "--init-config"
                            )
                        })
                        .ok_or_else(|| {
                            format!("选项 {arg} 需要一个字符串值；例如 {arg} \"如何查看端口占用\"")
                        })?;
                    if parsed.question.replace(value).is_some() {
                        return Err("-q / --question 只能提供一次".into());
                    }
                }
                _ if arg.starts_with('-') => {
                    return Err(format!(
                        "未知选项 {arg}；查询目标命令的选项时，请整体加引号或使用 -- 分隔"
                    )
                    .into());
                }
                _ => parsed.command.push(arg),
            }
        }
        if parsed.init_config && (!parsed.command.is_empty() || parsed.question.is_some()) {
            return Err("--init-config 需单独使用".into());
        }
        Ok(parsed)
    }
}

fn read_query(args: Arguments) -> Result<Option<query::Query>, Box<dyn Error>> {
    let command = args.command.join(" ");
    if (!args.command.is_empty() && command.trim().is_empty())
        || args.question.as_ref().is_some_and(|q| q.trim().is_empty())
    {
        return Err("命令查询或附带问题不能为空白".into());
    }

    let mut input = String::new();
    let mut stdin = io::stdin();
    if !stdin.is_terminal() {
        stdin.read_to_string(&mut input)?;
    }

    if args.command.is_empty() && args.question.is_none() && input.is_empty() {
        return Ok(None);
    }

    // 沿用现有管道组合方式；输出上下文的新语义由后续增量交付。
    let prompt = if command.is_empty() {
        input
    } else if input.trim().is_empty() {
        command
    } else {
        format!("{command}\n\n{input}")
    };
    if prompt.trim().is_empty() && args.question.is_none() {
        return Err("命令查询或 stdin 不能为空白".into());
    }
    Ok(Some(query::Query {
        command_query: if prompt.trim().is_empty() {
            None
        } else {
            Some(prompt)
        },
        question: args.question,
    }))
}
