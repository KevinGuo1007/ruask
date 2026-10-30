use std::process::{Command, Stdio};

use mockito::Server;
use serde_json::{Value, json};

mod support;
use support::{openai_stream, request_query};

fn ruask(server: &Server) -> Command {
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

#[test]
fn forced_color_distinguishes_commands_and_placeholders_without_changing_text() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            let instructions = body["messages"][0]["content"].as_str().unwrap();
            body["stream"] == true
                && body["model"] == "test-model"
                && request_query(request)["command_query"] == "git"
                && instructions.contains("two spaces")
                && instructions.contains("{{name}}")
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("Show a commit:\n  git show {{commit}}"))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"Show a commit:\n\x1b[1;36m  git show \x1b[1;33m{{commit}}\x1b[1;36m\x1b[0m\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn fragmented_fences_and_placeholders_are_rendered_without_transport_markup() {
    let mut server = Server::new();
    let mut body = String::new();
    for text in [
        "Show a commit:\n``",
        "`sh\ngit show {{com",
        "mit}}\n",
        "``",
        "`",
    ] {
        body += &format!(
            "data: {}\n\n",
            json!({"choices":[{"index":0,"delta":{"content":text}}]})
        );
    }
    body += &format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
    );
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"Show a commit:\n\x1b[1;36mgit show \x1b[1;33m{{commit}}\x1b[1;36m\x1b[0m\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn fence_only_answers_fail_without_printing_empty_pseudo_answers() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("\n```sh\n\n```"))
        .expect(1)
        .create();
    let output = ruask(&server).output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("没有文本回答"),
        "{output:?}"
    );
    response.assert();
}

#[test]
fn model_control_sequences_are_removed_even_when_split_across_text_increments() {
    let mut server = Server::new();
    let mut body = String::new();
    for text in [
        "\u{1b}[31",
        "mDescription:\u{1b}[0m\r\n  printf ",
        "\u{1b}]8;;https://example.test",
        "\u{1b}\\{{name}}\u{1b}]8;;\u{1b}",
        "\\\u{7}\u{0}\n",
    ] {
        body += &format!(
            "data: {}\n\n",
            json!({"choices":[{"index":0,"delta":{"content":text}}]})
        );
    }
    body += &format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
    );
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "never"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Description:\n  printf {{name}}\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[cfg(unix)]
fn terminal_output(mut command: Command) -> std::process::Output {
    use std::fs::File;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let mut master = -1;
    let mut slave = -1;
    // openpty initializes two owned descriptors; neither is shared with another owner.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // Each successfully opened descriptor is adopted exactly once.
    let mut terminal = unsafe { File::from_raw_fd(master) };
    let output = unsafe { OwnedFd::from_raw_fd(slave) };
    // Keep the slave open until buffered output is drained (macOS discards it on the final close).
    let _slave_guard = output.try_clone().unwrap();
    command.stdout(Stdio::from(output)).stderr(Stdio::piped());
    let child = command.spawn().unwrap();
    drop(command);
    let mut output = child.wait_with_output().unwrap();
    // The child is done, so drain all available bytes without waiting for future writes.
    let flags = unsafe { libc::fcntl(terminal.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe {
            libc::fcntl(
                terminal.as_raw_fd(),
                libc::F_SETFL,
                flags | libc::O_NONBLOCK,
            )
        },
        0
    );
    let mut buffer = [0; 1024];
    loop {
        match terminal.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => output.stdout.extend_from_slice(&buffer[..count]),
            Err(error)
                if error.raw_os_error() == Some(libc::EIO)
                    || error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                break;
            }
            Err(error) => panic!("PTY read failed: {error}"),
        }
    }
    output
}

