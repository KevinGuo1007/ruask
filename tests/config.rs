use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use mockito::{Matcher, Server};
use tempfile::TempDir;

mod support;
use support::{openai_stream, request_query};

struct Workspace {
    root: TempDir,
}

impl Workspace {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("home")).unwrap();
        fs::create_dir(root.path().join("work")).unwrap();
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
        command
            .env_clear()
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("settings"))
            .current_dir(self.path("work"))
            .stdin(Stdio::null());
        command
    }

    fn write_config(&self, contents: &str) {
        let path = self.path("settings/ruask/config.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn language_uses_cli_then_saved_preference_before_locale() {
    for (saved, args, locale, expected) in [
        ("zh", vec![], "en_US.UTF-8", "zh"),
        ("en", vec![], "zh_CN.UTF-8", "en"),
        ("zh", vec!["-L", "en"], "zh_CN.UTF-8", "en"),
        ("en", vec!["--language", "zh"], "en_US.UTF-8", "zh"),
        ("fr", vec!["-L", "zh"], "en_US.UTF-8", "zh"),
    ] {
        let workspace = Workspace::new();
        workspace.write_config(&format!("language = {saved:?}\n"));
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
        let output = workspace
            .command()
            .arg("git")
            .args(&args)
            .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
            .env("OPENAI_API_KEY", "test-key")
            .env("OPENAI_MODEL", "test-model")
            .env("LANG", locale)
            .env("LANGUAGE", "zh:en")
            .output()
            .unwrap();
        assert_success(&output);
        response.assert();
    }
}

#[test]
fn unsupported_saved_languages_fail_locally_instead_of_using_locale() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for saved in ["fr", "zh_TW", "", " ", "en_US.UTF-8"] {
        let workspace = Workspace::new();
        workspace.write_config(&format!("language = {saved:?}\n"));
        let output = workspace
            .command()
            .arg("git")
            .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
            .env("OPENAI_API_KEY", "test-key")
            .env("OPENAI_MODEL", "test-model")
            .env("LANG", "zh_CN.UTF-8")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("结果语言仅支持 en 或 zh"),
            "{output:?}"
        );
    }
    requests.assert();
}

#[test]
fn init_config_creates_private_template_and_preserves_existing_file() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for xdg in [Some("configured"), None, Some("")] {
        let workspace = Workspace::new();
        let mut command = workspace.command();
        let config = match xdg {
            Some("configured") => workspace.path("settings/ruask/config.toml"),
            value => {
                if value.is_none() {
                    command.env_remove("XDG_CONFIG_HOME");
                } else {
                    command.env("XDG_CONFIG_HOME", "");
                }
                workspace.path("home/.config/ruask/config.toml")
            }
        };
        command
            .arg("--init-config")
            .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
            .env("RUASK_API_TYPE", "invalid");
        let output = command.output().unwrap();
        assert_success(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains(config.to_str().unwrap()));
        let template = fs::read_to_string(&config).unwrap();
        for field in [
            "api_type",
            "language",
            "[openai]",
            "[anthropic]",
            "base_url",
            "model",
            "api_key",
        ] {
            assert!(template.contains(field), "{template}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&config).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let saved = "existing secret-key and deliberately invalid TOML";
        fs::write(&config, saved).unwrap();
        let repeated = command.output().unwrap();
        assert_success(&repeated);
        assert_eq!(fs::read_to_string(&config).unwrap(), saved);
        assert!(!String::from_utf8_lossy(&repeated.stdout).contains("secret-key"));
    }
    requests.assert();
}

#[test]
fn saved_openai_configuration_works_from_any_working_directory() {
    let workspace = Workspace::new();
    let mut server = Server::new();
    workspace.write_config(&format!(
        r#"
api_type = "openai"
language = "zh"
[openai]
base_url = "{}/gateway/v1/"
model = "file-model"
api_key = "file-key"
[anthropic]
base_url = "http://127.0.0.1:9/unused"
model = "other-model"
api_key = "other-key"
"#,
        server.url()
    ));
    fs::create_dir(workspace.path("other")).unwrap();
    // 项目及父目录中的 dotenv / 同名配置不应改变用户配置。
    fs::write(workspace.path(".env"), "OPENAI_API_KEY=wrong-key\n").unwrap();
    for directory in ["work", "other"] {
        fs::write(
            workspace.path(&format!("{directory}/.env")),
            "OPENAI_MODEL=wrong-model\n",
        )
        .unwrap();
        fs::write(
            workspace.path(&format!("{directory}/config.toml")),
            "invalid project config",
        )
        .unwrap();
    }
    let response = server
        .mock("POST", "/gateway/v1/chat/completions")
        .match_header("authorization", "Bearer file-key")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"model": "file-model"}),
        ))
        .with_body(openai_stream("查看状态：git status"))
        .expect(2)
        .create();
    for directory in ["work", "other"] {
        let output = workspace
            .command()
            .current_dir(workspace.path(directory))
            .arg("git")
            .output()
            .unwrap();
        assert_success(&output);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "查看状态：git status\n"
        );
    }
    response.assert();
}

