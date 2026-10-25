use std::io::Write;
use std::process::{Command, Output, Stdio};

use mockito::{Matcher, Server};
use serde_json::json;

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

#[test]
fn question_is_sent_to_model_and_answer_is_written_to_stdout() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .match_body(Matcher::Json(json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "什么是所有权？"}]
        })))
        .with_header("content-type", "application/json")
        .with_body(r#"{"choices":[{"message":{"content":"每个值都有一个所有者。"}}]}"#)
        .create();

    let output = ruask(&format!("{}/v1", server.url()))
        .arg("什么是所有权？")
        .output()
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "每个值都有一个所有者。\n"
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn piped_text_is_used_as_question() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::Json(json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "解释所有权。\n"}]
        })))
        .with_body(r#"{"choices":[{"message":{"content":"每个值都有一个所有者。"}}]}"#)
        .create();

    let output = run_with_stdin(ruask(&format!("{}/v1", server.url())), "解释所有权。\n");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "每个值都有一个所有者。\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn instruction_and_piped_text_are_both_sent_to_model() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::Json(json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "总结\n\n第一行\n第二行\n"}]
        })))
        .with_body(r#"{"choices":[{"message":{"content":"这是两行文本。"}}]}"#)
        .create();

    let mut command = ruask(&format!("{}/v1", server.url()));
    command.arg("总结");
    let output = run_with_stdin(command, "第一行\n第二行\n");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "这是两行文本。\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn blank_input_is_rejected_before_configuration_is_read() {
    for (args, input) in [(vec![], ""), (vec![" \t "], ""), (vec![], " \n\t")] {
        let mut command = ruask("http://127.0.0.1:9/v1");
        command.args(args).env_remove("OPENAI_API_KEY");
        let output = run_with_stdin(command, input);

        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("请通过参数或 stdin 提供问题"),
            "{output:?}"
        );
    }
}

#[test]
fn missing_or_blank_configuration_names_the_variable() {
    for name in ["OPENAI_BASE_URL", "OPENAI_API_KEY", "OPENAI_MODEL"] {
        for value in [None, Some(""), Some(" \t ")] {
            let mut command = ruask("http://127.0.0.1:9/v1");
            command.arg("你好");
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
        }
    }
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
        .arg("你好")
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
            .arg("你好")
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
        .with_body(r#"{"choices":[{"message":{"content":"你好。"}}]}"#)
        .create();
    let output = ruask(&format!("{}/compatible/v1/", server.url()))
        .arg("你好")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "你好。\n");
    response.assert();
}

#[test]
fn all_question_arguments_are_preserved() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::Json(json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "explain Rust ownership"}]
        })))
        .with_body(r#"{"choices":[{"message":{"content":"Each value has an owner."}}]}"#)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .args(["explain", "Rust", "ownership"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Each value has an owner.\n"
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
        .with_body(r#"{"choices":[{"message":{"content":"你好。"}}]}"#)
        .create();

    // 在启动 CLI 前关闭读取端，稳定地模拟下游不再接收输出。
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let output_fd: OwnedFd = writer.into();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("你好")
        .stdout(Stdio::from(output_fd))
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}
