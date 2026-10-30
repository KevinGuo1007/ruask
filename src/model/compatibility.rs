use serde_json::Value;

#[derive(Debug)]
pub(super) struct UnsupportedStreaming {
    pub status: Option<u16>,
}

impl std::fmt::Display for UnsupportedStreaming {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(status) = self.status {
            write!(
                formatter,
                "模型请求失败：HTTP {status}（服务明确不支持流式）"
            )
        } else {
            formatter.write_str("模型响应不完整：流内错误（服务明确不支持流式）")
        }
    }
}

impl std::error::Error for UnsupportedStreaming {}

pub(super) fn stream_error(value: &Value) -> Box<dyn std::error::Error> {
    if stream_unsupported(value) {
        Box::new(UnsupportedStreaming { status: None })
    } else {
        format!("模型响应不完整：流内错误（{}）", failure_kind(value, None)).into()
    }
}

// Gate retries on explicit capability evidence, never on a status code alone.
pub(super) fn stream_unsupported(value: &Value) -> bool {
    let error = value.get("error").unwrap_or(value);
    let code = error["code"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = error["type"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let message = error["message"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if [
        "auth",
        "permission",
        "rate",
        "quota",
        "timeout",
        "overload",
        "billing",
        "api_key",
    ]
    .iter()
    .any(|word| code.contains(word) || kind.contains(word))
        || [
            "api key",
            "authentication",
            "unauthorized",
            "quota",
            "rate limit",
            "timed out",
            "timeout",
            "invalid model",
            "unknown model",
            "model not found",
        ]
        .iter()
        .any(|word| message.contains(word))
    {
        return false;
    }
    let parameter = error["param"].as_str();
    if parameter.is_some_and(|parameter| parameter != "stream" && parameter != "streaming") {
        return false;
    }
    let words: Vec<_> = message
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
        .collect();
    matches!(
        code.as_str(),
        "unsupported_streaming" | "streaming_not_supported" | "stream_not_supported"
    ) || (parameter.is_some() && code == "unsupported_parameter")
        || [
            &["stream", "is", "not", "supported"][..],
            &["streaming", "is", "not", "supported"][..],
            &["stream", "not", "supported"][..],
            &["streaming", "not", "supported"][..],
            &["does", "not", "support", "streaming"][..],
            &["unsupported", "streaming"][..],
            &["streaming", "unsupported"][..],
            &["streaming", "is", "unsupported"][..],
            &["unsupported", "parameter", "stream"][..],
        ]
        .iter()
        .any(|phrase| words.windows(phrase.len()).any(|words| words == *phrase))
}

pub(super) fn failure_kind(value: &Value, status: Option<u16>) -> &'static str {
    let error = value.get("error").unwrap_or(value);
    let kind = format!(
        "{} {}",
        error["type"].as_str().unwrap_or_default(),
        error["code"].as_str().unwrap_or_default()
    );
    if matches!(status, Some(401)) || kind.contains("auth") || kind.contains("invalid_api_key") {
        "鉴权失败"
    } else if matches!(status, Some(403)) || kind.contains("permission") {
        "权限不足"
    } else if matches!(status, Some(429)) || kind.contains("rate_limit") || kind.contains("quota") {
        "限流或额度不足"
    } else if matches!(status, Some(408 | 504)) || kind.contains("timeout") {
        "请求超时"
    } else {
        "服务请求错误"
    }
}
