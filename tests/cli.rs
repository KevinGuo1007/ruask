use std::io::Write;
use std::process::{Command, Output, Stdio};

use mockito::{Matcher, Server};
use serde_json::{Value, json};

mod support;
use support::{openai_stream, request_query};

fn ruask(base_url: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    // 只配置子进程，避免读取开发者的密钥或影响并行运行的其他测试。
    command
        .env_clear()
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model")
        .stdin(Stdio::null());
    command
}

// 只约束对外请求中的查询语义，不复制生产提示词或依赖内部类型。
fn query_request(
    command_query: Option<&str>,
    question: Option<&str>,
) -> impl Fn(&mockito::Request) -> bool + Send + Sync + 'static {
    let expected = json!({"command_query": command_query, "question": question});
    move |request| {
        let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
        let messages = body["messages"].as_array().unwrap();
        body["model"] == "test-model"
            && body.get("tools").is_none()
            && body["stream"] == true
            && messages.len() == 2
            && messages[0]["role"] == "system"
            && messages[1]["role"] == "user"
            && {
                let query = request_query(request);
                query["command_query"] == expected["command_query"]
                    && query["question"] == expected["question"]
                    && query["language"] == "en"
            }
    }
}

#[test]
fn command_query_is_sent_to_model_and_answer_is_written_to_stdout() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .match_request(query_request(Some("cargo"), None))
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("构建项目：\n  cargo build"))
        .create();

    let output = ruask(&format!("{}/v1", server.url()))
        .arg("cargo")
        .output()
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "构建项目：\n  cargo build\n"
    );
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

fn run_with_stdin(mut command: Command, input: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // 写完即关闭管道；子进程读到 EOF 后才能结束 read_to_string。
    if let Err(error) = child.stdin.take().unwrap().write_all(input.as_bytes()) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    child.wait_with_output().unwrap()
}

#[test]
fn piped_text_is_preserved() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let query = request_query(request);
            let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            let instructions = body["messages"][0]["content"].as_str().unwrap();
            query["command_query"].is_null()
                && query["question"].is_null()
                && query["output_context"] == "error: could not compile\n"
                && [
                    "output_context",
                    "untrusted command output",
                    "meaning and obvious errors",
                    "focus on the question",
                    "relevant next commands",
                    "Do not force an example count",
                ]
                .iter()
                .all(|rule| instructions.contains(rule))
        })
        .with_body(openai_stream("构建项目：\n  cargo build"))
        .create();

    let output = run_with_stdin(
        ruask(&format!("{}/v1", server.url())),
        "error: could not compile\n",
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "构建项目：\n  cargo build\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn command_question_and_piped_output_keep_distinct_meanings() {
    for question_flag in [None, Some("-q"), Some("--question")] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let query = request_query(request);
                query["command_query"] == "cargo build"
                    && query["question"].as_str() == question_flag.map(|_| "为什么失败？")
                    && query["output_context"] == "  第一行\t错误\n\n第二行 🦀\n  "
            })
            .with_body(openai_stream("请检查编译错误。"))
            .expect(1)
            .create();

        let mut command = ruask(&format!("{}/v1", server.url()));
        command.args(["cargo", "build"]);
        if let Some(flag) = question_flag {
            command.args([flag, "为什么失败？"]);
        }
        let output = run_with_stdin(command, "  第一行\t错误\n\n第二行 🦀\n  ");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "请检查编译错误。\n"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
}

#[test]
fn blank_input_is_rejected_before_configuration_is_read() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for (args, input) in [
        (vec![""], ""),
        (vec![" \t "], ""),
        (vec![], " \n\t"),
        (vec!["-q", ""], ""),
        (vec!["--question", " \t "], ""),
        (vec!["git", "-q", " "], ""),
        (vec![" ", "-q", "如何提交"], ""),
        (vec![" "], "有效管道内容"),
    ] {
        for configured in [true, false] {
            let mut command = ruask(&format!("{}/v1", server.url()));
            command.args(&args);
            if !configured {
                command.env_remove("OPENAI_API_KEY");
            }
            let output = run_with_stdin(command, input);

            assert!(!output.status.success(), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("不能为空白"),
                "{output:?}"
            );
        }
    }
    requests.assert();
}

