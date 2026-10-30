use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

fn ruask(base_url: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command
        .env_clear()
        .env("RUASK_API_TYPE", "anthropic")
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", "native-key")
        .env("ANTHROPIC_MODEL", "native-model")
        .stdin(Stdio::null());
    command
}

fn event(value: Value) -> String {
    format!(
        "event: {}\ndata: {value}\n\n",
        value["type"].as_str().unwrap()
    )
}

fn start() -> String {
    event(
        json!({"type":"message_start","message":{"type":"message","role":"assistant","content":[],"stop_reason":null}}),
    ) + &event(
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
    )
}

fn text(text: &str) -> String {
    event(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}))
}

fn finish() -> String {
    event(json!({"type":"content_block_stop","index":0}))
        + &event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}))
        + &event(json!({"type":"message_stop"}))
}

#[test]
fn native_messages_stream_displays_a_line_before_the_service_finishes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/team/llm/v1///", listener.local_addr().unwrap());
    let (release, wait) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).unwrap();
        assert!(
            first_line.starts_with("POST /team/llm/v1/messages "),
            "{first_line}"
        );
        let mut headers = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap();
            }
            headers.push_str(&line.to_ascii_lowercase());
        }
        assert!(
            headers.contains("authorization: bearer native-key\r\n"),
            "{headers}"
        );
        assert!(
            headers.contains("anthropic-version: 2026-10-21\r\n"),
            "{headers}"
        );
        assert!(
            headers.contains("anthropic-workspace-id: workspace-test\r\n"),
            "{headers}"
        );
        assert!(!headers.contains("x-api-key:"));
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["model"], "native-model");
        assert_eq!(body["stream"], true);
        assert!(body["max_tokens"].as_u64().is_some_and(|v| v > 0));
        assert!(body["system"].as_str().unwrap().contains("command-line"));
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        let query: Value =
            serde_json::from_str(body["messages"][0]["content"].as_str().unwrap()).unwrap();
        assert_eq!(query["command_query"], "git status");
        assert_eq!(query["question"], "如何看修改");
        assert_eq!(query["language"], "zh");
        assert_eq!(query["platform"], "linux");
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let first = start() + &text("首行 🦀\n尾");
        for byte in first.as_bytes() {
            stream.write_all(&[*byte]).unwrap();
        }
        stream.flush().unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        stream
            .write_all((text("行") + &finish()).as_bytes())
            .unwrap();
    });
    let mut child = ruask(&base_url)
        .env("ANTHROPIC_WORKSPACE_ID", "workspace-test")
        .args([
            "git",
            "status",
            "-q",
            "如何看修改",
            "-L",
            "zh",
            "-p",
            "linux",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, line) = mpsc::channel();
    let reader = thread::spawn(move || {
        for value in BufReader::new(stdout).lines() {
            send.send(value.unwrap()).unwrap();
        }
    });
    assert_eq!(
        line.recv_timeout(Duration::from_secs(5)).unwrap(),
        "首行 🦀"
    );
    release.send(()).unwrap();
    assert_eq!(line.recv_timeout(Duration::from_secs(5)).unwrap(), "尾行");
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    reader.join().unwrap();
    server.join().unwrap();
}

#[test]
fn native_stream_errors_preserve_visible_lines_and_do_not_replay() {
    let mut server = mockito::Server::new();
    let response=server.mock("POST","/v1/messages")
        .with_header("content-type","text/event-stream")
        .with_body(start()+&text("Readable line\nunfinished")+&event(json!({"type":"error","error":{"type":"overloaded_error","message":"native-key private details"}}))+&finish())
        .expect(1).create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Readable line\n");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("不完整") && error.contains("流内错误"),
        "{error}"
    );
    assert!(!error.contains("native-key"), "{error}");
    response.assert();
}

