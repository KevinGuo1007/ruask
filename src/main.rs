mod config;
mod environment;
mod model;
mod output;
mod query;
mod tldr;
mod trace;
mod waiting;

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
已安装 tldr 页面时，只有命令名的查询（如 ruask tar）直接显示本地页面，不需要网络和模型配置；
带问题或 stdin 的查询会把命中的页面作为参考交给模型。页面来自 tldr-pages（CC BY 4.0）。
命令词之后 ruask 不认识的选项属于目标命令（如 ruask tar -xzvf）；与 -q/-L/-p/--color 等重名时，请整体加引号或放在 -- 后。
默认流式接收：说明随到随显示，命令整行显示；断流或截断会保留已显示内容并报不完整。
明确不支持流式且尚未显示回答时，仅回退一次；完整 JSON 回答直接复用，提醒写 stderr。
stdin 作为原始输出上下文，读到结束后解读含义、明显错误及相关命令。
管道上限为 65,536 字节（64 KiB）；超限请先筛选或使用 tail，不会截断或分段。

用户配置：$XDG_CONFIG_HOME/ruask/config.toml（默认 $HOME/.config/ruask/config.toml）
--init-config 创建 0600 模板并显示位置，保留已有文件；不读取项目 .env。
tldr 页面：RUASK_TLDR_DIR → $XDG_DATA_HOME/ruask/tldr → $HOME/.local/share/ruask/tldr。
协议优先级：RUASK_API_TYPE → 文件 api_type → openai。
所选协议的环境变量逐字段覆盖文件，不借用另一组的地址、模型或密钥。
基础地址保留路径前缀与尾斜线兼容性，分别追加 /chat/completions 或 /messages，不自动补 /v1。
语言优先级：-L / --language → 文件 language → locale → 英文。
模型环境变量（可覆盖用户配置）：
  OPENAI_BASE_URL  接口基础地址（包含服务所需路径，例如 /v1）
  OPENAI_API_KEY   接口密钥
  OPENAI_MODEL     模型名称
  OPENAI_THINKING  可选 enabled / disabled，仅用于支持 thinking.type 的 OpenAI 兼容服务
  ANTHROPIC_BASE_URL / ANTHROPIC_API_KEY / ANTHROPIC_MODEL  原生 Messages 配置
  ANTHROPIC_WORKSPACE_ID  非单 workspace 密钥所需的可选 workspace 元数据
  RUASK_API_TYPE   openai / anthropic（Chat Completions / 原生 Messages）

选项：
  -q, --question <字符串>  附带问题，也可独立使用
  -L, --language <en|zh>   结果语言：英文或简体中文（覆盖文件与 locale）
  -p, --platform <平台>    目标平台：linux / osx / macos（默认当前主机）
  --color <auto|always|never>  颜色：auto 按 stdout 是否终端判断；NO_COLOR 非空时关闭自动颜色
                            always 显式启用（覆盖 NO_COLOR），never 始终纯文本
  --init-config           创建用户配置模板，保留已有文件
  --update-pages [--from <tldr.zip>]  下载并校验官方 tldr 归档（或使用本地归档），安装英文与简体中文页面
  -V, --version            显示版本与本地 tldr 页面状态
  -h                       显示简短帮助
  --help                   显示完整帮助
带值选项也可写成 --language=zh、-Lzh。

首版支持 macOS / Linux、英文和简体中文，仅用于命令速查与输出解读。
模型查询每次都在线请求，不缓存回答、不执行建议命令、不保留对话历史。
目标平台为本机时，请求附带 shell、可用包管理器和所查命令是否已安装（只检查 PATH，不执行程序）。
stdout 输出回答（或明确请求的帮助/配置位置），stderr 输出错误和兼容提醒。
终端中示例带编号、说明按宽度折行（命令不折）；stderr 是终端时，模型回答后显示模型与耗时。
退出码：0 成功，1 其他错误，2 用法或输入错误，3 配置错误，4 无法连接或服务返回错误，5 回答不完整。
RUASK_TRACE=1 可向 stderr 输出仅含时间与数值的阶段记录；默认关闭。";

