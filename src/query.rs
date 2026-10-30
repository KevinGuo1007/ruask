use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    Zh,
}

impl Language {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "en" => Ok(Self::En),
            "zh" => Ok(Self::Zh),
            _ => Err("结果语言仅支持 en 或 zh（简体中文）"),
        }
    }

    pub fn from_locale() -> Self {
        let Some(lang) = std::env::var_os("LANG") else {
            return Self::En;
        };
        let language = std::env::var("LANGUAGE").unwrap_or_default();
        language
            .split(':')
            .chain(std::iter::once(lang.to_str().unwrap_or_default()))
            .find_map(|candidate| {
                let base = candidate.split(['.', '@', '_', '-']).next()?;
                Self::parse(&base.to_ascii_lowercase()).ok()
            })
            .unwrap_or(Self::En)
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Linux,
    Osx,
}

impl Platform {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "linux" => Ok(Self::Linux),
            "osx" | "macos" => Ok(Self::Osx),
            _ => Err("目标平台仅支持 linux、osx 或 macos"),
        }
    }

    pub fn host() -> Result<Self, &'static str> {
        Self::parse(std::env::consts::OS)
    }
}

// 分开传递命令查询与附带问题，避免把 -q 的语义变成普通位置参数。
#[derive(Serialize)]
pub struct Query {
    pub command_query: Option<String>,
    pub question: Option<String>,
}