#[test]
fn native_stream_reads_multiple_text_blocks_and_ignores_non_answer_data() {
    let mut server = mockito::Server::new();
    let body = event(
        json!({"type":"message_start","message":{"type":"message","role":"assistant","content":[]}}),
    ) + &event(json!({"type":"ping"}))
        + &event(
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"private thought"}}),
        )
        + &event(
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"more private"}}),
        )
        + &event(
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"secret"}}),
        )
        + &event(json!({"type":"content_block_stop","index":0}))
        + &event(
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":"第一"}}),
        )
        + &event(
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"段\n"}}),
        )
        + &event(json!({"type":"content_block_stop","index":1}))
        + &event(
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","input":{"text":"tool secret"}}}),
        )
        + &event(
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"tool data"}}),
        )
        + &event(json!({"type":"content_block_stop","index":2}))
        + &event(json!({"type":"future_event","text":"not an answer"}))
        + &event(
            json!({"type":"content_block_start","index":3,"content_block":{"type":"text","text":""}}),
        )
        + &event(
            json!({"type":"content_block_delta","index":3,"delta":{"type":"text_delta","text":"第二段 🦀"}}),
        )
        + &event(json!({"type":"content_block_stop","index":3}))
        + &event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}))
        + &event(json!({"type":"message_stop"}));
    let response = server
        .mock("POST", "/v1/messages")
        .with_header("content-type", "text/event-stream")
        .with_body(body.replace('\n', "\r\n"))
        .expect(1)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "第一段\n第二段 🦀\n"
    );
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    response.assert();
}

#[test]
fn native_json_parses_only_text_blocks_with_a_normal_stop_reason() {
    let mut server = mockito::Server::new();
    let response = server
        .mock("POST", "/v1/messages")
        .with_header("content-type", "application/json; charset=utf-8")
        .with_body(
            json!({"type":"message","role":"assistant","stop_reason":"end_turn","content":[
                {"type":"thinking","thinking":"private thought"},
                {"type":"text","text":"Explain output:\n"},
                {"type":"tool_use","input":{"text":"secret"}},
                {"type":"text","text":"  cargo check 🦀"}
            ]})
            .to_string(),
        )
        .expect(1)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Explain output:\n  cargo check 🦀\n"
    );
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    response.assert();
}

#[test]
fn native_stream_requires_message_and_content_block_completion() {
    for body in [
        text("Visible\nunfinished") + &finish(),
        start()
            + &text("Visible\nunfinished")
            + &event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}))
            + &event(json!({"type":"message_stop"})),
    ] {
        let mut server = mockito::Server::new();
        let response = server
            .mock("POST", "/v1/messages")
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("unfinished"),
            "{output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("不完整"),
            "{output:?}"
        );
        response.assert();
    }
}