#[test]
fn missing_or_blank_configuration_names_the_variable() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for name in ["OPENAI_BASE_URL", "OPENAI_API_KEY", "OPENAI_MODEL"] {
        for value in [None, Some(""), Some(" \t ")] {
            let mut command = ruask(&format!("{}/v1", server.url()));
            command.arg("git");
            match value {
                Some(value) => {
                    command.env(name, value);
                }
                None => {
                    command.env_remove(name);
                }
            }
            let output = command.output().unwrap();

            assert!(!output.status.success(), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(name),
                "{name}: {output:?}"
            );
            assert!(!String::from_utf8_lossy(&output.stderr).contains("test-key"));
        }
    }
    requests.assert();
}

#[test]
fn model_http_failure_is_reported_without_polluting_stdout() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_status(401)
        .with_body(r#"{"error":{"message":"invalid key"}}"#)
        .create();

    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();

    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("模型请求失败") && error.contains("401"),
        "{error}"
    );
    assert!(!error.contains("test-key"), "{error}");
    response.assert();
}

#[test]
fn unusable_model_responses_are_reported_without_output() {
    for body in [
        "not json",
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":null}}]}"#,
        r#"{"choices":[{"message":{"content":""}}]}"#,
        r#"{"choices":[{"message":{"content":" \n\t"}}]}"#,
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .with_body(body)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();

        assert!(!output.status.success(), "{body}: {output:?}");
        assert!(output.stdout.is_empty(), "{body}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("模型响应"),
            "{body}: {output:?}"
        );
        response.assert();
    }
}

#[test]
fn base_url_preserves_custom_path_and_accepts_trailing_slash() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/compatible/v1/chat/completions")
        .with_body(openai_stream("查看状态：git status"))
        .create();
    let output = ruask(&format!("{}/compatible/v1/", server.url()))
        .arg("git")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "查看状态：git status\n"
    );
    response.assert();
}

#[test]
fn all_command_arguments_are_preserved() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(query_request(Some("git remote add"), None))
        .with_body(openai_stream("Add a remote: git remote add origin URL"))
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .args(["git", "remote", "add"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Add a remote: git remote add origin URL\n"
    );
    response.assert();
}

#[test]
fn help_succeeds_without_input_or_model_configuration() {
    for flag in ["--help", "-h"] {
        let output = ruask("unused")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("OPENAI_API_KEY")
            .env_remove("OPENAI_MODEL")
            .arg(flag)
            .output()
            .unwrap();

        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let help = String::from_utf8_lossy(&output.stdout);
        for text in [
            "用法",
            "stdin",
            "OPENAI_BASE_URL",
            "OPENAI_API_KEY",
            "OPENAI_MODEL",
        ] {
            assert!(help.contains(text), "{help}");
        }
    }
}

#[cfg(unix)]
#[test]
fn closed_downstream_output_ends_without_error() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream("查看状态：git status"))
        .create();

    // 在启动 CLI 前关闭读取端，稳定地模拟下游不再接收输出。
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let output_fd: OwnedFd = writer.into();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .stdout(Stdio::from(output_fd))
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn query_and_question_semantics_are_preserved() {
    for (args, query, question) in [
        (vec!["git", "commit"], Some("git commit"), None),
        (
            vec!["如何查看占用 8080 端口的进程"],
            Some("如何查看占用 8080 端口的进程"),
            None,
        ),
        (
            vec!["  查找\t文件\n保留原文  "],
            Some("  查找\t文件\n保留原文  "),
            None,
        ),
        (vec!["git commit --amend"], Some("git commit --amend"), None),
        (
            vec!["--", "git", "commit", "--amend", "-q", "--help"],
            Some("git commit --amend -q --help"),
            None,
        ),
        (
            vec!["git", "commit", "-q", "如何修改上一次提交的信息"],
            Some("git commit"),
            Some("如何修改上一次提交的信息"),
        ),
        (
            vec![
                "git",
                "--question",
                "  how to amend\n提交信息？  ",
                "commit",
            ],
            Some("git commit"),
            Some("  how to amend\n提交信息？  "),
        ),
        (
            vec!["-q", "如何查看端口占用"],
            None,
            Some("如何查看端口占用"),
        ),
        (
            vec!["--question", "how to find large files"],
            None,
            Some("how to find large files"),
        ),
        (
            vec!["-q", "如何修改提交", "--", "git", "commit", "--amend"],
            Some("git commit --amend"),
            Some("如何修改提交"),
        ),
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(query_request(query, question))
            .with_body(openai_stream("用途说明\n  example command"))
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .args(&args)
            .output()
            .unwrap();

        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "用途说明\n  example command\n"
        );
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        response.assert();
    }
}

