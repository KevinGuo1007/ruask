use std::process::{Command, Stdio};

use mockito::{Matcher, Server};
use serde_json::json;

fn command(server: &Server) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command
        .env_clear()
        .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model")
        .stdin(Stdio::null())
        .arg("git");
    command
}

fn answer() -> String {
    json!({"choices":[{"message":{"content":"- Check:\n`git status`"},"finish_reason":"stop"}]})
        .to_string()
}

#[test]
fn thinking_is_omitted_by_default_and_only_sent_when_explicitly_configured() {
    for value in [None, Some("enabled"), Some("disabled")] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let body: serde_json::Value =
                    serde_json::from_slice(request.body().unwrap()).unwrap();
                match value {
                    None => body.get("thinking").is_none(),
                    Some(value) => body["thinking"] == json!({"type":value}),
                }
            })
            .with_header("content-type", "application/json")
            .with_body(answer())
            .expect(1)
            .create();
        let mut cmd = command(&server);
        if let Some(value) = value {
            cmd.env("OPENAI_THINKING", value);
        }
        let output = cmd.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"  Check:\n    git status\n");
        response.assert();
    }
}

#[test]
fn saved_thinking_and_environment_override_follow_selected_provider() {
    let home = tempfile::tempdir().unwrap();
    let mut server = Server::new();
    std::fs::create_dir(home.path().join("ruask")).unwrap();
    std::fs::write(
        home.path().join("ruask/config.toml"),
        "[openai]\nthinking = \"enabled\"\n",
    )
    .unwrap();
    for value in [None, Some("disabled")] {
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_body(Matcher::PartialJson(
                json!({"thinking":{"type":value.unwrap_or("enabled")}}),
            ))
            .with_header("content-type", "application/json")
            .with_body(answer())
            .expect(1)
            .create();
        let mut cmd = command(&server);
        cmd.env("XDG_CONFIG_HOME", home.path());
        if let Some(value) = value {
            cmd.env("OPENAI_THINKING", value);
        }
        assert!(cmd.output().unwrap().status.success());
        response.assert();
    }
}

#[test]
fn invalid_thinking_fails_locally_without_echoing_its_value() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .expect(0)
        .create();
    for value in ["", "private-invalid-value", "false"] {
        let output = command(&server)
            .env("OPENAI_THINKING", value)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("OPENAI_THINKING"));
        assert!(!stderr.contains("private-invalid-value"));
    }
    response.assert();
}

#[test]
fn native_anthropic_ignores_openai_thinking_even_if_it_is_invalid() {
    let mut server = Server::new();
    let response=server.mock("POST","/v1/messages")
        .match_request(|request| {
            let body:serde_json::Value=serde_json::from_slice(request.body().unwrap()).unwrap();
            body.get("thinking").is_none()
        }).with_header("content-type","application/json")
        .with_body(json!({"type":"message","role":"assistant","content":[{"type":"text","text":"native answer"}],"stop_reason":"end_turn"}).to_string())
        .expect(1).create();
    let output = command(&server)
        .env("RUASK_API_TYPE", "anthropic")
        .env("ANTHROPIC_BASE_URL", format!("{}/v1", server.url()))
        .env("ANTHROPIC_API_KEY", "test-key")
        .env("ANTHROPIC_MODEL", "test-model")
        .env("OPENAI_THINKING", "private-invalid-value")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"native answer\n");
    response.assert();
}

#[test]
fn explicitly_requested_thinking_survives_the_single_streaming_fallback() {
    let mut server = Server::new();
    let unsupported = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::PartialJson(
            json!({"stream":true,"thinking":{"type":"disabled"}}),
        ))
        .with_status(400)
        .with_body(
            r#"{"error":{"code":"unsupported_streaming","message":"streaming is not supported"}}"#,
        )
        .expect(1)
        .create();
    let completed = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::PartialJson(
            json!({"stream":false,"thinking":{"type":"disabled"}}),
        ))
        .with_header("content-type", "application/json")
        .with_body(answer())
        .expect(1)
        .create();
    let output = command(&server)
        .env("OPENAI_THINKING", "disabled")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"  Check:\n    git status\n");
    unsupported.assert();
    completed.assert();
}
