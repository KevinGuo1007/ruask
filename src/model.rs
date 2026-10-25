use std::env;
use std::error::Error;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

pub struct Config {
    base_url: String,
    api_key: String,
    model: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            base_url: required_env("OPENAI_BASE_URL")?,
            api_key: required_env("OPENAI_API_KEY")?,
            model: required_env("OPENAI_MODEL")?,
        })
    }
}

fn required_env(name: &str) -> Result<String, Box<dyn Error>> {
    let value = env::var(name).map_err(|_| format!("请设置环境变量 {name}"))?;
    if value.trim().is_empty() {
        return Err(format!("环境变量 {name} 不能为空").into());
    }
    Ok(value)
}

// 只描述我们需要读取的字段，服务返回的其他字段由 serde 忽略。
#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

pub fn ask(config: &Config, prompt: &str) -> Result<String, Box<dyn Error>> {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.model,
        "messages": [{"role": "user", "content": prompt}]
    });

    let response: ChatResponse = ureq::post(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .header("Authorization", format!("Bearer {}", config.api_key))
        .send_json(&body)
        .map_err(|error| format!("模型请求失败：{error}"))?
        .body_mut()
        .read_json()
        .map_err(|error| format!("无法读取模型响应：{error}"))?;

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or("模型响应中没有回答")?;
    if choice.message.content.trim().is_empty() {
        return Err("模型响应中没有文本回答".into());
    }
    Ok(choice.message.content)
}