#[cfg(unix)]
#[test]
fn color_modes_follow_real_stdout_terminality_and_no_color() {
    for (terminal, mode, no_color, colored) in [
        (false, None, None, false),
        (false, Some("auto"), None, false),
        (false, Some("always"), None, true),
        (false, Some("never"), None, false),
        (true, None, None, true),
        (true, Some("auto"), None, true),
        (true, Some("never"), None, false),
        (true, Some("always"), None, true),
        (true, None, Some("1"), false),
        (true, Some("auto"), Some("1"), false),
        (true, Some("always"), Some("1"), true),
        (true, Some("auto"), Some(""), true),
        (false, Some("always"), Some("1"), true),
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .match_request(|request| {
                let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
                body["stream"] == true && body["model"] == "test-model"
            })
            .with_header("content-type", "text/event-stream")
            .with_body(openai_stream("Show a commit:\n  git show {{commit}}\n"))
            .expect(1)
            .create();
        let mut command = ruask(&server);
        if let Some(mode) = mode {
            command.args(["--color", mode]);
        }
        if let Some(value) = no_color {
            command.env("NO_COLOR", value);
        }
        let output = if terminal {
            terminal_output(command)
        } else {
            command.output().unwrap()
        };
        assert!(
            output.status.success(),
            "terminal={terminal}, mode={mode:?}, NO_COLOR={no_color:?}: {output:?}"
        );
        let expected = if colored {
            "Show a commit:\n\x1b[1;36m  git show \x1b[1;33m{{commit}}\x1b[1;36m\x1b[0m\n"
        } else {
            "Show a commit:\n  git show {{commit}}\n"
        };
        let expected = if terminal {
            expected.replace('\n', "\r\n")
        } else {
            expected.to_owned()
        };
        assert_eq!(
            output.stdout,
            expected.as_bytes(),
            "terminal={terminal}, mode={mode:?}, NO_COLOR={no_color:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
}

#[test]
fn rendered_command_line_is_visible_before_the_service_releases_the_rest() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (release, wait) = mpsc::channel();
    let service = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(&mut socket);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("POST /v1/chat/completions "));
        let mut length = 0;
        loop {
            line.clear();
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
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["stream"], true);
        drop(reader);
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        for text in ["``", "`sh\n  git show {{提", "交}}\n"] {
            let event = format!(
                "data: {}\n\n",
                json!({"choices":[{"index":0,"delta":{"content":text}}]})
            );
            // Byte writes split Unicode and SSE fields independently of text-increment boundaries.
            for byte in event.as_bytes() {
                socket.write_all(&[*byte]).unwrap();
            }
        }
        socket.flush().unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        socket
            .write_all(openai_stream("```\n说明尾行").as_bytes())
            .unwrap();
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_ruask"))
        .env_clear()
        .env("OPENAI_BASE_URL", url)
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model")
        .args(["git", "--color", "always"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sent, received) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            sent.send(line.unwrap()).unwrap();
        }
    });
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        "\x1b[1;36m  git show \x1b[1;33m{{提交}}\x1b[1;36m\x1b[0m"
    );
    release.send(()).unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        "说明尾行"
    );
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    reader.join().unwrap();
    service.join().unwrap();
}

#[test]
fn color_options_fail_locally_and_help_explains_modes_without_credentials() {
    let mut server = Server::new();
    let requests = server
        .mock("POST", mockito::Matcher::Any)
        .expect(0)
        .create();
    for (args, expected) in [
        (vec!["--color"], "选项 --color 需要"),
        (vec!["--color", "rainbow"], "--color 仅支持"),
        (vec!["--color", "--help"], "选项 --color 需要"),
    ] {
        let output = ruask(&server)
            .env_remove("OPENAI_API_KEY")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{output:?}"
        );
    }
    let output = ruask(&server)
        .env_remove("OPENAI_API_KEY")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("--color") && help.contains("auto|always|never") && help.contains("NO_COLOR"),
        "{help}"
    );
    assert!(!help.contains('\u{1b}'), "{help}");
    assert!(output.stderr.is_empty(), "{output:?}");
    requests.assert();
}

