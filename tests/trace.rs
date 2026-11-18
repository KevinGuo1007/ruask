use std::process::{Command, Stdio};

use mockito::Server;
use serde_json::{Value, json};

fn openai_stream(text: &str) -> String {
    format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":{"content":text}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
    )
}

fn command(server: &Server) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command
        .env_clear()
        .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
        .env("OPENAI_API_KEY", "private-credential")
        .env("OPENAI_MODEL", "private-model")
        .stdin(Stdio::null())
        .args(["-q", "private-query"]);
    command
}

fn trace(stderr: &[u8]) -> Value {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("ruask-trace: "))
        .collect();
    assert_eq!(lines.len(), 1, "{text}");
    serde_json::from_str(lines[0]).unwrap()
}

#[test]
fn trace_is_opt_in_and_never_changes_stdout_or_records_private_data() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream(
            "# same title\n> private-answer\n- Check:\n`git status`\n",
        ))
        .expect(2)
        .create();
    let plain = command(&server).output().unwrap();
    let measured = command(&server).env("RUASK_TRACE", "1").output().unwrap();
    assert!(plain.status.success() && measured.status.success());
    assert!(plain.stderr.is_empty());
    assert_eq!(plain.stdout, measured.stdout);
    let diagnostics = String::from_utf8_lossy(&measured.stderr);
    for secret in [
        "private-credential",
        "private-query",
        "private-answer",
        "private-model",
        &server.url(),
    ] {
        assert!(!diagnostics.contains(secret));
    }
    let record = trace(&measured.stderr);
    let events = record["events"].as_array().unwrap();
    for pair in events.windows(2) {
        assert!(pair[0]["elapsed_us"].as_u64() <= pair[1]["elapsed_us"].as_u64());
    }
    let names: Vec<_> = events.iter().map(|e| e["name"].as_str().unwrap()).collect();
    for pair in [
        ("http_start", "http_headers"),
        ("http_headers", "first_http_body_byte"),
        ("first_http_body_byte", "first_answer_text"),
        ("first_answer_text", "first_stdout_line"),
        ("first_stdout_line", "first_command_candidate"),
        ("first_command_candidate", "response_complete"),
        ("response_complete", "exit"),
    ] {
        assert!(
            names.iter().position(|x| *x == pair.0).unwrap()
                < names.iter().position(|x| *x == pair.1).unwrap()
        );
    }
    response.assert();
}

#[test]
fn error_trace_does_not_invent_answer_or_command_timings() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_status(401)
        .with_body(r#"{"error":{"message":"private-credential"}}"#)
        .expect(1)
        .create();
    let output = command(&server).env("RUASK_TRACE", "1").output().unwrap();
    assert!(!output.status.success());
    let record = trace(&output.stderr);
    let events = record["events"].as_array().unwrap();
    assert!(!events.iter().any(|e| matches!(
        e["name"].as_str(),
        Some("first_answer_text" | "first_command_candidate" | "response_complete")
    )));
    assert_eq!(events.last().unwrap()["value"], 0);
    response.assert();
}

#[test]
fn fallback_trace_keeps_both_attempts_and_one_answer() {
    let mut server = Server::new();
    let unsupported = server
        .mock("POST", "/v1/chat/completions")
        .match_body(mockito::Matcher::PartialJson(json!({"stream":true})))
        .with_status(400)
        .with_body(
            r#"{"error":{"code":"unsupported_streaming","message":"streaming is not supported"}}"#,
        )
        .expect(1)
        .create();
    let answer = server.mock("POST", "/v1/chat/completions")
        .match_body(mockito::Matcher::PartialJson(json!({"stream":false})))
        .with_header("content-type", "application/json")
        .with_body(json!({"choices":[{"message":{"content":"- Check:\n`git status`"},"finish_reason":"stop"}]}).to_string())
        .expect(1).create();
    let output = command(&server).env("RUASK_TRACE", "1").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let record = trace(&output.stderr);
    let events = record["events"].as_array().unwrap();
    assert_eq!(
        events.iter().filter(|e| e["name"] == "http_start").count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["name"] == "first_answer_text")
            .count(),
        1
    );
    unsupported.assert();
    answer.assert();
}
