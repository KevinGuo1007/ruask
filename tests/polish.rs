//! v0.3 体验打磨：版本信息、选项写法、首次使用引导、错误提示、请求头、stdin 与等待提示、tldr 助记符。

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use mockito::Server;
use serde_json::json;

mod support;
use support::openai_stream;

const TAR: &str = "# tar\n\n> Archiving utility.\n\n- E[x]tract an archive to a [d]irectory:\n\n`tar xf {{path/to/source.tar}} -C {{path/to/directory}}`\n";
const SS: &str =
    "# ss\n\n> Investigate sockets.\n\n- Show all sockets:\n\n`ss -a {{--tcp|--udp}} {{port}}`\n";

fn pages() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (platform, name, text) in [("common", "tar", TAR), ("linux", "ss", SS)] {
        let path = dir.path().join("pages").join(platform);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(format!("{name}.md")), text).unwrap();
    }
    dir
}

fn bare() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command.env_clear().stdin(Stdio::null());
    command
}

fn model(command: &mut Command, base_url: &str) {
    command
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model");
}

#[test]
fn version_reports_the_package_and_local_pages() {
    let empty = tempfile::tempdir().unwrap();
    let output = bare()
        .env("HOME", empty.path())
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with(concat!("ruask ", env!("CARGO_PKG_VERSION"), "\n")),
        "{stdout}"
    );
    assert!(stdout.contains("未安装") && stdout.contains("--update-pages"));

    let dir = pages();
    let installed = bare()
        .env("RUASK_TLDR_DIR", dir.path())
        .arg("-V")
        .output()
        .unwrap();
    let stdout = String::from_utf8(installed.stdout).unwrap();
    assert!(
        stdout.contains(&dir.path().display().to_string()),
        "{stdout}"
    );
}

#[test]
fn attached_option_values_are_accepted() {
    let dir = pages();
    for args in [
        vec!["--language=en", "--platform=linux", "--color=never", "tar"],
        vec!["-Len", "-plinux", "--color", "never", "tar"],
    ] {
        let output = bare()
            .env("RUASK_TLDR_DIR", dir.path())
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("Archiving utility."));
    }
    let output = bare().args(["--help=yes"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("未知选项 --help=yes"));
    // `--` 之后的内容原样属于目标命令。
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            support::request_query(request)["command_query"] == "grep --color=auto"
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream(
            "- Color matches:\n`grep --color=auto {{pattern}}`",
        ))
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let output = command
        .args(["--color", "never", "--", "grep", "--color=auto"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    mock.assert();
}

#[test]
fn first_use_without_pages_or_configuration_points_to_update_pages() {
    let empty = tempfile::tempdir().unwrap();
    let output = bare()
        .env("HOME", empty.path())
        .arg("tar")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("OPENAI_BASE_URL") && stderr.contains("ruask --update-pages"),
        "{stderr}"
    );
    // 带问题的查询必须使用模型，不提示页面。
    let output = bare()
        .env("HOME", empty.path())
        .args(["-q", "list ports"])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("--update-pages"));
}

#[test]
fn refused_connections_explain_the_next_step() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut command = bare();
    model(&mut command, &format!("http://127.0.0.1:{port}/v1"));
    let output = command.args(["-q", "list ports"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("模型请求失败：无法连接 127.0.0.1:{port}"))
            && stderr.contains("base_url"),
        "{stderr}"
    );
}

/// 本机“代理”：`accept_connect` 为真时先回应 CONNECT 成功再断开（代理连不上目标），否则立即断开。
fn failing_proxy(accept_connect: bool) -> (String, thread::JoinHandle<()>) {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = proxy.local_addr().unwrap();
    let service = thread::spawn(move || {
        let (mut socket, _) = proxy.accept().unwrap();
        if accept_connect {
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            socket
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .unwrap();
            // 读完经隧道发来的请求头后断开，ureq 读取响应时遇到对端关闭。
            line.clear();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
        }
    });
    (format!("http://user:secret@{address}"), service)
}

