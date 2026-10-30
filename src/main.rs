mod config;
mod model;
mod output;
mod query;

use std::error::Error;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use query::{Language, Platform};

const HELP: &str = "用法：ruask [命令或命令行任务...] [-q \"附带问题\"]
      ruask -q \"命令行问题\"
      ruask -- 命令 [目标命令的选项...]
      cat file | ruask [命令...] [-q \"附带问题\"]

查询命令、子命令或自然语言命令行任务，输出简短的纯文本速查结果。
例如：ruask git commit -q \"如何修改上一次提交的信息\"
目标命令自带选项时，请整体加引号或放在 -- 后。
默认流式接收，完整行到达即显示；断流或截断会保留已显示内容并报不完整。
stdin 作为原始输出上下文，读到结束后解读含义、明显错误及相关命令。
管道上限为 65,536 字节（64 KiB）；超限请先筛选或使用 tail，不会截断或分段。

用户配置：$XDG_CONFIG_HOME/ruask/config.toml（默认 $HOME/.config/ruask/config.toml）
模型环境变量（可覆盖用户配置）：
  OPENAI_BASE_URL  接口基础地址（包含服务所需路径，例如 /v1）
  OPENAI_API_KEY   接口密钥
  OPENAI_MODEL     模型名称
  ANTHROPIC_BASE_URL / ANTHROPIC_API_KEY / ANTHROPIC_MODEL  原生 Messages 配置
  ANTHROPIC_WORKSPACE_ID  非单 workspace 密钥所需的可选 workspace 元数据
  RUASK_API_TYPE   openai / anthropic（Chat Completions / 原生 Messages）

选项：
  -q, --question <字符串>  附带问题，也可独立使用
  -L, --language <en|zh>   结果语言：英文或简体中文（覆盖文件与 locale）
  -p, --platform <平台>    目标平台：linux / osx / macos（默认当前主机）
  --init-config           创建用户配置模板，保留已有文件
  -h, --help               显示帮助";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
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

    let Some(query) = read_query(&args)? else {
        return write_output(HELP);
    };
    let config = config::load()?;
    let language = match args.language {
        Some(language) => language,
        None => match config.language.as_deref() {
            Some(language) => Language::parse(language)?,
            None => Language::from_locale(),
        },
    };
    let platform = match args.platform {
        Some(platform) => platform,
        None => Platform::host()?,
    };
    let mut output = output::Lines::new(io::stdout().lock());
    model::ask(&config, &query, language, platform, &mut output)
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
    language: Option<Language>,
    platform: Option<Platform>,
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
                "-L" | "--language" => {
                    let value = option_value(&arg, &mut args)?;
                    parsed.language = Some(Language::parse(&value)?);
                }
                "-p" | "--platform" => {
                    let value = option_value(&arg, &mut args)?;
                    parsed.platform = Some(Platform::parse(&value)?);
                }
                "-q" | "--question" => {
                    let value = option_value(&arg, &mut args)?;
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
        if parsed.init_config
            && (!parsed.command.is_empty()
                || parsed.question.is_some()
                || parsed.language.is_some()
                || parsed.platform.is_some())
        {
            return Err("--init-config 需单独使用".into());
        }
        Ok(parsed)
    }
}

fn option_value(
    option: &str,
    args: &mut impl Iterator<Item = String>,
) -> Result<String, Box<dyn Error>> {
    args.next()
        .filter(|value| {
            !matches!(
                value.as_str(),
                "--" | "-q"
                    | "--question"
                    | "-L"
                    | "--language"
                    | "-p"
                    | "--platform"
                    | "-h"
                    | "--help"
                    | "--init-config"
            )
        })
        .ok_or_else(|| format!("选项 {option} 需要一个字符串值").into())
}

fn read_query(args: &Arguments) -> Result<Option<query::Query>, Box<dyn Error>> {
    let command = args.command.join(" ");
    if (!args.command.is_empty() && command.trim().is_empty())
        || args.question.as_ref().is_some_and(|q| q.trim().is_empty())
    {
        return Err("命令查询或附带问题不能为空白".into());
    }

    const MAX_INPUT_BYTES: u64 = 65_536;
    let stdin = io::stdin();
    let mut bytes = Vec::new();
    if !stdin.is_terminal() {
        stdin.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err("stdin 超过 65,536 字节（64 KiB）；请先筛选或使用 tail 缩小输入后重试，不会截断或分段发送".into());
        }
    }
    let input = String::from_utf8(bytes).map_err(|_| "stdin 必须是有效 UTF-8 文本")?;

    if args.command.is_empty() && args.question.is_none() && input.is_empty() {
        return Ok(None);
    }

    if command.trim().is_empty() && input.trim().is_empty() && args.question.is_none() {
        return Err("命令查询或 stdin 不能为空白".into());
    }
    Ok(Some(query::Query {
        command_query: if args.command.is_empty() {
            None
        } else {
            Some(command)
        },
        question: args.question.clone(),
        output_context: if input.is_empty() { None } else { Some(input) },
    }))
}
