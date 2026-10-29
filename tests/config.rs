use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use mockito::{Matcher, Server};
use tempfile::TempDir;

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
        .with_body(r#"{"choices":[{"message":{"content":"查看状态：git status"}}]}"#)
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
    let requests = server.mock("POST", Matcher::Any).expect(0).create();
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
        let mut command = workspace.command();
        command.arg("git");
        if let Some(value) = override_type {
            command.env("RUASK_API_TYPE", value);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("Anthropic 原生协议尚未接入"), "{error}");
        assert!(!error.contains("secret"), "{error}");
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
                .with_body(r#"{"choices":[{"message":{"content":"git status"}}]}"#)
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
        .with_body(r#"{"choices":[{"message":{"content":"git status"}}]}"#)
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
