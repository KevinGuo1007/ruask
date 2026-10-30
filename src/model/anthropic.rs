use std::error::Error;
use std::io::BufReader;
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::Config;
use crate::output::Lines;

use super::sse;

pub(super) fn ask(
    config: &Config,
    instructions: &str,
    input: &str,
    output: &mut Lines<impl std::io::Write>,
) -> Result<(), Box<dyn Error>> {
    let url = format!("{}/messages", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.model,
        "max_tokens": 4096,
        "system": instructions,
        "messages": [{"role":"user", "content":input}],
        "stream": true,
    });
    let mut request = ureq::post(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("anthropic-version", "2026-10-21");
    if let Some(workspace) = std::env::var_os("ANTHROPIC_WORKSPACE_ID") {
        let workspace = workspace
            .into_string()
            .map_err(|_| "ANTHROPIC_WORKSPACE_ID 必须是有效 Unicode 文本")?;
        if workspace.trim().is_empty() {
            return Err("ANTHROPIC_WORKSPACE_ID 不能为空白；不需要时请取消设置".into());
        }
        request = request.header("anthropic-workspace-id", workspace);
    }
    let mut response = request
        .send_json(&body)
        .map_err(|error| format!("模型请求失败：{error}"))?;
    if response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(';').next().unwrap_or_default().trim() == "application/json"
        })
    {
        let message: Value = response
            .body_mut()
            .read_json()
            .map_err(|_| "无法读取 Anthropic JSON 回答")?;
        return write_message(&message, output);
    }
    let mut reader = BufReader::new(response.body_mut().as_reader());
    let mut started = false;
    let mut block: Option<(u64, bool)> = None;
    let mut stopped = false;
    loop {
        let event = sse::read_event(&mut reader)?;
        let chunk: Value = serde_json::from_str(&event.data)
            .map_err(|_| "模型响应不完整：无效 Anthropic 流式事件")?;
        if event.name != chunk["type"].as_str().unwrap_or_default() {
            return Err("模型响应不完整：Anthropic 事件名称与类型不一致".into());
        }
        match event.name.as_str() {
            "error" => return Err("模型响应不完整：Anthropic 流内错误".into()),
            "message_start" => {
                if started || chunk["message"]["role"] != "assistant" {
                    return Err("模型响应不完整：无效 Anthropic 消息开始".into());
                }
                started = true;
            }
            "content_block_start" => {
                if !started || stopped || block.is_some() {
                    return Err("模型响应不完整：无效 Anthropic 内容块开始".into());
                }
                let index = chunk["index"]
                    .as_u64()
                    .ok_or("模型响应不完整：内容块缺少索引")?;
                let is_text = chunk["content_block"]["type"] == "text";
                block = Some((index, is_text));
                if is_text {
                    output.push(
                        chunk["content_block"]["text"]
                            .as_str()
                            .ok_or("模型响应不完整：内容块缺少文本")?,
                    )?;
                }
            }
            "content_block_delta" => {
                let (index, is_text) = block.ok_or("模型响应不完整：缺少 Anthropic 内容块开始")?;
                if chunk["index"].as_u64() != Some(index) {
                    return Err("模型响应不完整：Anthropic 内容块索引不一致".into());
                }
                if is_text && chunk["delta"]["type"] == "text_delta" {
                    output.push(
                        chunk["delta"]["text"]
                            .as_str()
                            .ok_or("模型响应不完整：正文增量缺少文本")?,
                    )?;
                }
            }
            "content_block_stop" => {
                if block.map(|(index, _)| index) != chunk["index"].as_u64() || block.is_none() {
                    return Err("模型响应不完整：无效 Anthropic 内容块结束".into());
                }
                block = None;
            }
            "message_delta" => {
                if !started || block.is_some() {
                    return Err("模型响应不完整：Anthropic 内容块未结束".into());
                }
                if !chunk["delta"]["stop_reason"].is_null() {
                    if !normal_stop(&chunk["delta"]["stop_reason"]) {
                        return Err(
                            "模型响应不完整：Anthropic 未正常完成（截断或非最终回答）".into()
                        );
                    }
                    stopped = true;
                }
            }
            "message_stop" => {
                if !started || !stopped || block.is_some() {
                    return Err("模型响应不完整：Anthropic 缺少正常完成原因".into());
                }
                return output.finish().map_err(Into::into);
            }
            _ => {}
        }
    }
}

fn write_message(
    message: &Value,
    output: &mut Lines<impl std::io::Write>,
) -> Result<(), Box<dyn Error>> {
    if message["type"] != "message" || message["role"] != "assistant" {
        return Err("无效 Anthropic JSON 回答".into());
    }
    for block in message["content"]
        .as_array()
        .ok_or("Anthropic 回答缺少内容块")?
    {
        if block["type"] == "text" {
            output.push(block["text"].as_str().ok_or("Anthropic 文本块缺少文本")?)?;
        }
    }
    if !normal_stop(&message["stop_reason"]) {
        return Err("模型响应不完整：Anthropic 未正常完成".into());
    }
    output.finish().map_err(Into::into)
}

fn normal_stop(reason: &Value) -> bool {
    matches!(reason.as_str(), Some("end_turn" | "stop_sequence"))
}