#[test]
fn proxy_failures_name_the_variable_without_its_address() {
    for (accept_connect, expected) in [
        // 立即断开时，可能在 CONNECT 阶段或读取响应时失败，两种提示都要点名代理变量。
        (false, "HTTP_PROXY"),
        (true, "检测到 HTTP_PROXY"),
    ] {
        let (proxy, service) = failing_proxy(accept_connect);
        let mut command = bare();
        model(&mut command, "http://192.0.2.1/v1");
        let output = command
            .env("HTTP_PROXY", proxy)
            .args(["-q", "list ports"])
            .output()
            .unwrap();
        service.join().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected) && stderr.contains("NO_PROXY"),
            "{stderr}"
        );
        assert!(!stderr.contains("secret"), "{stderr}");
    }
}

#[test]
fn streaming_requests_do_not_accept_compressed_responses() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_header("accept-encoding", "identity")
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("- List:\n`ls`"))
        .expect(1)
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let output = command.args(["--color", "never", "ls"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    mock.assert();
}

#[test]
fn an_open_stdin_is_reported_once_and_still_read_to_the_end() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            support::request_query(request)["output_context"] == "error: late input\n"
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("> The input reports an error."))
        .expect(1)
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let mut child = command
        .args(["--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    thread::sleep(Duration::from_millis(1400));
    stdin.write_all(b"error: late input\n").unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("正在等待 stdin").count(), 1, "{stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("reports an error"));
    mock.assert();
}

#[test]
fn quick_stdin_and_non_terminal_stderr_show_no_waiting_messages() {
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("> Fine."))
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let mut child = command
        .args(["--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"ok\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// stderr 是终端时显示等待提示，第一行回答前清除；stdout 不受影响。
#[cfg(unix)]
#[test]
fn a_terminal_stderr_shows_a_waiting_line_until_the_first_answer_line() {
    use std::fs::File;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let service = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut line = String::new();
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
        thread::sleep(Duration::from_millis(500));
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let event = format!(
            "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"index":0,"delta":{"content":"> Done.\n"}}]}),
            json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
        );
        socket.write_all(event.as_bytes()).unwrap();
    });

    let (mut master, mut slave) = (-1, -1);
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
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    let _guard = slave.try_clone().unwrap();
    let mut command = bare();
    model(&mut command, &url);
    let output = command
        .args(["-L", "zh", "--color", "never", "-q", "slow"])
        .stdout(Stdio::piped())
        .stderr(Stdio::from(slave))
        .output()
        .unwrap();
    service.join().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "  Done.\n");
    let flags = unsafe { libc::fcntl(terminal.as_raw_fd(), libc::F_GETFL) };
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
    let mut seen = Vec::new();
    let mut buffer = [0; 1024];
    while let Ok(count) = terminal.read(&mut buffer) {
        if count == 0 {
            break;
        }
        seen.extend_from_slice(&buffer[..count]);
    }
    let seen = String::from_utf8_lossy(&seen);
    assert!(seen.contains("正在询问 test-model…"), "{seen:?}");
    // 等待提示在回答前清除；之后 stderr 是终端，回答末尾附上模型与耗时。
    let cleared = seen.find("\r\x1b[K").expect("等待提示没有清除");
    let footer = seen
        .find("AI 生成，请核对 · test-model")
        .expect("缺少来源标识");
    assert!(cleared < footer, "{seen:?}");
}

