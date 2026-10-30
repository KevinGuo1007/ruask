use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use mockito::{Matcher, Server};
use serde_json::{Value, json};

fn ruask(server: &Server, protocol: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command
        .env_clear()
        .env("RUASK_API_TYPE", protocol)
        .env("OPENAI_BASE_URL", format!("{}/team/v1/", server.url()))
        .env("OPENAI_API_KEY", "openai-test-key")
        .env("OPENAI_MODEL", "openai-test-model")
        .env("ANTHROPIC_BASE_URL", format!("{}/team/v1/", server.url()))
        .env("ANTHROPIC_API_KEY", "native-test-key")
        .env("ANTHROPIC_MODEL", "native-test-model")
        .stdin(Stdio::null())
        .args(["git", "-q", "show changes", "-L", "en", "-p", "linux"]);
    command
}

fn endpoint(protocol: &str) -> &'static str {
    if protocol == "openai" {
        "/team/v1/chat/completions"
    } else {
        "/team/v1/messages"
    }
}

fn answer(protocol: &str) -> Value {
    if protocol == "openai" {
        json!({"choices":[{"index":0,"message":{"role":"assistant","content":"Show changes:\n  git diff"},"finish_reason":"stop"}]})
    } else {
        json!({"type":"message","role":"assistant","content":[{"type":"text","text":"Show changes:\n  git diff"}],"stop_reason":"end_turn"})
    }
}