#[test]
fn protocol_selection_is_explicit_and_does_not_send_anthropic_to_openai() {
    let mut server = Server::new();
    let requests = server
        .mock("POST", "/openai/chat/completions")
        .expect(0)
        .create();
    for (file_type, override_type) in [("anthropic", None), ("openai", Some("anthropic"))] {
        let workspace = Workspace::new();
        workspace.write_config(&format!(
            r#"
api_type = "{file_type}"
[openai]
base_url = "{url}/openai"
model = "claude-via-compatible-gateway"
api_key = "openai-secret"
[anthropic]
base_url = "{url}/anthropic"
model = "native-model"
api_key = "anthropic-secret"
"#,
            url = server.url()
        ));
        let response = server.mock("POST", "/anthropic/messages")
            .match_header("authorization", "Bearer anthropic-secret")
            .match_header("anthropic-version", "2026-10-21")
            .match_body(Matcher::PartialJson(serde_json::json!({"model":"native-model", "stream":true})))
            .with_header("content-type", "application/json")
            .with_body(r#"{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"git status"}]}"#)
            .expect(1).create();
        let mut command = workspace.command();
        command.arg("git");
        if let Some(value) = override_type {
            command.env("RUASK_API_TYPE", value);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "ruask: 服务返回完整 JSON 回答，实际使用非流式响应\n"
        );
        assert_eq!(output.stdout, b"git status\n");
        response.assert();
    }
    requests.assert();
}

#[test]
fn environment_overrides_each_selected_field_and_protocol() {
    for file_type in [None, Some("anthropic")] {
        for overrides in [
            [].as_slice(),
            ["BASE_URL"].as_slice(),
            &["API_KEY"],
            &["MODEL"],
            &["BASE_URL", "API_KEY", "MODEL"],
        ] {
            let workspace = Workspace::new();
            let mut file_server = Server::new();
            let mut env_server = Server::new();
            let type_setting = file_type
                .map(|value| format!("api_type = \"{value}\"\n"))
                .unwrap_or_default();
            workspace.write_config(&format!(
                r#"{type_setting}
[openai]
base_url = "{}/file/v1"
api_key = "file-key"
model = "file-model"
[anthropic]
base_url = "http://127.0.0.1:9/anthropic"
api_key = "other-key"
model = "other-model"
"#,
                file_server.url()
            ));
            let (server, path) = if overrides.contains(&"BASE_URL") {
                (&mut env_server, "/override/chat/completions")
            } else {
                (&mut file_server, "/file/v1/chat/completions")
            };
            let expected_key = if overrides.contains(&"API_KEY") {
                "env-key"
            } else {
                "file-key"
            };
            let expected_model = if overrides.contains(&"MODEL") {
                "env-model"
            } else {
                "file-model"
            };
            let response = server
                .mock("POST", path)
                .match_header("authorization", format!("Bearer {expected_key}").as_str())
                .match_body(Matcher::PartialJson(
                    serde_json::json!({"model": expected_model}),
                ))
                .with_body(openai_stream("git status"))
                .expect(1)
                .create();
            let mut command = workspace.command();
            command
                .arg("git")
                .env("ANTHROPIC_BASE_URL", "http://127.0.0.1:9/unused")
                .env("ANTHROPIC_API_KEY", "unselected-secret")
                .env("ANTHROPIC_MODEL", "unselected-model");
            if file_type.is_some() {
                command.env("RUASK_API_TYPE", "openai");
            }
            for name in overrides {
                let value = match *name {
                    "BASE_URL" => format!("{}/override///", env_server.url()),
                    "API_KEY" => "env-key".into(),
                    "MODEL" => "env-model".into(),
                    _ => unreachable!(),
                };
                command.env(format!("OPENAI_{name}"), value);
            }
            let output = command.output().unwrap();
            assert_success(&output);
            assert_eq!(String::from_utf8_lossy(&output.stdout), "git status\n");
            response.assert();
        }
    }
}

#[test]
fn selected_protocol_requires_its_own_complete_nonblank_configuration() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for (api_type, prefix, other) in [
        ("openai", "OPENAI", "ANTHROPIC"),
        ("anthropic", "ANTHROPIC", "OPENAI"),
    ] {
        for missing in ["base_url", "api_key", "model"] {
            for blank_override in [None, Some(""), Some(" \t ")] {
                let workspace = Workspace::new();
                let mut fields = format!("api_type = \"{api_type}\"\n[{api_type}]\n");
                for (field, value) in [
                    ("base_url", format!("{}/v1", server.url())),
                    ("api_key", "selected-secret".into()),
                    ("model", "selected-model".into()),
                ] {
                    if field != missing || blank_override.is_some() {
                        fields.push_str(&format!("{field} = \"{value}\"\n"));
                    }
                }
                fields.push_str(&format!("[{}]\nbase_url = \"{}/unused\"\napi_key = \"other-file-secret\"\nmodel = \"other-file-model\"\n", other.to_lowercase(), server.url()));
                workspace.write_config(&fields);
                let mut command = workspace.command();
                command
                    .arg("git")
                    .env(
                        format!("{other}_BASE_URL"),
                        format!("{}/unused", server.url()),
                    )
                    .env(format!("{other}_API_KEY"), "other-secret")
                    .env(format!("{other}_MODEL"), "other-model");
                let name = format!("{prefix}_{}", missing.to_uppercase());
                if let Some(value) = blank_override {
                    command.env(&name, value);
                }
                let output = command.output().unwrap();
                assert!(!output.status.success(), "{output:?}");
                assert!(output.stdout.is_empty());
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(
                    error.contains(&name) && error.contains("config.toml"),
                    "{error}"
                );
                assert!(!error.contains("secret"), "{error}");
            }
        }
    }
    requests.assert();
}

