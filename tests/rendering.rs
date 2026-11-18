use std::process::{Command, Stdio};

use mockito::Server;
use serde_json::{Value, json};

mod support;
use support::{openai_stream, request_query};

#[derive(Clone, Copy, Default, Debug)]
struct TerminalStyle {
    italic: bool,
    bold: bool,
    foreground: Option<u16>,
}

// Interpret public terminal SGR semantics independently of the renderer's escape strings.
fn terminal_characters(bytes: &[u8]) -> Vec<(char, TerminalStyle)> {
    let text = String::from_utf8_lossy(bytes);
    let mut characters = text.chars();
    let mut style = TerminalStyle::default();
    let mut result = Vec::new();
    while let Some(character) = characters.next() {
        if character != '\x1b' {
            result.push((character, style));
            continue;
        }
        assert_eq!(characters.next(), Some('['));
        let mut sequence = String::new();
        for value in characters.by_ref() {
            if value == 'm' {
                break;
            }
            sequence.push(value);
        }
        for code in sequence
            .split(';')
            .map(|value| value.parse::<u16>().unwrap_or(0))
        {
            match code {
                0 => style = TerminalStyle::default(),
                1 => style.bold = true,
                3 => style.italic = true,
                22 => style.bold = false,
                23 => style.italic = false,
                30..=37 => style.foreground = Some(code),
                39 => style.foreground = None,
                _ => {}
            }
        }
    }
    result
}

#[test]
fn inline_emphasis_restores_the_actual_terminal_attributes() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "# `git` reference\n> Use `git` normally.\n",
            "- Compare revisions:\n`git diff {{base}}..{{head}} --stat`"
        )))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let observed = terminal_characters(&output.stdout);
    let visible: String = observed.iter().map(|(character, _)| character).collect();
    for (needle, bold, foreground) in [
        (" reference", true, Some(35)),
        (" normally.", false, None),
        ("..", false, Some(36)),
        (" --stat", false, Some(36)),
    ] {
        let index = visible[..visible.find(needle).unwrap()].chars().count();
        for (_, style) in &observed[index..index + needle.chars().count()] {
            assert!(!style.italic, "italic leaked into {needle:?}: {style:?}");
            assert_eq!(style.bold, bold, "{needle:?}");
            assert_eq!(style.foreground, foreground, "{needle:?}");
        }
    }
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn bullet_wrapped_command_lines_are_normalized_without_reclassifying_inline_mentions() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "> Inspect `Cargo.toml` and the `--version` option.\n",
            "- Inspect `Cargo.toml` settings:\n- `cat Cargo.toml`\n",
            "- Check a version:\n- `cc --version`"
        )))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let characters = terminal_characters(&output.stdout);
    let visible: String = characters.iter().map(|(character, _)| character).collect();
    assert_eq!(
        visible,
        concat!(
            "  Inspect Cargo.toml and the --version option.\n\n",
            "  Inspect Cargo.toml settings:\n    cat Cargo.toml\n\n",
            "  Check a version:\n    cc --version\n"
        )
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("\x1b[36m"))
            .count(),
        2
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn unpaired_example_purposes_preserve_the_answer_and_warn_once_without_retrying() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "- Install: `brew install {{package}}`\n",
            "- List packages: `brew list`"
        )))
        .expect(1)
        .create();
    let output = ruask(&server).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"  Install: brew install {{package}}\n  List packages: brew list\n"
    );
    let notice = String::from_utf8_lossy(&output.stderr);
    assert!(
        notice.contains("格式") && notice.contains("命令行"),
        "{notice}"
    );
    assert_eq!(notice.lines().count(), 1, "{notice}");
    response.assert();
}