#[test]
fn native_saved_settings_merge_only_selected_environment_fields() {
    for overrides in [
        vec![],
        vec!["BASE_URL"],
        vec!["API_KEY"],
        vec!["MODEL"],
        vec!["BASE_URL", "API_KEY", "MODEL"],
    ] {
        let workspace = tempfile::tempdir().unwrap();
        let config_dir = workspace.path().join("settings/ruask");
        std::fs::create_dir_all(&config_dir).unwrap();
        let mut saved_server = mockito::Server::new();
        let mut env_server = mockito::Server::new();
        std::fs::write(
            config_dir.join("config.toml"),
            format!(
                r#"
api_type="anthropic"
[openai]
base_url="http://127.0.0.1:9/unselected"
api_key="unselected-file-key"
model="unselected-file-model"
[anthropic]
base_url="{}/saved/v1/"
api_key="saved-key"
model="saved-model"
"#,
                saved_server.url()
            ),
        )
        .unwrap();
        let (server, path) = if overrides.contains(&"BASE_URL") {
            (&mut env_server, "/team/proxy/messages")
        } else {
            (&mut saved_server, "/saved/v1/messages")
        };
        let key = if overrides.contains(&"API_KEY") {
            "override-key"
        } else {
            "saved-key"
        };
        let model = if overrides.contains(&"MODEL") {
            "override-model"
        } else {
            "saved-model"
        };
        let response = server
            .mock("POST", path)
            .match_header("authorization", format!("Bearer {key}").as_str())
            .match_request(move |request| {
                let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
                body["model"] == model
                    && body["stream"] == true
                    && request.header("anthropic-workspace-id").is_empty()
            })
            .with_header("content-type", "text/event-stream")
            .with_body(start() + &text("Saved native response") + &finish())
            .expect(1)
            .create();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
        command
            .env_clear()
            .env("HOME", workspace.path().join("home"))
            .env("XDG_CONFIG_HOME", workspace.path().join("settings"))
            .env("OPENAI_BASE_URL", "http://127.0.0.1:9/unused-env")
            .env("OPENAI_API_KEY", "unselected-env-key")
            .env("OPENAI_MODEL", "unselected-env-model")
            .arg("git")
            .stdin(Stdio::null());
        for name in overrides {
            let value = match name {
                "BASE_URL" => format!("{}/team/proxy///", env_server.url()),
                "API_KEY" => "override-key".into(),
                "MODEL" => "override-model".into(),
                _ => unreachable!(),
            };
            command.env(format!("ANTHROPIC_{name}"), value);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Saved native response\n");
        assert!(output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
}

#[test]
fn native_truncation_missing_end_and_empty_answers_are_not_successful() {
    let valid_partial = start() + &text("Visible line\nunfinished");
    let mut cases = vec![
        valid_partial.clone(),
        valid_partial.clone()
            + &event(json!({"type":"content_block_stop","index":0}))
            + &event(json!({"type":"message_stop"})),
        start() + &finish(),
    ];
    for reason in [
        "max_tokens",
        "model_context_window_exceeded",
        "tool_use",
        "pause_turn",
        "refusal",
        "unknown",
    ] {
        cases.push(
            valid_partial.clone()
                + &event(json!({"type":"content_block_stop","index":0}))
                + &event(json!({"type":"message_delta","delta":{"stop_reason":reason}}))
                + &event(json!({"type":"message_stop"})),
        );
    }
    for body in cases {
        let mut server = mockito::Server::new();
        let response = server
            .mock("POST", "/v1/messages")
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("unfinished"),
            "{output:?}"
        );
        assert!(!output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
    for reason in [json!("max_tokens"), json!("tool_use"), Value::Null] {
        let mut server = mockito::Server::new();
        let response=server.mock("POST","/v1/messages").with_header("content-type","application/json").with_body(json!({"type":"message","role":"assistant","content":[{"type":"text","text":"Visible line\nunfinished"}],"stop_reason":reason}).to_string()).expect(1).create();
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Visible line\n");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("不完整"),
            "{output:?}"
        );
        response.assert();
    }
}

#[test]
fn native_workspace_metadata_rejects_blank_values_before_request() {
    let mut server = mockito::Server::new();
    let requests = server
        .mock("POST", mockito::Matcher::Any)
        .expect(0)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .env("ANTHROPIC_WORKSPACE_ID", "  ")
        .arg("git")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("ANTHROPIC_WORKSPACE_ID"),
        "{output:?}"
    );
    requests.assert();
}

#[test]
fn native_non_answer_failures_never_print_protocol_or_credentials() {
    for (status,content_type,body) in [
        (401,"application/json",json!({"type":"error","error":{"type":"authentication_error","message":"native-key"}}).to_string()),
        (200,"application/json",json!({"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"thinking","thinking":"private"}]}).to_string()),
        (200,"application/json",json!({"choices":[{"message":{"content":"wrong protocol"}}]}).to_string()),
        (200,"text/event-stream",start()+&text("Visible\nunfinished")+"event: content_block_delta\ndata: invalid JSON native-key\n\n"),
    ] {
        let mut server=mockito::Server::new();
        let response=server.mock("POST","/v1/messages").with_status(status).with_header("content-type",content_type).with_body(body).expect(1).create();
        let output=ruask(&format!("{}/v1",server.url())).arg("git").output().unwrap();
        assert!(!output.status.success(),"{output:?}");
        assert!(output.stdout.is_empty() || output.stdout==b"Visible\n","{output:?}");
        let error=String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("native-key"),"{error}");
        assert!(!error.is_empty(),"{output:?}");
        response.assert();
    }
}

#[test]
fn native_normal_stop_sequence_and_usage_events_keep_final_text() {
    let mut server = mockito::Server::new();
    let body = start()
        + &text("Complete text")
        + &event(json!({"type":"content_block_stop","index":0}))
        + &event(
            json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"END"}}),
        )
        + &event(json!({"type":"message_delta","delta":{},"usage":{"output_tokens":10}}))
        + &event(json!({"type":"message_stop"}));
    let response = server
        .mock("POST", "/v1/messages")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .expect(1)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Complete text\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn native_stream_ignores_empty_sse_events() {
    let mut server = mockito::Server::new();
    let body = start() + "data:\n\nevent: ping\ndata: \n\n" + &text("Readable answer") + &finish();
    let response = server
        .mock("POST", "/v1/messages")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .expect(1)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Readable answer\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}