#[test]
fn tldr_mnemonics_and_notes_are_rendered_for_the_terminal() {
    let dir = pages();
    let plain = bare()
        .env("RUASK_TLDR_DIR", dir.path())
        .args(["-L", "en", "-p", "linux", "--color", "never", "tar"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(plain.stdout).unwrap();
    assert!(
        stdout.contains("  Extract an archive to a directory:\n"),
        "{stdout}"
    );
    assert!(stdout.contains("{{path/to/source.tar}}"), "{stdout}");

    let colored = bare()
        .env("RUASK_TLDR_DIR", dir.path())
        .args(["-L", "en", "-p", "linux", "--color", "always", "tar"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(colored.stdout).unwrap();
    assert!(
        stdout.contains("E\x1b[4mx\x1b[24mtract an archive to a \x1b[4md\x1b[24mirectory:"),
        "{stdout:?}"
    );
    assert!(
        stdout.contains("tar xf \x1b[3;4mpath/to/source.tar\x1b[23;24m -C"),
        "{stdout:?}"
    );

    let note = bare()
        .env("RUASK_TLDR_DIR", dir.path())
        .args(["-L", "en", "-p", "osx", "--color", "always", "ss"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(note.stdout).unwrap();
    assert!(
        stdout.contains("\x1b[33m  Note: this page is for Linux only"),
        "{stdout:?}"
    );
    assert!(stdout.contains("\n  Investigate sockets.\n"), "{stdout:?}");
    // 多选一保留花括号，避免看起来像管道；普通占位符去掉花括号。
    assert!(
        stdout.contains("ss -a \x1b[3;4m{{--tcp|--udp}}\x1b[23;24m \x1b[3;4mport\x1b[23;24m"),
        "{stdout:?}"
    );
}

#[test]
fn unknown_options_after_the_command_belong_to_the_target_command() {
    let dir = pages();
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let query = support::request_query(request);
            query["command_query"] == "tar -xzvf archive.tgz"
                && query["reference"]
                    .as_str()
                    .is_some_and(|r| r.contains("tar xf"))
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("> Extract a gzipped archive verbosely."))
        .expect(1)
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let output = command
        .env("RUASK_TLDR_DIR", dir.path())
        .args(["tar", "-xzvf", "archive.tgz", "--color", "never"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    mock.assert();
    // 命令之前的未知选项仍然报错。
    let output = bare().args(["-xzvf", "tar"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

#[test]
fn exit_codes_tell_scripts_why_a_query_failed() {
    let empty = tempfile::tempdir().unwrap();
    let usage = bare().args(["-x"]).output().unwrap();
    assert_eq!(usage.status.code(), Some(2), "{usage:?}");
    let config = bare()
        .env("HOME", empty.path())
        .args(["-q", "list ports"])
        .output()
        .unwrap();
    assert_eq!(config.status.code(), Some(3), "{config:?}");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut command = bare();
    model(&mut command, &format!("http://127.0.0.1:{port}/v1"));
    let network = command.args(["-q", "list ports"]).output().unwrap();
    assert_eq!(network.status.code(), Some(4), "{network:?}");

    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(format!(
            "data: {}\n\n",
            json!({"choices":[{"index":0,"delta":{"content":"> Partial answer\n"}}]})
        ))
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let incomplete = command
        .args(["--color", "never", "-q", "list ports"])
        .output()
        .unwrap();
    assert_eq!(incomplete.status.code(), Some(5), "{incomplete:?}");
    assert_eq!(
        String::from_utf8_lossy(&incomplete.stdout),
        "  Partial answer\n"
    );

    // 断在逐字显示的行中间：已显示的文字保留并补上换行，错误另起一行写到 stderr。
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(format!(
            "data: {}\n\n",
            json!({"choices":[{"index":0,"delta":{"content":"> Partial answer that stops"}}]})
        ))
        .create();
    let mut command = bare();
    model(&mut command, &format!("{}/v1", server.url()));
    let cut = command
        .args(["--color", "never", "-q", "list ports"])
        .output()
        .unwrap();
    assert_eq!(cut.status.code(), Some(5), "{cut:?}");
    assert_eq!(
        String::from_utf8_lossy(&cut.stdout),
        "  Partial answer that stops\n"
    );
}

#[test]
fn short_help_points_to_the_full_help_and_english_is_explicit() {
    let short = bare().arg("-h").output().unwrap();
    let full = bare().arg("--help").output().unwrap();
    assert!(short.status.success() && full.status.success());
    let (short, full) = (
        String::from_utf8(short.stdout).unwrap(),
        String::from_utf8(full.stdout).unwrap(),
    );
    assert!(short.contains("--help（完整帮助）"), "{short}");
    assert!(short.lines().count() * 2 < full.lines().count(), "{short}");
    assert!(full.contains("退出码"), "{full}");

    let english = bare().args(["-L", "en", "-h"]).output().unwrap();
    let english = String::from_utf8(english.stdout).unwrap();
    assert!(english.starts_with("Usage: ruask"), "{english}");
    let english = bare().args(["--language=en", "--help"]).output().unwrap();
    let english = String::from_utf8(english.stdout).unwrap();
    assert!(english.contains("Exit codes: 0 success"), "{english}");
}