#[test]
fn semantic_lines_are_rendered_without_leaking_display_markers() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream(
            "# cargo\n> Manage Rust packages.\n- Create a project:\n`cargo new {{name}}`",
        ))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "never"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        [
            "cargo",
            "Manage Rust packages.",
            "Create a project:",
            "cargo new {{name}}"
        ]
    );
    assert!(!stdout.contains('\x1b'));
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn indented_explanations_are_plain_instead_of_guessed_to_be_commands() {
    let mut server = Server::new();
    let text = "  git status\n  Show a short status.\n  显示简短状态。\n\tAn explanation, not a command.\n";
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(text))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, text.as_bytes());
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn model_is_given_explicit_line_roles_for_prose_purposes_and_commands() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            let instructions = body["messages"][0]["content"].as_str().unwrap();
            ["line markers", "> ", "- ", "backticks", "{{name}}"]
                .iter()
                .all(|rule| instructions.contains(rule))
                && body["stream"] == true
        })
        .with_body(openai_stream(
            "> Show the working tree.\n- Show status:\n`git status`",
        ))
        .expect(1)
        .create();
    let output = ruask(&server).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn missing_or_repeated_model_blank_lines_do_not_change_example_layout() {
    for text in [
        "> Manage packages.\n- Create a project:\n`cargo new {{name}}`\n- Build release:\n`cargo build --release`",
        "\n\n> Manage packages.\n\n\n- Create a project:\n\n`cargo new {{name}}`\n\n\n- Build release:\n\n`cargo build --release`\n\n",
    ] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .with_body(openai_stream(text))
            .expect(1)
            .create();
        let output = ruask(&server).args(["--color", "never"]).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout,
            b"  Manage packages.\n\n  Create a project:\n    cargo new {{name}}\n\n  Build release:\n    cargo build --release\n");
        assert!(output.stderr.is_empty(), "{output:?}");
        response.assert();
    }
}

#[test]
fn semantic_styles_distinguish_prose_purposes_commands_and_inline_code() {
    let mut server = Server::new();
    let response = server.mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(
            "# git\n> Inspect the working tree.\n- Run `git status` for {{path}}:\n`git status -- {{path}}`"))
        .expect(1).create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "\x1b[1;35m  git\x1b[0m\n",
            "  Inspect the working tree.\n\n",
            "\x1b[32m  Run \x1b[3;33mgit status\x1b[0m\x1b[32m for {{path}}:\x1b[0m\n",
            "\x1b[36m    git status -- \x1b[3;4mpath\x1b[23;24m\x1b[0m\n"
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn display_markers_without_text_do_not_count_as_an_answer() {
    for text in ["# \n> \n- \n``", ">   \n\n`  `\n"] {
        let mut server = Server::new();
        let response = server
            .mock("POST", "/v1/chat/completions")
            .with_body(openai_stream(text))
            .expect(1)
            .create();
        let output = ruask(&server).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("没有文本回答"));
        response.assert();
    }
}

#[test]
fn inline_code_and_shell_operators_keep_their_text_in_plain_output() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "> Use `git status` to inspect changes.\n",
            "> 然后检查 `Cargo.toml`。\n",
            "- Run safely:\n",
            "`printf '%s\\n' \"{{name}}\" && echo `date` > result.txt`"
        )))
        .expect(1)
        .create();
    let output = ruask(&server).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "  Use git status to inspect changes.\n",
            "  然后检查 Cargo.toml。\n\n",
            "  Run safely:\n",
            "    printf '%s\\n' \"{{name}}\" && echo `date` > result.txt\n"
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn split_line_markers_inline_code_and_unicode_placeholders_share_stable_layout() {
    let mut server = Server::new();
    let mut body = String::new();
    for text in [
        "#",
        " git\n>",
        " 说明 `git",
        " status`。\n-",
        " Show path:\n`git status -- {{路",
        "径}}`",
    ] {
        body += &format!(
            "data: {}\n\n",
            json!({"choices":[{"index":0,"delta":{"content":text}}]})
        );
    }
    body += "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(body)
        .expect(1)
        .create();
    let output = ruask(&server).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "  git\n  说明 git status。\n\n",
            "  Show path:\n    git status -- {{路径}}\n"
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn unknown_text_keeps_its_blank_lines_even_after_a_semantic_line() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream("> Intro\n  plain A\n\n\n\tplain B\n\n"))
        .expect(1)
        .create();
    let output = ruask(&server).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"  Intro\n  plain A\n\n\n\tplain B\n\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[test]