#[test]
fn malformed_configuration_and_invalid_protocol_errors_never_echo_secrets() {
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for (contents, api_type, expected) in [
        ("[openai]\napi_key = \"sensitive-secret\" BAD", None, "TOML"),
        (
            "[openai]\napi_key = { sensitive-secret = 123 }",
            None,
            "TOML",
        ),
        ("api_type = \"sensitive-secret\"", None, "api_type"),
        ("", Some("sensitive-secret"), "RUASK_API_TYPE"),
        ("", Some(""), "RUASK_API_TYPE"),
    ] {
        let workspace = Workspace::new();
        workspace.write_config(contents);
        let mut command = workspace.command();
        command
            .arg("git")
            .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
            .env("OPENAI_API_KEY", "env-secret")
            .env("OPENAI_MODEL", "env-model");
        if let Some(value) = api_type {
            command.env("RUASK_API_TYPE", value);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("secret"), "{error}");
    }
    requests.assert();
}

#[test]
fn a_directory_at_the_config_file_path_is_reported_as_an_error() {
    let workspace = Workspace::new();
    let config = workspace.path("settings/ruask/config.toml");
    fs::create_dir_all(&config).unwrap();
    for args in ["--init-config", "git"] {
        let output = workspace.command().arg(args).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(config.to_str().unwrap()));
        assert!(config.is_dir());
    }
}

