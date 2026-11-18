//! 本机环境摘要：只在目标平台就是当前主机时提供给模型。
//! 只检查 PATH 中是否存在可执行文件，不执行任何程序，也不读取文件内容。

use std::path::Path;

use serde_json::{Value, json};

const PACKAGE_MANAGERS: [&str; 9] = [
    "apt", "dnf", "yum", "pacman", "zypper", "apk", "brew", "port", "nix",
];

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program)))
    })
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// `command` 是查询中的首个命令词（若像命令名），用于告诉模型它是否已安装。
pub fn detect(command: Option<&str>) -> Value {
    let shell = std::env::var("SHELL")
        .ok()
        .and_then(|s| Path::new(&s).file_name()?.to_str().map(String::from));
    let managers: Vec<&str> = PACKAGE_MANAGERS
        .iter()
        .copied()
        .filter(|m| on_path(m))
        .collect();
    let mut value = json!({ "shell": shell, "package_managers": managers });
    if let Some(command) = command.filter(|c| {
        !c.is_empty()
            && c.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '+'))
    }) {
        value["queried_command_installed"] = json!(on_path(command));
    }
    value
}