fn prose_after_commands_and_fenced_comments_are_not_styled_as_commands() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "`git status`\n",
            "> This describes the result.\n",
            "> 这仍然是说明。\n",
            "```sh\n# comment, not executable syntax\ngit diff\n```"
        )))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "\x1b[36m    git status\x1b[0m\n\n",
            "  This describes the result.\n",
            "  这仍然是说明。\n",
            "# comment, not executable syntax\n",
            "\x1b[36m    git diff\x1b[0m\n"
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

#[cfg(unix)]
#[test]
fn narrow_terminals_do_not_insert_newlines_into_copyable_commands() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "> Inspect changes in a narrow terminal.\n",
            "> 窄终端中的说明由终端自动折行。\n",
            "- Show a long command:\n",
            "`git log --format='%h %s' -- {{长路径}}`"
        )))
        .expect(1)
        .create();
    let mut command = ruask(&server);
    command.args(["--color", "never"]);
    let output = terminal_output_at_width(command, 18);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "  Inspect changes in a narrow terminal.\r\n",
            "  窄终端中的说明由终端自动折行。\r\n\r\n",
            "  1 Show a long command:\r\n",
            "    git log --format='%h %s' -- {{长路径}}\r\n"
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    response.assert();
}

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
                && instructions.contains("line markers")
                && instructions.contains("{{name}}")
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("- Show a commit:\n`git show {{commit}}`"))
        .expect(1)
        .create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"\x1b[32m  Show a commit:\x1b[0m\n\x1b[36m    git show \x1b[3;4mcommit\x1b[23;24m\x1b[0m\n"
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
        b"Show a commit:\n\x1b[36mgit show \x1b[3;4mcommit\x1b[23;24m\x1b[0m\n"
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
fn terminal_output(command: Command) -> std::process::Output {
    terminal_output_at_width(command, 0)
}

#[cfg(unix)]
fn terminal_output_at_width(mut command: Command, columns: u16) -> std::process::Output {
    use std::fs::File;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let mut master = -1;
    let mut slave = -1;
    let mut dimensions = libc::winsize {
        ws_row: 24,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // openpty initializes two owned descriptors; neither is shared with another owner.
    // The window size is passed as a raw pointer: glibc takes `*const winsize`, macOS `*mut winsize`.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut dimensions,
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
            .with_body(openai_stream("- Show a commit:\n`git show {{commit}}`\n"))
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
        // 终端中示例带编号（暗色数字）；重定向时保持原样。
        let expected = match (colored, terminal) {
            (true, false) => {
                "\x1b[32m  Show a commit:\x1b[0m\n\x1b[36m    git show \x1b[3;4mcommit\x1b[23;24m\x1b[0m\n"
            }
            (true, true) => {
                "  \x1b[2m1\x1b[0m \x1b[32mShow a commit:\x1b[0m\n\x1b[36m    git show \x1b[3;4mcommit\x1b[23;24m\x1b[0m\n"
            }
            (false, false) => "  Show a commit:\n    git show {{commit}}\n",
            (false, true) => "  1 Show a commit:\n    git show {{commit}}\n",
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
        for text in ["`git", " show {{提", "交}}`\n"] {
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
            .write_all(openai_stream("> 说明尾行").as_bytes())
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
        "\x1b[36m    git show \x1b[3;4m提交\x1b[23;24m\x1b[0m"
    );
    release.send(()).unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), "");
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        "  说明尾行"
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
    let text = "> Inspect a commit.\n- Show a commit:\n`git show {{commit}}`";
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
                b"  Inspect a commit.\n\n\x1b[32m  Show a commit:\x1b[0m\n\x1b[36m    git show \x1b[3;4mcommit\x1b[23;24m\x1b[0m\n"
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
            json!({"choices":[{"index":0,"delta":{"content":"#\n> \n-\n```sh\n"}}]}),
            json!({"error":{"code":"unsupported_streaming","message":"streaming is not supported"}})
        ))
        .expect(1)
        .create();
    let retry = server.mock("POST", "/v1/chat/completions")
        .match_body(Matcher::PartialJson(json!({"stream":false})))
        .with_header("content-type", "application/json")
        .with_body(json!({"choices":[{"message":{"content":"- Show changes:\n`git diff {{commit}}`"},"finish_reason":"stop"}]}).to_string())
        .expect(1).create();
    let output = ruask(&server).args(["--color", "always"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"\x1b[32m  Show changes:\x1b[0m\n\x1b[36m    git diff \x1b[3;4mcommit\x1b[23;24m\x1b[0m\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("回退"),
        "{output:?}"
    );
    first.assert();
    retry.assert();
}

