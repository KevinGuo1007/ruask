mod model;

use std::error::Error;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

const HELP: &str = "用法：ruask [问题...]
      cat file | ruask [指令...]

从参数或 stdin 接收文本，输出模型回答。参数与 stdin 同时存在时以空行分隔。

必需的环境变量：
  OPENAI_BASE_URL  接口基础地址（包含服务所需路径，例如 /v1）
  OPENAI_API_KEY   接口密钥
  OPENAI_MODEL     模型名称

选项：-h, --help    显示帮助（单独使用）";

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "-h" | "--help") {
        return write_output(HELP);
    }

    let prompt = read_prompt(&args)?;
    let config = model::Config::from_env()?;
    let answer = model::ask(&config, &prompt)?;
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

fn read_prompt(args: &[String]) -> Result<String, Box<dyn Error>> {
    let instruction = args.join(" ");
    let mut input = String::new();
    let mut stdin = io::stdin();
    if !stdin.is_terminal() {
        stdin.read_to_string(&mut input)?;
    }

    let prompt = if instruction.trim().is_empty() {
        input
    } else if input.trim().is_empty() {
        instruction
    } else {
        format!("{instruction}\n\n{input}")
    };
    if prompt.trim().is_empty() {
        return Err("请通过参数或 stdin 提供问题".into());
    }
    Ok(prompt)
}