#[test]
fn explicit_stream_rejection_retries_once_without_changing_the_query() {
    for protocol in ["openai", "anthropic"] {
        let mut server = Server::new();
        let captured = Arc::new(Mutex::new(None));
        let first_body = captured.clone();
        let first = server
        .mock("POST", endpoint(protocol))
        .match_header("authorization", if protocol == "openai" { "Bearer openai-test-key" } else { "Bearer native-test-key" })
        .match_body(Matcher::PartialJson(json!({"stream":true,"model":if protocol == "openai" {"openai-test-model"} else {"native-test-model"}})))
        .match_request(move |request| {
            let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            *first_body.lock().unwrap() = Some(body);
            true
        })
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(json!({"error":{"type":"invalid_request_error","param":"stream","code":"unsupported_parameter","message":"stream is not supported"}}).to_string())
        .expect(1)
        .create();
        let second = server
            .mock("POST", endpoint(protocol))
            .match_header(
                "authorization",
                if protocol == "openai" {
                    "Bearer openai-test-key"
                } else {
                    "Bearer native-test-key"
                },
            )
            .match_request(move |request| {
                let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
                let input: Value = serde_json::from_str(
                    body["messages"][if protocol == "openai" { 1 } else { 0 }]["content"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
                let mut original: Value = captured.lock().unwrap().clone().unwrap();
                original["stream"] = json!(false);
                body == original
                    && body["stream"] == false
                    && body["model"]
                        == if protocol == "openai" {
                            "openai-test-model"
                        } else {
                            "native-test-model"
                        }
                    && input["command_query"] == "git"
                    && input["question"] == "show changes"
                    && input["language"] == "en"
                    && input["platform"] == "linux"
            })
            .with_header("content-type", "application/json")
            .with_body(answer(protocol).to_string())
            .expect(1)
            .create();
        let output = ruask(&server, protocol).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Show changes:\n  git diff\n");
        let notices = String::from_utf8_lossy(&output.stderr);
        assert!(
            notices.contains("回退") && notices.contains("非流式"),
            "{notices}"
        );
        assert_eq!(notices.lines().count(), 1, "{notices}");
        first.assert();
        second.assert();
    }
}

#[test]
fn complete_json_answers_to_streaming_requests_are_reused_with_only_a_stderr_notice() {
    for protocol in ["openai", "anthropic"] {
        let mut server = Server::new();
        let response = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":true})))
            .with_header("content-type", "application/json; charset=utf-8")
            .with_body(answer(protocol).to_string())
            .expect(1)
            .create();
        let retry = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":false})))
            .expect(0)
            .create();
        let output = ruask(&server, protocol).output().unwrap();
        assert!(output.status.success(), "{protocol}: {output:?}");
        assert_eq!(output.stdout, b"Show changes:\n  git diff\n");
        let notice = String::from_utf8_lossy(&output.stderr);
        assert!(
            notice.contains("非流式") && notice.contains("JSON"),
            "{notice}"
        );
        assert!(!notice.contains("回退"), "{notice}");
        assert_eq!(notice.lines().count(), 1);
        response.assert();
        retry.assert();
    }
}

#[test]
fn ordinary_failures_never_retry_even_when_an_error_mentions_stream() {
    for protocol in ["openai", "anthropic"] {
        for (status, error) in [
            (
                400,
                json!({"type":"authentication_error","message":"stream is not supported without a valid API key"}),
            ),
            (
                400,
                json!({"type":"rate_limit_error","message":"stream is not supported while rate limited"}),
            ),
            (
                400,
                json!({"param":"model","code":"unsupported_parameter","message":"stream is not supported for the invalid model"}),
            ),
            (
                400,
                json!({"param":"stream","message":"stream must be a boolean"}),
            ),
            (
                400,
                json!({"message":"unsupported parameter: stream_options"}),
            ),
            (400, json!({"message":"upstream not supported"})),
            (401, json!({"message":"stream is not supported"})),
            (403, json!({"message":"stream is not supported"})),
            (429, json!({"message":"stream is not supported"})),
            (408, json!({"message":"request timeout"})),
            (504, json!({"message":"upstream timeout"})),
            (500, json!({"message":"internal error"})),
            (501, json!({"message":"not implemented"})),
        ] {
            let mut server = Server::new();
            let response = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":true})))
                .with_status(status)
                .with_header("content-type", "application/json")
                .with_body(json!({"error":error}).to_string())
                .expect(1)
                .create();
            let retry = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":false})))
                .expect(0)
                .create();
            let output = ruask(&server, protocol).output().unwrap();
            assert!(!output.status.success(), "{protocol} {status}: {output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains(&format!("HTTP {status}")), "{stderr}");
            assert!(!stderr.contains("回退"), "{stderr}");
            response.assert();
            retry.assert();
        }
    }
}

fn event(protocol: &str, value: Value) -> String {
    if protocol == "anthropic" {
        format!(
            "event: {}\ndata: {value}\n\n",
            value["type"].as_str().unwrap()
        )
    } else {
        format!("data: {value}\n\n")
    }
}

fn partial_stream(protocol: &str, text: &str) -> String {
    if protocol == "anthropic" {
        event(
            protocol,
            json!({"type":"message_start","message":{"role":"assistant"}}),
        ) + &event(
            protocol,
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        ) + &event(
            protocol,
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        )
    } else {
        event(
            protocol,
            json!({"choices":[{"index":0,"delta":{"content":text}}]}),
        )
    }
}

#[test]
fn explicit_stream_errors_before_display_retry_once_and_discard_unseen_partial_text() {
    for protocol in ["openai", "anthropic"] {
        for prefix in ["", "unfinished unseen text"] {
            let mut server = Server::new();
            let first = server.mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":true})))
                .with_header("content-type", "text/event-stream")
                .with_body(partial_stream(protocol, prefix) + &event(protocol, json!({"type":"error","error":{"type":"invalid_request_error","param":"stream","message":"Streaming is not supported"}})))
                .expect(1).create();
            let second = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":false})))
                .with_header("content-type", "application/json")
                .with_body(answer(protocol).to_string())
                .expect(1)
                .create();
            let output = ruask(&server, protocol).output().unwrap();
            assert!(output.status.success(), "{protocol}: {output:?}");
            assert_eq!(output.stdout, b"Show changes:\n  git diff\n");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("回退"),
                "{output:?}"
            );
            first.assert();
            second.assert();
        }
    }
}

#[test]
fn explicit_stream_errors_after_display_keep_output_incomplete_without_retry() {
    for protocol in ["openai", "anthropic"] {
        let mut server = Server::new();
        let first = server.mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":true})))
            .with_header("content-type", "text/event-stream")
            .with_body(partial_stream(protocol, "Visible line\nunfinished") + &event(protocol, json!({"type":"error","error":{"param":"stream","code":"unsupported_parameter","message":"stream is not supported"}})))
            .expect(1).create();
        let retry = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":false})))
            .expect(0)
            .create();
        let output = ruask(&server, protocol).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Visible line\n");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("不完整") && !stderr.contains("回退"),
            "{stderr}"
        );
        first.assert();
        retry.assert();
    }
}

#[test]
fn a_failed_non_streaming_fallback_never_issues_a_third_request() {
    for protocol in ["openai", "anthropic"] {
        for (status, body) in [
            (
                400,
                json!({"error":{"param":"stream","message":"stream is not supported"}}).to_string(),
            ),
            (
                401,
                json!({"error":{"type":"authentication_error","message":"bad native-test-key"}})
                    .to_string(),
            ),
            (
                429,
                json!({"error":{"type":"rate_limit_error","message":"quota"}}).to_string(),
            ),
            (
                503,
                json!({"error":{"type":"api_error","message":"unavailable"}}).to_string(),
            ),
            (200, "invalid JSON".to_owned()),
            (
                200,
                if protocol == "openai" {
                    json!({"choices":[{"message":{"content":"truncated"},"finish_reason":"length"}]}).to_string()
                } else {
                    json!({"type":"message","role":"assistant","content":[{"type":"text","text":"truncated"}],"stop_reason":"max_tokens"}).to_string()
                },
            ),
        ] {
            let mut server = Server::new();
            let first = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":true})))
                .with_status(422)
                .with_header("content-type", "application/json")
                .with_body(json!({"error":{"code":"unsupported_streaming"}}).to_string())
                .expect(1)
                .create();
            let fallback = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":false})))
                .with_status(status)
                .with_header("content-type", "application/json")
                .with_body(body)
                .expect(1)
                .create();
            let output = ruask(&server, protocol).output().unwrap();
            assert!(!output.status.success(), "{protocol} {status}: {output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                stderr.lines().filter(|line| line.contains("回退")).count(),
                1,
                "{stderr}"
            );
            assert!(
                !stderr.contains("native-test-key") && !stderr.contains("openai-test-key"),
                "{stderr}"
            );
            first.assert();
            fallback.assert();
        }
    }
}

#[test]
fn incomplete_json_answers_are_failures_without_retry_or_success_notices() {
    for protocol in ["openai", "anthropic"] {
        for body in [
            json!({"error":{"type":"api_error","message":"native-test-key private details"}}),
            if protocol == "openai" {
                json!({"choices":[{"message":{"content":"unfinished"}}]})
            } else {
                json!({"type":"message","role":"assistant","content":[{"type":"text","text":"unfinished"}]})
            },
        ] {
            let mut server = Server::new();
            let first = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":true})))
                .with_header("content-type", "application/json")
                .with_body(body.to_string())
                .expect(1)
                .create();
            let retry = server
                .mock("POST", endpoint(protocol))
                .match_body(Matcher::PartialJson(json!({"stream":false})))
                .expect(0)
                .create();
            let output = ruask(&server, protocol).output().unwrap();
            assert!(!output.status.success(), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !stderr.contains("回退")
                    && !stderr.contains("实际使用非流式")
                    && !stderr.contains("native-test-key"),
                "{stderr}"
            );
            first.assert();
            retry.assert();
        }
    }
}

#[test]
fn explicit_capability_errors_in_successful_json_envelopes_can_fallback() {
    for protocol in ["openai", "anthropic"] {
        let mut server = Server::new();
        let first = server.mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":true})))
            .with_header("content-type", "application/json")
            .with_body(json!({"error":{"param":"stream","code":"unsupported_parameter","message":"stream is not supported"}}).to_string())
            .expect(1).create();
        let fallback = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":false})))
            .with_header("content-type", "application/json")
            .with_body(answer(protocol).to_string())
            .expect(1)
            .create();
        let output = ruask(&server, protocol).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Show changes:\n  git diff\n");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("回退"),
            "{output:?}"
        );
        first.assert();
        fallback.assert();
    }
}