const SHORT_HELP: &str = "用法：ruask <命令> [子命令...] [-q \"问题\"]
      ruask -q \"命令行任务\"
      命令 2>&1 | ruask [-q \"问题\"]       通过 stdin 解读命令输出
      ruask -- <命令> [它自己的选项...]

例如：ruask tar
      ruask git commit -q \"如何修改上一次提交的信息\"
      ruask -q \"查找大于 100MB 的文件\"
      ruask tar -xzvf                    命令词之后的未知选项属于目标命令

选项：-q, --question <问题>   -L, --language <en|zh>   -p, --platform <linux|osx>
      --color <auto|always|never>   -V, --version   --help（完整帮助）
模型配置：ruask --init-config，或环境变量 OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL。
本地页面：ruask --update-pages 安装后，只有命令名的查询不联网、无需模型配置。";

const SHORT_HELP_EN: &str = "Usage: ruask <command> [subcommand...] [-q \"question\"]
       ruask -q \"command-line task\"
       command 2>&1 | ruask [-q \"question\"]   explain command output read from stdin
       ruask -- <command> [its own options...]

Examples: ruask tar
          ruask git commit -q \"how do I change the last commit message\"
          ruask -q \"find files larger than 100 MB\"
          ruask tar -xzvf                  unknown options after the command belong to it

Options: -q, --question <text>   -L, --language <en|zh>   -p, --platform <linux|osx>
         --color <auto|always|never>   -V, --version   --help (full help)
Model: ruask --init-config, or OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL.
Local pages: after ruask --update-pages, command-only queries work offline without a model.";

const HELP_EN: &str = "Usage: ruask [command or task...] [-q \"question\"]
       ruask -q \"command-line question\"
       ruask -- command [the command's own options...]
       cat file | ruask [command...] [-q \"question\"]

Look up a command, subcommand, or natural-language command-line task and print a short plain-text reference.
Example: ruask git commit -q \"how do I change the last commit message\"
With tldr pages installed, command-only queries (such as ruask tar) show the local page without network or model configuration;
queries with a question or stdin send the matching pages to the model as reference. Pages come from tldr-pages (CC BY 4.0).
Options ruask does not know that follow the command word belong to the target command (ruask tar -xzvf); when they clash with -q/-L/-p/--color, quote the command or put it after --.
Answers stream by default; partial explanation lines appear as they arrive. A broken or truncated stream keeps what was shown and reports it as incomplete.
If the service explicitly rejects streaming before anything is shown, ruask falls back once to a non-streaming request; notices go to stderr.
stdin is raw output context, read to the end and explained (meaning, obvious errors, related commands).
stdin is limited to 65,536 bytes (64 KiB); filter or use tail first. Input is never truncated or split.

User configuration: $XDG_CONFIG_HOME/ruask/config.toml (default $HOME/.config/ruask/config.toml)
--init-config creates a 0600 template and prints its location; an existing file is kept; project .env files are not read.
tldr pages: RUASK_TLDR_DIR → $XDG_DATA_HOME/ruask/tldr → $HOME/.local/share/ruask/tldr.
Protocol: RUASK_API_TYPE → file api_type → openai.
Environment variables of the selected protocol override the file field by field and never borrow the other protocol's address, model, or key.
Base URLs keep their path prefix; /chat/completions or /messages is appended, /v1 is not added automatically.
Language: -L / --language → file language → locale → English.
Model environment variables (override the user configuration):
  OPENAI_BASE_URL  API base URL (including any path the service needs, such as /v1)
  OPENAI_API_KEY   API key
  OPENAI_MODEL     model name
  OPENAI_THINKING  optional enabled / disabled, only for OpenAI-compatible services that support thinking.type
  ANTHROPIC_BASE_URL / ANTHROPIC_API_KEY / ANTHROPIC_MODEL  native Messages settings
  ANTHROPIC_WORKSPACE_ID  optional workspace metadata for keys that span workspaces
  RUASK_API_TYPE   openai / anthropic (Chat Completions / native Messages)

Options:
  -q, --question <text>    a focused question; can also be used alone
  -L, --language <en|zh>   answer language: English or Simplified Chinese
  -p, --platform <name>    target platform: linux / osx / macos (default: this host)
  --color <auto|always|never>  auto follows whether stdout is a terminal; a non-empty NO_COLOR disables auto color
                            always forces color (overrides NO_COLOR), never always prints plain text
  --init-config           create the user configuration template, keeping an existing file
  --update-pages [--from <tldr.zip>]  download and verify the official tldr archive (or use a local one); installs English and Simplified Chinese pages
  -V, --version            show the version and local tldr page status
  -h                       show short help
  --help                   show this full help
Options taking a value also accept --language=zh and -Lzh.

Supports macOS / Linux, English and Simplified Chinese; for command lookup and output explanation only.
Model queries are always sent online; answers are not cached, suggested commands are never run, and no conversation history is kept.
When the target platform is this host, the request includes the shell, available package managers, and whether the queried command is installed (PATH only, nothing is executed).
stdout carries the answer (or the help or configuration location you asked for); stderr carries errors and notices.
In a terminal, examples are numbered and explanations wrap to the width (commands never wrap); when stderr is a terminal, the model and elapsed time follow a model answer.
Exit codes: 0 success, 1 other error, 2 usage or input error, 3 configuration error, 4 connection or service error, 5 incomplete answer.
RUASK_TRACE=1 writes a timing-only stage record to stderr; off by default.";

/// 带退出码的失败：按发生阶段分类，供脚本区分原因。
struct Failure {
    code: u8,
    error: Box<dyn Error>,
}

/// 退出码：2 用法或输入，3 配置，4 无法连接或服务返回错误，5 回答不完整，1 其他。
const USAGE: u8 = 2;
const CONFIG: u8 = 3;

fn fail(code: u8) -> impl FnOnce(Box<dyn Error>) -> Failure {
    move |error| Failure { code, error }
}

/// 模型阶段的错误按信息分类：请求失败（连接、HTTP 状态）为 4，回答不完整为 5。
fn model_failure(error: Box<dyn Error>) -> Failure {
    let text = error.to_string();
    let code = if text.starts_with("模型请求失败") {
        4
    } else if text.starts_with("模型响应") || text.starts_with("无法读取模型响应") {
        5
    } else {
        1
    };
    Failure { code, error }
}

impl From<Box<dyn Error>> for Failure {
    fn from(error: Box<dyn Error>) -> Self {
        Self { code: 1, error }
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self {
            code: 1,
            error: error.into(),
        }
    }
}

fn main() -> ExitCode {
    let trace = trace::Trace::new();
    trace.mark("process_entry");
    match run(&trace) {
        Ok(()) => {
            trace.finish(true);
            ExitCode::SUCCESS
        }
        Err(failure)
            if failure
                .error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe) =>
        {
            trace.finish(true);
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("ruask: {}", failure.error);
            trace.finish(false);
            ExitCode::from(failure.code)
        }
    }
}

/// 结果语言：-L → 文件 language → locale → 英文。
fn resolve_language(
    argument: Option<Language>,
    file: Option<&str>,
) -> Result<Language, Box<dyn Error>> {
    Ok(match (argument, file) {
        (Some(language), _) => language,
        (None, Some(language)) => Language::parse(language)?,
        (None, None) => Language::from_locale(),
    })
}

/// 命令词在本地页面中的解析结果。
struct Resolved {
    /// 页面名，例如 `git-commit`。
    name: String,
    /// 所有命令词都用于定位页面（没有剩余的选项或文字）。
    complete: bool,
    /// 首词拼写被纠正时，纠正后的命令。
    corrected: Option<String>,
}

/// 路由的第一步：用命令词定位 tldr 页面，必要时纠正首词拼写。
fn resolve_command(pages: &tldr::Pages, words: &[String]) -> Option<Resolved> {
    let view: Vec<&str> = words.iter().map(String::as_str).collect();
    if view.is_empty() {
        return None;
    }
    if let Some((name, used)) = pages.resolve(&view) {
        return Some(Resolved {
            name,
            complete: used == view.len(),
            corrected: None,
        });
    }
    let fixed = pages.suggest(view[0])?;
    let mut retry: Vec<&str> = vec![fixed.as_str()];
    retry.extend(&view[1..]);
    let (name, used) = pages.resolve(&retry)?;
    Some(Resolved {
        name,
        complete: used == retry.len(),
        corrected: Some(retry[..used].join(" ")),
    })
}

/// stdout 所在终端的列数；不是终端或无法读取时为 0。
fn terminal_width() -> usize {
    use std::os::fd::AsRawFd;
    let mut size = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // TIOCGWINSZ 只读取 stdout 对应终端的窗口大小，写入本地结构体。
    let ok = unsafe { libc::ioctl(io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut size) } == 0;
    if ok { usize::from(size.ws_col) } else { 0 }
}

fn run(trace: &trace::Trace) -> Result<(), Failure> {
    let args = Arguments::parse().map_err(fail(USAGE))?;
    if args.help || args.short_help {
        let language = resolve_language(args.language, config::file_language().as_deref())
            .unwrap_or(Language::Zh);
        // 未显式选择英文时保持中文帮助（与错误信息一致）。
        let english = args.language == Some(Language::En)
            || (args.language.is_none() && config::file_language().as_deref() == Some("en"));
        let text = match (args.help, english && language == Language::En) {
            (true, true) => HELP_EN,
            (true, false) => HELP,
            (false, true) => SHORT_HELP_EN,
            (false, false) => SHORT_HELP,
        };
        return Ok(write_output(text)?);
    }
    if args.version {
        return Ok(write_output(&format!(
            "ruask {}\n{}",
            env!("CARGO_PKG_VERSION"),
            tldr::status()
        ))?);
    }
    if args.init_config {
        return Ok(write_output(&config::initialize().map_err(fail(CONFIG))?)?);
    }
    if args.update_pages {
        return Ok(update_pages(args.from.as_deref())?);
    }

    let Some(mut query) = read_query(&args).map_err(fail(USAGE))? else {
        return Ok(write_output(SHORT_HELP)?);
    };
    trace.mark("query_ready");
    let platform = match args.platform {
        Some(platform) => platform,
        None => Platform::host().map_err(|error| fail(CONFIG)(error.into()))?,
    };
    let terminal = io::stdout().is_terminal();
    let color = args.color.enabled(terminal);
    let lines = |language: Language| {
        let lines = output::Lines::new(io::stdout().lock(), color, trace).language(language);
        if terminal {
            lines.terminal(terminal_width())
        } else {
            lines
        }
    };
    let pages = tldr::Pages::locate();
    let words: Vec<String> = query
        .command_query
        .as_deref()
        .map(|c| c.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    // 路由：命令词完整命中 tldr 页面（必要时纠正首词拼写）、且没有附带问题和 stdin 时本地直出，
    // 不需要模型凭据；其余查询交给模型，并把命中的页面作为参考。
    let resolved = pages
        .as_ref()
        .and_then(|pages| resolve_command(pages, &words));
    if let (Some(pages), Some(resolved)) = (&pages, &resolved)
        && resolved.complete
        && query.question.is_none()
        && query.output_context.is_none()
    {
        let language = resolve_language(args.language, config::file_language().as_deref())
            .map_err(fail(CONFIG))?;
        if let Some(page) = pages.page(&resolved.name, language, platform) {
            trace.mark("tldr_hit");
            if let Some(corrected) = &resolved.corrected {
                eprintln!("ruask: 未找到 {}，按 {corrected} 显示", words[0]);
            }
            let mut output = lines(language).mnemonics();
            output.push(&tldr::for_display(&page, platform, language))?;
            output.finish()?;
            output.footer(tldr::ATTRIBUTION)?;
            return Ok(());
        }
    }

    let config = config::load()
        .map_err(|error| -> Box<dyn Error> {
            // 首次使用：只有命令名的查询本可以本地直出，提示安装页面而不是只要求模型配置。
            if pages.is_none() && query.question.is_none() && query.output_context.is_none() {
                format!(
                    "{error}\n提示：运行 ruask --update-pages 安装本地 tldr 页面后，只有命令名的查询（如 ruask tar）无需模型配置"
                )
                .into()
            } else {
                error
            }
        })
        .map_err(fail(CONFIG))?;
    trace.mark("config_ready");
    let language =
        resolve_language(args.language, config.language.as_deref()).map_err(fail(CONFIG))?;
    // 显式命令命中 tldr 时，把命令页及同族相关页作为参考交给模型。
    if let (Some(pages), Some(resolved)) = (&pages, &resolved) {
        query.reference = pages.reference(
            &resolved.name,
            query.question.as_deref(),
            language,
            platform,
        );
        if query.reference.is_some() {
            trace.mark("tldr_reference");
        }
    }
    // 目标平台就是本机时，附上 shell、包管理器与所查命令是否已安装。
    if Platform::host().is_ok_and(|host| host == platform) {
        let first = resolved
            .as_ref()
            .map(|r| r.name.split('-').next().unwrap_or(&r.name).to_string())
            .or_else(|| words.first().cloned());
        query.environment = Some(environment::detect(first.as_deref()));
    }
    let label = match language {
        Language::Zh => format!("正在询问 {}…", config.model),
        Language::En => format!("Asking {}…", config.model),
    };
    let started = std::time::Instant::now();
    let mut output = lines(language).waiting(waiting::Waiting::start(label));
    if let Err(error) = model::ask(&config, &query, language, platform, &mut output, trace) {
        output.interrupt();
        return Err(model_failure(error));
    }
    drop(output);
    // 来源标识只写到终端中的 stderr：不进入 stdout，重定向和管道结果保持不变。
    if io::stderr().is_terminal() {
        let reference = resolved
            .as_ref()
            .filter(|_| query.reference.is_some())
            .map(|r| r.name.as_str());
        let seconds = started.elapsed().as_secs_f64();
        let text = match (language, reference) {
            (Language::Zh, Some(page)) => format!(
                "AI 生成，请核对 · {} · {seconds:.1} s · 参考 tldr：{page}",
                config.model
            ),
            (Language::Zh, None) => format!("AI 生成，请核对 · {} · {seconds:.1} s", config.model),
            (Language::En, Some(page)) => format!(
                "AI-generated, please verify · {} · {seconds:.1} s · tldr reference: {page}",
                config.model
            ),
            (Language::En, None) => {
                format!(
                    "AI-generated, please verify · {} · {seconds:.1} s",
                    config.model
                )
            }
        };
        if std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()) {
            eprintln!("\n  \x1b[2m{text}\x1b[0m");
        } else {
            eprintln!("\n  {text}");
        }
    }
    Ok(())
}

fn update_pages(from: Option<&std::path::Path>) -> Result<(), Box<dyn Error>> {
    write_output(&tldr::update(from)?)
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
    short_help: bool,
    version: bool,
    init_config: bool,
    update_pages: bool,
    from: Option<std::path::PathBuf>,
    color: output::Color,
}

impl Arguments {
    fn parse() -> Result<Self, Box<dyn Error>> {
        let mut parsed = Self::default();
        let mut args = expand_attached(std::env::args().skip(1)).into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--" => {
                    parsed.command.extend(args);
                    break;
                }
                "-h" => parsed.short_help = true,
                "--help" => parsed.help = true,
                "-V" | "--version" => parsed.version = true,
                "--init-config" => parsed.init_config = true,
                "--update-pages" => parsed.update_pages = true,
                "--from" => parsed.from = Some(option_value(&arg, &mut args)?.into()),
                "-L" | "--language" => {
                    let value = option_value(&arg, &mut args)?;
                    parsed.language = Some(Language::parse(&value)?);
                }
                "-p" | "--platform" => {
                    let value = option_value(&arg, &mut args)?;
                    parsed.platform = Some(Platform::parse(&value)?);
                }
                "--color" => {
                    let value = option_value(&arg, &mut args)?;
                    parsed.color = output::Color::parse(&value)?;
                }
                "-q" | "--question" => {
                    let value = option_value(&arg, &mut args)?;
                    if parsed.question.replace(value).is_some() {
                        return Err("-q / --question 只能提供一次".into());
                    }
                }
                // 命令词之后 ruask 不认识的选项属于目标命令，例如 `ruask tar -xzvf`。
                _ if arg.starts_with('-') && !parsed.command.is_empty() => {
                    parsed.command.push(arg);
                }
                _ if arg.starts_with('-') => {
                    return Err(format!(
                        "未知选项 {arg}；查询目标命令的选项时，请写在命令之后、整体加引号或使用 -- 分隔"
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
        if parsed.update_pages
            && (parsed.init_config
                || !parsed.command.is_empty()
                || parsed.question.is_some()
                || parsed.language.is_some()
                || parsed.platform.is_some())
        {
            return Err("--update-pages 只能与 --from 一起使用".into());
        }
        if parsed.from.is_some() && !parsed.update_pages {
            return Err("--from 只能与 --update-pages 一起使用".into());
        }
        Ok(parsed)
    }
}

/// `--language=zh`、`-Lzh` 拆成两个参数；只处理带值选项，`--` 之后原样保留。
fn expand_attached(args: impl Iterator<Item = String>) -> Vec<String> {
    const LONG: [&str; 5] = [
        "--question",
        "--language",
        "--platform",
        "--color",
        "--from",
    ];
    const SHORT: [&str; 3] = ["-q", "-L", "-p"];
    let mut out = Vec::new();
    let mut literal = false;
    for arg in args {
        if literal || arg == "--" {
            literal = true;
            out.push(arg);
            continue;
        }
        if let Some((option, value)) = arg.split_once('=')
            && LONG.contains(&option)
        {
            out.push(option.to_string());
            out.push(value.to_string());
        } else if let Some(short) = SHORT
            .iter()
            .find(|short| arg.len() > 2 && arg.starts_with(**short))
        {
            out.push(short.to_string());
            out.push(arg[2..].to_string());
        } else {
            out.push(arg);
        }
    }
    out
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
                    | "-V"
                    | "--version"
                    | "--update-pages"
                    | "--from"
                    | "--color"
            )
        })
        .ok_or_else(|| format!("选项 {option} 需要一个字符串值").into())
}

/// 读取 stdin 直到结束；超过 1 s 仍未结束时在 stderr 提示一次，避免一个不关闭的管道看起来像卡死。
fn read_stdin(stdin: io::Stdin, limit: u64) -> io::Result<Vec<u8>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdin.take(limit).read_to_end(&mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(std::time::Duration::from_secs(1)) {
        Ok(result) => result,
        Err(_) => {
            eprintln!(
                "ruask: 正在等待 stdin 结束（上游命令仍在运行，或按 Ctrl-D 结束；不需要输入时可加 </dev/null）"
            );
            receiver
                .recv()
                .map_err(|_| io::Error::other("读取 stdin 失败"))?
        }
    }
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
        bytes = read_stdin(stdin, MAX_INPUT_BYTES + 1)?;
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
        reference: None,
        environment: None,
    }))
}