#[test]
fn environment_only_configuration_does_not_create_user_files_or_infer_protocol() {
    let workspace = Workspace::new();
    let mut server = Server::new();
    let response = server
        .mock("POST", "/compatible/chat/completions")
        .match_header("authorization", "Bearer env-key")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"model": "claude-compatible-model"}),
        ))
        .with_body(openai_stream("git status"))
        .expect(1)
        .create();
    let output = workspace
        .command()
        .arg("git")
        .env("OPENAI_BASE_URL", format!("{}/compatible", server.url()))
        .env("OPENAI_API_KEY", "env-key")
        .env("OPENAI_MODEL", "claude-compatible-model")
        .env("ANTHROPIC_API_KEY", "unselected-key")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(!workspace.path("settings").exists());
    assert!(!workspace.path("home/.config").exists());
    response.assert();
}

#[test]
fn help_and_input_errors_do_not_read_even_a_malformed_user_config() {
    let workspace = Workspace::new();
    workspace.write_config("this is not valid TOML and contains a saved-secret");
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    for (args, success, expected) in [
        (vec![], true, "用法"),
        (vec!["--help"], true, "用法"),
        (vec!["git", "--unknown"], false, "未知选项"),
        (vec!["-q"], false, "需要一个字符串值"),
        (vec![" "], false, "不能为空白"),
        (vec!["--init-config", "git"], false, "需单独使用"),
        (vec!["-q", "--init-config"], false, "需要一个字符串值"),
    ] {
        let output = workspace
            .command()
            .args(args)
            .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success, "{output:?}");
        let text = if success {
            &output.stdout
        } else {
            &output.stderr
        };
        assert!(
            String::from_utf8_lossy(text).contains(expected),
            "{output:?}"
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("saved-secret"));
    }
    requests.assert();
}

#[test]
fn configuration_roots_must_be_absolute_and_initialization_needs_a_user_location() {
    let workspace = Workspace::new();
    for args in ["git", "--init-config"] {
        let output = workspace
            .command()
            .arg(args)
            .env("XDG_CONFIG_HOME", "relative-settings")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("XDG_CONFIG_HOME 必须是绝对路径"));
        assert!(!workspace.path("work/relative-settings").exists());
    }
    let output = workspace
        .command()
        .arg("--init-config")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("HOME 或 XDG_CONFIG_HOME"));
}

#[test]
fn help_explains_complete_setup_and_runtime_contract_without_credentials() {
    let workspace = Workspace::new();
    let mut server = Server::new();
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
    let output = workspace
        .command()
        .arg("--help")
        .env("OPENAI_BASE_URL", server.url())
        .env("ANTHROPIC_BASE_URL", server.url())
        .output()
        .unwrap();
    assert_success(&output);
    let help = String::from_utf8(output.stdout).unwrap();
    for contract in [
        "--init-config",
        "保留已有文件",
        "0600",
        "RUASK_API_TYPE → 文件 api_type → openai",
        "所选协议的环境变量逐字段覆盖文件",
        "不借用另一组",
        "/chat/completions",
        "/messages",
        "不自动补 /v1",
        "语言优先级",
        "文件 language",
        "locale",
        "英文",
        "linux / osx / macos",
        "65,536",
        "回退一次",
        "不完整",
        "每次有效查询都在线请求",
        "不执行建议命令",
        "不保留对话历史",
        "macOS / Linux",
        "stderr",
    ] {
        assert!(
            help.contains(contract),
            "missing {contract:?} in help:\n{help}"
        );
    }
    requests.assert();
}