#[test]
fn instructions_define_the_command_line_reference_contract() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            let system = body["messages"][0]["content"].as_str().unwrap();
            // 检查内容约束，不固定整段提示词、标点或排版。
            [
                "command-line usage",
                "general chat",
                "decline unrelated",
                "Do not retrieve documentation",
                "execute suggested commands",
                "one-sentence introduction",
                "about five",
                "one to three",
                "purpose description with a command",
                "plain text",
                "without Markdown code fences",
                "ANSI escape",
                "single query",
                "language=en",
                "language=zh",
                "Simplified Chinese",
                "Traditional Chinese",
                "platform=linux",
                "platform=osx",
                "natural-language task",
                "prefer commands available on the target platform",
                "explicitly queried command",
                "briefly note its applicable platform",
                "continue explaining the original command",
                "do not replace it with another tool",
            ]
            .iter()
            .all(|rule| system.contains(rule))
                && query_request(Some("git"), None)(request)
        })
        .with_body(openai_stream("Git manages version history."))
        .expect(1)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    response.assert();
}

#[test]
fn invalid_options_fail_locally_with_clear_errors() {
    for (args, expected) in [
        (vec!["--qusetion", "how to commit"], "未知选项 --qusetion"),
        (vec!["--amend", "git", "commit"], "未知选项 --amend"),
        (vec!["-x"], "未知选项 -x"),
        (vec!["-q"], "选项 -q 需要一个字符串值"),
        (
            vec!["git", "--question"],
            "选项 --question 需要一个字符串值",
        ),
        (vec!["-q", "--help"], "选项 -q 需要一个字符串值"),
        (vec!["--question", "--"], "选项 --question 需要一个字符串值"),
        (vec!["-q", "one", "--question", "two"], "只能提供一次"),
    ] {
        let mut server = Server::new();
        let requests = server.mock("POST", Matcher::Any).expect(0).create();
        for configured in [true, false] {
            let mut command = ruask(&format!("{}/v1", server.url()));
            command.args(&args);
            if !configured {
                command.env_remove("OPENAI_API_KEY");
            }
            let output = command.output().unwrap();
            assert!(!output.status.success(), "{args:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains(expected), "{args:?}: {error}");
            assert!(!error.contains("test-key"), "{error}");
        }
        requests.assert();
    }
}

#[test]
fn bare_invocation_and_help_do_not_request_the_model() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for args in [vec![], vec!["-h"], vec!["--help"]] {
        for configured in [true, false] {
            let mut command = ruask(&format!("{}/v1", server.url()));
            command.args(&args);
            if !configured {
                command
                    .env_remove("OPENAI_BASE_URL")
                    .env_remove("OPENAI_API_KEY")
                    .env_remove("OPENAI_MODEL");
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            let help = String::from_utf8_lossy(&output.stdout);
            for text in ["用法", "命令", "-q", "--question", "--", "OPENAI_API_KEY"] {
                assert!(help.contains(text), "{help}");
            }
        }
    }
    requests.assert();
}

#[test]
fn repeated_queries_each_make_one_fresh_request() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(query_request(Some("git"), None))
        .with_body(openai_stream("git status"))
        .expect(2)
        .create();
    for _ in 0..2 {
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    response.assert();
}

#[test]
fn explicit_language_is_sent_to_the_model_and_fixed_answers_are_displayed() {
    for (flag, language, answer) in [
        ("-L", "zh", "查看状态：\n  git status"),
        ("--language", "en", "Show status:\n  git status"),
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let query = request_query(request);
                query["language"] == language
                    && query["command_query"] == "git status"
                    && query["question"] == "show changes"
            })
            .with_body(openai_stream(answer))
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .args(["git", flag, language, "status", "-q", "show changes"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("{answer}\n")
        );
        response.assert();
    }
}

