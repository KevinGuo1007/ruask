use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn ruask(base_url: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command
        .env_clear()
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model")
        .stdin(Stdio::null());
    command
}

fn read_request(stream: &mut TcpStream) -> serde_json::Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).unwrap();
    assert!(first_line.starts_with("POST /v1/chat/completions "));
    let mut content_length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[test]
fn openai_displays_a_complete_line_before_the_service_finishes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (release, wait) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request["stream"], true);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        // Send individual bytes, including splits inside Unicode and event fields.
        let first = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"首行 🦀\\n尾\"},\"finish_reason\":null}]}\n\n";
        for byte in first.as_bytes() {
            stream.write_all(&[*byte]).unwrap();
        }
        stream.flush().unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        stream.write_all(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"line\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n").unwrap();
    });
    let mut child = ruask(&base_url)
        .arg("git")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (lines, next_line) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            lines.send(line.unwrap()).unwrap();
        }
    });
    assert_eq!(
        next_line.recv_timeout(Duration::from_secs(5)).unwrap(),
        "首行 🦀"
    );
    release.send(()).unwrap();
    assert_eq!(
        next_line.recv_timeout(Duration::from_secs(5)).unwrap(),
        "尾line"
    );
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    reader.join().unwrap();
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn a_closed_downstream_ends_quietly_without_waiting_for_the_stream_end() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (release, wait) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request = read_request(&mut stream);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"first line\\n\"}}]}\n\n").unwrap();
        stream.flush().unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let fd: OwnedFd = writer.into();
    let child = ruask(&base_url)
        .arg("git")
        .stdout(Stdio::from(fd))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (finished, result) = mpsc::channel();
    thread::spawn(move || {
        finished.send(child.wait_with_output()).unwrap();
    });
    let output = result
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    release.send(()).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    server.join().unwrap();
}

fn event(value: serde_json::Value) -> String {
    format!("data: {value}\n\n")
}

#[test]
fn a_stream_error_preserves_complete_lines_and_never_replays_the_answer() {
    use mockito::Server;
    use serde_json::json;
    let mut server = Server::new();
    let body =
        event(json!({"choices":[{"index":0,"delta":{"content":"Readable line\nunfinished"}}]}))
            + &event(
                json!({"error":{"type":"server_error","message":"test-key internal details"}}),
            )
            + &event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}))
            + "data: [DONE]\n\n";
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .expect(1)
        .create();
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
    assert!(!error.contains("test-key"), "{error}");
    response.assert();
}

#[test]
fn a_broken_http_body_marks_displayed_output_incomplete() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request = read_request(&mut stream);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 9999\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Visible line\\nunfinished\"}}]}\n\n").unwrap();
    });
    let output = ruask(&base_url).arg("git").output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Visible line\n");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("不完整"),
        "{output:?}"
    );
    server.join().unwrap();
}

#[test]
fn openai_ignores_non_answer_events_and_handles_multiline_crlf_events() {
    use mockito::Server;
    use serde_json::json;
    let mut server = Server::new();
    let body = ": heartbeat\r\nevent: message\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\r\n\r\n".to_owned()
        + &event(json!({"choices":[]}))
        + &event(json!({"choices":[{"index":0,"delta":{"content":null,"reasoning_content":"private thought","tool_calls":[{"function":{"arguments":"secret"}}]}}]}))
        + &event(json!({"choices":[{"index":1,"delta":{"content":"other choice"}}]}))
        + "data: {\"choices\": [\r\ndata: {\"index\":0,\"delta\":{\"content\":\"  中文\"}}]}\r\n\r\n"
        + &event(json!({"choices":[{"index":0,"delta":{"content":" 🦀\nlast line"}}]}))
        + &event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}))
        + "data: [DONE]\n\n";
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .expect(1)
        .create();
    let output = ruask(&format!("{}/v1", server.url()))
        .arg("git")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "  中文 🦀\nlast line\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn incomplete_and_truncated_streams_keep_only_complete_displayed_lines() {
    use mockito::Server;
    use serde_json::json;
    for ending in [
        String::new(),
        "data: [DONE]\n\n".to_owned(),
        event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})),
        event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"length"}]}))
            + "data: [DONE]\n\n",
        event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"content_filter"}]}))
            + "data: [DONE]\n\n",
        "data: {broken json}\n\n".to_owned(),
    ] {
        let mut server = Server::new();
        let body =
            event(json!({"choices":[{"index":0,"delta":{"content":"Complete line\nunfinished"}}]}))
                + &ending;
        let response = server
            .mock("POST", "/v1/chat/completions")
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{ending}: {output:?}");
        assert_eq!(output.stdout, b"Complete line\n", "{ending}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("不完整"),
            "{ending}: {output:?}"
        );
        response.assert();
    }
}

#[test]
fn empty_and_whitespace_only_streams_fail_without_pseudo_body_output() {
    use mockito::Server;
    use serde_json::json;
    for content in [None, Some(""), Some(" \n\t")] {
        let mut server = Server::new();
        let body = event(
            json!({"choices":[{"index":0,"delta":{"role":"assistant","content":content},"finish_reason":null}]}),
        ) + &event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}))
            + "data: [DONE]\n\n";
        let response = server
            .mock("POST", "/v1/chat/completions")
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .expect(1)
            .create();
        let output = ruask(&format!("{}/v1", server.url()))
            .arg("git")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("没有文本回答"),
            "{output:?}"
        );
        response.assert();
    }
}