fn configure_initialized_workspace(workspace: &Workspace, openai: &str, anthropic: &str) -> String {
    let output = workspace.command().arg("--init-config").output().unwrap();
    assert_success(&output);
    let path = workspace.path("settings/ruask/config.toml");
    assert!(String::from_utf8_lossy(&output.stdout).contains(path.to_str().unwrap()));
    let mut template: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    template
        .as_table_mut()
        .unwrap()
        .insert("language".into(), toml::Value::String("en".into()));
    for (protocol, base_url, model, key) in [
        ("openai", openai, "saved-openai-model", "saved-openai-key"),
        (
            "anthropic",
            anthropic,
            "saved-native-model",
            "saved-native-key",
        ),
    ] {
        for (field, value) in [("base_url", base_url), ("model", model), ("api_key", key)] {
            template[protocol][field] = toml::Value::String(value.into());
        }
    }
    let saved = toml::to_string(&template).unwrap();
    workspace.write_config(&saved);
    fs::create_dir(workspace.path("other-work")).unwrap();
    saved
}

fn protocol_query(request: &mockito::Request, protocol: &str) -> serde_json::Value {
    if protocol == "openai" {
        request_query(request)
    } else {
        let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
        serde_json::from_str(body["messages"][0]["content"].as_str().unwrap()).unwrap()
    }
}