fn native_stream(text: &str) -> String {
    [
        json!({"type":"message_start","message":{"role":"assistant"}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
        json!({"type":"message_stop"}),
    ]
    .into_iter()
    .map(|event| {
        format!(
            "event: {}\ndata: {event}\n\n",
            event["type"].as_str().unwrap()
        )
    })
    .collect()
}

#[test]
fn both_protocols_and_fallback_answers_share_the_same_renderer() {
    use mockito::Matcher;
    let text = "Show a commit:\n```sh\ngit show {{commit}}\n```";
    for protocol in ["openai", "anthropic"] {
        for fallback in [false, true] {
            let mut server = Server::new();
            let path = if protocol == "openai" {
                "/v1/chat/completions"
            } else {
                "/v1/messages"
            };
            let first = server.mock("POST", path)
                .match_body(Matcher::PartialJson(json!({"stream":true,"model":"test-model"})))
                .with_status(if fallback { 400 } else { 200 })
                .with_header("content-type", if fallback { "application/json" } else { "text/event-stream" })
                .with_body(if fallback {
                    json!({"error":{"code":"unsupported_streaming","message":"streaming is not supported"}}).to_string()
                } else if protocol == "openai" { openai_stream(text) } else { native_stream(text) })
                .expect(1).create();
            let answer = if protocol == "openai" {
                json!({"choices":[{"message":{"content":text},"finish_reason":"stop"}]})
            } else {
                json!({"type":"message","role":"assistant","content":[{"type":"text","text":text}],"stop_reason":"end_turn"})
            };
            let retry = server
                .mock("POST", path)
                .match_body(Matcher::PartialJson(
                    json!({"stream":false,"model":"test-model"}),
                ))
                .with_header("content-type", "application/json")
                .with_body(answer.to_string())
                .expect(usize::from(fallback))
                .create();
            let output = ruask(&server)
                .env("RUASK_API_TYPE", protocol)
                .env("ANTHROPIC_BASE_URL", format!("{}/v1", server.url()))
                .env("ANTHROPIC_API_KEY", "native-test-key")
                .env("ANTHROPIC_MODEL", "test-model")
                .args(["--color", "always"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{protocol} fallback={fallback}: {output:?}"
            );
            assert_eq!(
                output.stdout,
                b"Show a commit:\n\x1b[1;36mgit show \x1b[1;33m{{commit}}\x1b[1;36m\x1b[0m\n"
            );
            if fallback {
                let notice = String::from_utf8_lossy(&output.stderr);
                assert!(notice.contains("回退"), "{notice}");
                assert_eq!(notice.lines().count(), 1);
            } else {
                assert!(output.stderr.is_empty(), "{output:?}");
            }
            first.assert();
            retry.assert();
        }
    }
}

#[test]
fn filtered_fences_allow_fallback_and_do_not_leak_render_state_into_the_answer() {
    use mockito::Matcher;
    let mut server = Server::new();
    let first = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::PartialJson(json!({"stream":true})))
        .with_header("content-type", "text/event-stream")
        .with_body(format!(
            "data: {}\n\ndata: {}\n\n",
            json!({"choices":[{"index":0,"delta":{"content":"```sh\n"}}]}),
            json!({"error":{"code":"unsupported_streaming","message":"streaming is not supported"}})
        ))
        .expect(1)
        .create();
    let retry = server.mock("POST", "/v1/chat/completions")
        .match_body(Matcher::PartialJson(json!({"stream":false})))
        .with_header("content-type", "application/json")
        .with_body(json!({"choices":[{"message":{"content":"Show changes:\n  git diff {{commit}}"},"finish_reason":"stop"}]}).to_string())
        .expect(1).create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"Show changes:\n\x1b[1;36m  git diff \x1b[1;33m{{commit}}\x1b[1;36m\x1b[0m\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("回退"),
        "{output:?}"
    );
    first.assert();
    retry.assert();
}