#[cfg(unix)]
#[test]
fn wide_terminals_wrap_explanations_with_hanging_indent_but_not_commands() {
    let mut server = Server::new();
    let response = server
        .mock("POST", "/v1/chat/completions")
        .with_body(openai_stream(concat!(
            "> Inspect the working tree and the staging area before you commit anything.\n",
            "> 中文说明按两列计宽，折行后续行保持缩进，标点不会出现在行首。\n",
            "- Show the staged and unstaged changes for every tracked file:\n",
            "`git diff HEAD --stat --find-renames --find-copies -- {{path/to/directory}}`"
        )))
        .expect(1)
        .create();
    let mut command = ruask(&server);
    command.args(["--color", "never"]);
    let output = terminal_output_at_width(command, 40);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "  Inspect the working tree and the\r\n",
            "  staging area before you commit\r\n",
            "  anything.\r\n",
            "  中文说明按两列计宽，折行后续行保持缩\r\n",
            "  进，标点不会出现在行首。\r\n\r\n",
            "  1 Show the staged and unstaged changes\r\n",
            "    for every tracked file:\r\n",
            "    git diff HEAD --stat --find-renames --find-copies -- {{path/to/directory}}\r\n"
        )
    );
    response.assert();
}

/// 说明行不必等到换行：服务暂停在行中间时，已收到的文字已经显示。
#[test]
fn explanation_text_is_visible_before_its_line_ends() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (release, wait) = mpsc::channel::<()>();
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
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        for text in ["> Inspect the work", "ing tree `git", " status` and"] {
            let event = format!(
                "data: {}\n\n",
                json!({"choices":[{"index":0,"delta":{"content":text}}]})
            );
            socket.write_all(event.as_bytes()).unwrap();
        }
        socket.flush().unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        socket
            .write_all(openai_stream(" more.\n- Show:\n`git status`").as_bytes())
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
    let mut stdout = child.stdout.take().unwrap();
    let (sent, received) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut buffer = [0; 256];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => sent.send(buffer[..count].to_vec()).unwrap(),
            }
        }
    });
    // 服务暂停期间，已配对的行内代码已经显示，行尾未完的空白暂缓。
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("status\x1b[0m and") {
        seen.extend(received.recv_timeout(Duration::from_secs(5)).unwrap());
    }
    assert_eq!(
        String::from_utf8_lossy(&seen),
        "  Inspect the working tree \x1b[3;33mgit status\x1b[0m and"
    );
    release.send(()).unwrap();
    while let Ok(chunk) = received.recv_timeout(Duration::from_secs(5)) {
        seen.extend(chunk);
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&seen),
        "  Inspect the working tree \x1b[3;33mgit status\x1b[0m and more.\n\n\x1b[32m  Show:\x1b[0m\n\x1b[36m    git status\x1b[0m\n"
    );
    reader.join().unwrap();
    service.join().unwrap();
}