#[test]
fn initialized_configuration_combines_cross_directory_queries_and_chinese_pipe_analysis() {
    use std::io::Write;

    let workspace = Workspace::new();
    let mut openai = Server::new();
    let mut native = Server::new();
    let saved = configure_initialized_workspace(
        &workspace,
        &format!("{}/team/v1/", openai.url()),
        &format!("{}/native/v1/", native.url()),
    );
    let command = openai
        .mock("POST", "/team/v1/chat/completions")
        .match_header("authorization", "Bearer saved-openai-key")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"model":"saved-openai-model", "stream":true}),
        ))
        .match_request(|request| {
            let query = request_query(request);
            query["command_query"] == "git status"
                && query["question"].is_null()
                && query["output_context"].is_null()
                && query["language"] == "en"
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("Inspect changes:\n  git status"))
        .expect(1)
        .create();
    let output = workspace
        .command()
        .args(["git", "status"])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(output.stdout, b"Inspect changes:\n  git status\n");

    let question = native
        .mock("POST", "/native/v1/messages")
        .match_header("authorization", "Bearer saved-native-key")
        .match_body(Matcher::PartialJson(serde_json::json!({"model":"saved-native-model", "stream":true})))
        .match_request(|request| {
            let query = protocol_query(request, "anthropic");
            query["command_query"].is_null()
                && query["question"] == "How can I inspect changes?"
                && query["output_context"].is_null()
                && query["language"] == "en"
                && query["platform"] == "osx"
        })
        .with_header("content-type", "application/json")
        .with_body(serde_json::json!({"type":"message", "role":"assistant", "content":[{"type":"text", "text":"Inspect changes:\n  git diff"}], "stop_reason":"end_turn"}).to_string())
        .expect(1)
        .create();
    let output = workspace
        .command()
        .current_dir(workspace.path("other-work"))
        .env("RUASK_API_TYPE", "anthropic")
        .args([
            "--question",
            "How can I inspect changes?",
            "-L",
            "en",
            "-p",
            "macos",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"Inspect changes:\n  git diff\n");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "ruask: 服务返回完整 JSON 回答，实际使用非流式响应\n"
    );

    let interpretation = openai
        .mock("POST", "/team/v1/chat/completions")
        .match_header("authorization", "Bearer saved-openai-key")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"model":"saved-openai-model", "stream":true}),
        ))
        .match_request(|request| {
            let query = request_query(request);
            query["command_query"] == "cargo build"
                && query["question"] == "为什么失败，下一步如何排查？"
                && query["output_context"] == "  error: 缺少依赖 🦀\n\n  at Cargo.toml\n"
                && query["language"] == "zh"
                && query["platform"] == "linux"
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("检查依赖配置：\n  cargo check"))
        .expect(1)
        .create();
    let mut child = workspace
        .command()
        .current_dir(workspace.path("other-work"))
        .args([
            "cargo",
            "build",
            "-q",
            "为什么失败，下一步如何排查？",
            "-L",
            "zh",
            "-p",
            "linux",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all("  error: 缺少依赖 🦀\n\n  at Cargo.toml\n".as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_success(&output);
    assert_eq!(output.stdout, "检查依赖配置：\n  cargo check\n".as_bytes());
    assert_eq!(
        fs::read_to_string(workspace.path("settings/ruask/config.toml")).unwrap(),
        saved
    );
    command.assert();
    question.assert();
    interpretation.assert();
}

#[test]
fn repeated_configured_queries_request_again_and_keep_fallback_notices_out_of_redirected_answers() {
    for protocol in ["openai", "anthropic"] {
        let workspace = Workspace::new();
        let mut server = Server::new();
        let saved = configure_initialized_workspace(
            &workspace,
            &format!("{}/openai/v1", server.url()),
            &format!("{}/native/v1", server.url()),
        );
        let (path, key, model) = if protocol == "openai" {
            (
                "/openai/v1/chat/completions",
                "Bearer saved-openai-key",
                "saved-openai-model",
            )
        } else {
            (
                "/native/v1/messages",
                "Bearer saved-native-key",
                "saved-native-model",
            )
        };
        let streaming = server.mock("POST", path)
            .match_header("authorization", key)
            .match_body(Matcher::PartialJson(serde_json::json!({"model":model, "stream":true})))
            .match_request(move |request| {
                let query = protocol_query(request, protocol);
                query["command_query"] == "git status" && query["question"].is_null() && query["language"] == "en"
            })
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error":{"code":"unsupported_streaming","message":"streaming is not supported"}}"#)
            .expect(2).create();
        let content = "Show changes:\n```sh\n  git status\n```";
        let answer = if protocol == "openai" {
            serde_json::json!({"choices":[{"message":{"content":content},"finish_reason":"stop"}]})
        } else {
            serde_json::json!({"type":"message","role":"assistant","content":[{"type":"text","text":content}],"stop_reason":"end_turn"})
        };
        let fallback = server
            .mock("POST", path)
            .match_header("authorization", key)
            .match_body(Matcher::PartialJson(
                serde_json::json!({"model":model,"stream":false}),
            ))
            .match_request(move |request| {
                protocol_query(request, protocol)["command_query"] == "git status"
            })
            .with_header("content-type", "application/json")
            .with_body(answer.to_string())
            .expect(2)
            .create();
        for run in 0..2 {
            let answer_path = workspace.path(&format!("answer-{run}.txt"));
            let output = workspace
                .command()
                .current_dir(workspace.path("other-work"))
                .env("RUASK_API_TYPE", protocol)
                .args(["git", "status"])
                .stdout(fs::File::create(&answer_path).unwrap())
                .output()
                .unwrap();
            assert!(output.status.success(), "{protocol}: {output:?}");
            assert!(output.stdout.is_empty());
            assert_eq!(
                fs::read(&answer_path).unwrap(),
                b"Show changes:\n  git status\n"
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                "ruask: 服务明确不支持流式，回退一次非流式请求\n"
            );
        }
        streaming.assert();
        fallback.assert();
        assert_eq!(
            fs::read_to_string(workspace.path("settings/ruask/config.toml")).unwrap(),
            saved
        );
    }
}