#[test]
fn automatic_language_follows_locale_candidates_and_falls_back_to_english() {
    for (lang, language, expected) in [
        (None, Some("zh:en"), "en"),
        (Some("en_US.UTF-8"), Some("fr:zh_TW.UTF-8:en"), "zh"),
        (Some("zh_CN.UTF-8"), Some("fr:de_DE"), "zh"),
        (Some("zh_TW.UTF-8"), None, "zh"),
        (Some("zh-Hant-TW.UTF-8"), None, "zh"),
        (Some("en_GB.UTF-8"), Some("fr:en_US:zh"), "en"),
        (Some("zh_HK.UTF-8"), Some("C:POSIX:de:en"), "en"),
        (Some("C"), Some("zh"), "zh"),
        (Some("C.UTF-8"), None, "en"),
        (Some("POSIX"), Some("C:POSIX:fr"), "en"),
        (Some("de_DE.UTF-8"), Some("fr:ja_JP"), "en"),
        (Some(""), Some(":C::zh_CN.UTF-8:"), "zh"),
        (Some("ZH_tw.UTF-8@variant"), None, "zh"),
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let query = request_query(request);
                query["language"] == expected
            })
            .with_body(openai_stream("git status"))
            .expect(1)
            .create();
        let mut command = ruask(&format!("{}/v1", server.url()));
        command
            .arg("git")
            .env("LC_ALL", "zh_CN.UTF-8")
            .env("LC_MESSAGES", "zh_CN.UTF-8");
        if let Some(value) = lang {
            command.env("LANG", value);
        }
        if let Some(value) = language {
            command.env("LANGUAGE", value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "LANG={lang:?} LANGUAGE={language:?}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
}

#[test]
fn platform_defaults_to_host_and_accepts_overrides_and_macos_alias() {
    let host = match std::env::consts::OS {
        "macos" => "osx",
        "linux" => "linux",
        other => panic!("unsupported test host: {other}"),
    };
    for (args, expected) in [
        (vec![], host),
        (vec!["-p", "linux"], "linux"),
        (vec!["--platform", "osx"], "osx"),
        (vec!["-p", "macos"], "osx"),
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let query = request_query(request);
                query["platform"] == expected
                    && query["language"] == "zh"
                    && query["command_query"] == "find a process using port 8080"
            })
            .with_body(openai_stream("固定回答"))
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .args(["-L", "zh"])
            .args(&args)
            .arg("find a process using port 8080")
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "固定回答\n");
        response.assert();
    }
}

#[test]
fn language_and_platform_options_reject_invalid_or_missing_values_locally() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for (args, expected) in [
        (vec!["git", "-L", "fr"], "结果语言仅支持 en 或 zh"),
        (
            vec!["git", "--language", "zh_TW"],
            "结果语言仅支持 en 或 zh",
        ),
        (vec!["git", "-L", ""], "结果语言仅支持 en 或 zh"),
        (vec!["git", "--language", " "], "结果语言仅支持 en 或 zh"),
        (vec!["git", "-p", "windows"], "目标平台仅支持"),
        (vec!["git", "--platform", ""], "目标平台仅支持"),
        (vec!["git", "-L"], "选项 -L 需要一个字符串值"),
        (
            vec!["git", "--language"],
            "选项 --language 需要一个字符串值",
        ),
        (vec!["git", "-p"], "选项 -p 需要一个字符串值"),
        (
            vec!["git", "--platform"],
            "选项 --platform 需要一个字符串值",
        ),
        (vec!["git", "-L", "--help"], "选项 -L 需要一个字符串值"),
        (vec!["git", "-p", "--"], "选项 -p 需要一个字符串值"),
        (
            vec!["git", "-q", "--language", "zh"],
            "选项 -q 需要一个字符串值",
        ),
        (
            vec!["git", "--question", "-p", "linux"],
            "选项 --question 需要一个字符串值",
        ),
        (vec!["--init-config", "-L", "zh"], "需单独使用"),
        (vec!["--init-config", "-p", "linux"], "需单独使用"),
    ] {
        for configured in [true, false] {
            let mut command = ruask(&format!("{}/v1", server.url()));
            command.args(&args);
            if !configured {
                command.env_remove("OPENAI_API_KEY");
            }
            let output = command.output().unwrap();
            assert!(!output.status.success(), "{args:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains(expected), "{args:?}: {error}");
            assert!(!error.contains("test-key"), "{error}");
        }
    }
    requests.assert();
}