#[test]
fn explicit_plain_text_stream_rejection_is_capability_evidence() {
    for protocol in ["openai", "anthropic"] {
        let mut server = Server::new();
        let first = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":true})))
            .with_status(400)
            .with_header("content-type", "text/plain")
            .with_body("This endpoint does not support streaming.")
            .expect(1)
            .create();
        let fallback = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":false})))
            .with_header("content-type", "application/json")
            .with_body(answer(protocol).to_string())
            .expect(1)
            .create();
        let output = ruask(&server, protocol).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Show changes:\n  git diff\n");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("回退"),
            "{output:?}"
        );
        first.assert();
        fallback.assert();
    }
}

#[test]
fn complete_json_without_a_json_content_type_is_still_reused() {
    for protocol in ["openai", "anthropic"] {
        let mut server = Server::new();
        let first = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":true})))
            .with_body(format!(" \n{}", answer(protocol)))
            .expect(1)
            .create();
        let retry = server
            .mock("POST", endpoint(protocol))
            .match_body(Matcher::PartialJson(json!({"stream":false})))
            .expect(0)
            .create();
        let output = ruask(&server, protocol).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"Show changes:\n  git diff\n");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("JSON") && !stderr.contains("回退"),
            "{stderr}"
        );
        first.assert();
        retry.assert();
    }
}

#[test]
fn a_transport_disconnect_after_the_request_never_starts_a_fallback() {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    for protocol in ["openai", "anthropic"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let service = thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut connection);
            let mut first_line = String::new();
            reader.read_line(&mut first_line).unwrap();
            assert!(first_line.starts_with(&format!("POST {} ", endpoint(protocol))));
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
            }
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["stream"], true);
            drop(connection);
            listener
        });
        let server = Server::new();
        let mut command = ruask(&server, protocol);
        command
            .env("OPENAI_BASE_URL", format!("{base_url}/team/v1"))
            .env("ANTHROPIC_BASE_URL", format!("{base_url}/team/v1"));
        let output = command.output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("模型请求失败") && !stderr.contains("回退"),
            "{stderr}"
        );
        assert!(!stderr.contains("test-key"), "{stderr}");
        let listener = service.join().unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
