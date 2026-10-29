use std::env;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde::Deserialize;

const TEMPLATE: &str = r#"api_type = "openai" # openai / anthropic
# language = "zh"  # 保存语言偏好，当前版本暂不应用

[openai]
base_url = "https://your-service.example/v1"
model = "your-model"
api_key = "YOUR_API_KEY"

[anthropic]
base_url = "https://your-anthropic-service.example/v1"
model = "your-model"
api_key = "YOUR_API_KEY"
"#;

fn config_path() -> Result<Option<PathBuf>, Box<dyn Error>> {
    let root = if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(xdg);
        if !path.is_absolute() {
            return Err("XDG_CONFIG_HOME 必须是绝对路径".into());
        }
        path
    } else if let Some(home) = env::var_os("HOME").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(home);
        if !path.is_absolute() {
            return Err("HOME 必须是绝对路径".into());
        }
        path.join(".config")
    } else {
        return Ok(None);
    };
    Ok(Some(root.join("ruask/config.toml")))
}

pub fn initialize() -> Result<String, Box<dyn Error>> {
    let path = config_path()?.ok_or("请设置 HOME 或 XDG_CONFIG_HOME 以定位用户配置")?;
    fs::create_dir_all(path.parent().ok_or("无法定位配置目录")?)
        .map_err(|_| format!("无法创建配置目录：{}", path.display()))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if path.is_dir() {
                return Err(format!("配置文件路径是目录：{}", path.display()).into());
            }
            return Ok(format!("保留已有配置：{}", path.display()));
        }
        Err(_) => return Err(format!("无法创建配置文件：{}", path.display()).into()),
    };
    file.write_all(TEMPLATE.as_bytes())
        .map_err(|_| format!("无法写入配置模板：{}", path.display()))?;
    Ok(format!("已创建配置模板：{}", path.display()))
}

pub struct Config {
    pub api_type: ApiType,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub enum ApiType {
    Openai,
    Anthropic,
}

#[derive(Default, Deserialize)]
struct FileConfig {
    api_type: Option<String>,
    #[serde(default)]
    openai: ProviderConfig,
    #[serde(default)]
    anthropic: ProviderConfig,
}

#[derive(Default, Deserialize)]
struct ProviderConfig {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

pub fn load() -> Result<Config, Box<dyn Error>> {
    let path = config_path()?;
    let file: FileConfig = match &path {
        Some(path) => match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|_: toml::de::Error| {
                format!(
                    "无法解析 TOML 配置：{}；请检查语法和字段类型",
                    path.display()
                )
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileConfig::default(),
            Err(_) => return Err(format!("无法读取用户配置：{}", path.display()).into()),
        },
        None => FileConfig::default(),
    };
    let selected = match env::var("RUASK_API_TYPE") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => file.api_type.unwrap_or_else(|| "openai".into()),
        Err(_) => return Err("RUASK_API_TYPE 必须为 openai 或 anthropic".into()),
    };
    let (api_type, provider, names) = match selected.as_str() {
        "openai" => (
            ApiType::Openai,
            file.openai,
            ["OPENAI_BASE_URL", "OPENAI_API_KEY", "OPENAI_MODEL"],
        ),
        "anthropic" => (
            ApiType::Anthropic,
            file.anthropic,
            ["ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY", "ANTHROPIC_MODEL"],
        ),
        _ => return Err("RUASK_API_TYPE 或配置 api_type 必须为 openai 或 anthropic".into()),
    };
    Ok(Config {
        api_type,
        base_url: setting(names[0], provider.base_url, path.as_deref())?,
        api_key: setting(names[1], provider.api_key, path.as_deref())?,
        model: setting(names[2], provider.model, path.as_deref())?,
    })
}

fn setting(
    name: &str,
    saved: Option<String>,
    path: Option<&std::path::Path>,
) -> Result<String, Box<dyn Error>> {
    let value = match env::var(name) {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent) => saved,
        Err(_) => return Err(format!("环境变量 {name} 必须是有效 Unicode 文本").into()),
    };
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => {
            let location = path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "用户配置（ruask --init-config）".into());
            Err(format!("请设置 {name} 或 {location} 中对应的非空字段").into())
        }
    }
}