#[test]
fn connection_failure_is_reported_without_leaking_credentials() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let output = ruask(&format!("http://{address}/v1"))
        .arg("git")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("模型请求失败"), "{error}");
    assert!(!error.contains("test-key"), "{error}");
}

#[test]
fn questions_starting_with_target_options_are_preserved() {
    for flag in ["-q", "--question"] {
        for question in ["--amend 如何使用", "--amend"] {
            let mut server = Server::new();
            let response = server
                .mock("POST", "/v1/chat/completions")
                .match_request(query_request(Some("git commit"), Some(question)))
                .with_body(openai_stream("修改最后一次提交：git commit --amend"))
                .expect(1)
                .create();
            let output = ruask(&format!("{}/v1", server.url()))
                .args(["git", "commit", flag, question])
                .output()
                .unwrap();
            assert!(output.status.success(), "{flag} {question}: {output:?}");
            response.assert();
        }
    }
}

#[test]
fn oversized_pipe_is_rejected_before_any_request_or_configuration_lookup() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    // The second input has fewer characters than the limit but more UTF-8 bytes.
    for input in [
        "x".repeat(65_537),
        format!("{}xx", "错".repeat(21_845)),
        "错".repeat(21_846),
    ] {
        for configured in [true, false] {
            let mut command = ruask(&format!("{}/v1", server.url()));
            command.args(["cargo", "build", "-q", "为什么失败"]);
            if !configured {
                command.env_remove("OPENAI_API_KEY");
            }
            let output = run_with_stdin(command, &input);
            assert!(!output.status.success(), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains("65,536"), "{error}");
            assert!(error.contains("tail") && error.contains("筛选"), "{error}");
            assert!(!error.contains("test-key"), "{error}");
        }
    }
    requests.assert();
}

#[test]
fn pipe_at_the_exact_byte_limit_is_sent_without_truncation() {
    for input in ["x".repeat(65_536), format!("{}x", "错".repeat(21_845))] {
        assert_eq!(input.len(), 65_536);
        let mut server = Server::new();
        let expected = input.clone();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let query = request_query(request);
                query["command_query"].is_null()
                    && query["output_context"] == expected
                    && query["question"].is_null()
            })
            .with_body(openai_stream("Output explained."))
            .expect(1)
            .create();
        let output = run_with_stdin(ruask(&format!("{}/v1", server.url())), &input);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Output explained.\n");
        assert!(output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
}

#[test]
fn finite_pipe_waits_for_eof_then_sends_one_output_interpretation_request() {
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

    let mut server = Server::new();
    let (sent, received) = mpsc::channel();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(move |request| {
            sent.send(request_query(request)).unwrap();
            true
        })
        .with_body(openai_stream("Use cargo check to inspect errors."))
        .expect(1)
        .create();
    let mut child = ruask(&format!("{}/v1", server.url()))
        .args(["-q", "下一步如何排查"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all("  error: 缺少依赖\n".as_bytes()).unwrap();
    // The observation window checks that a still-open pipe produces no request;
    // the timeout is not a response-speed expectation.
    assert_eq!(
        received.recv_timeout(Duration::from_millis(250)),
        Err(RecvTimeoutError::Timeout)
    );
    stdin.write_all(b"  at Cargo.toml\n").unwrap();
    drop(stdin);
    let query = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(query["command_query"].is_null());
    assert_eq!(query["question"], "下一步如何排查");
    assert_eq!(
        query["output_context"],
        "  error: 缺少依赖\n  at Cargo.toml\n"
    );
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Use cargo check to inspect errors.\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn an_empty_closed_pipe_shows_help_without_configuration_or_requests() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    let mut command = ruask(&format!("{}/v1", server.url()));
    command.env_remove("OPENAI_API_KEY");
    let output = run_with_stdin(command, "");
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("用法"));
    assert!(output.stderr.is_empty(), "{output:?}");
    requests.assert();
}
