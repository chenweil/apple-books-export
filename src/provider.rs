//! Apple Books Exporter - LLM Provider (OpenAI Compatible)

use crate::models::LLMConfig;
use anyhow::{Context, Result};
use reqwest;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// LLM 响应
#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_split: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseMessage {
    content: String,
}

/// LLM Provider
pub struct LLMProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    max_retries: u32,
    retry_delays: Vec<u64>,
}

impl LLMProvider {
    /// 创建新的 LLM Provider
    pub fn new(config: &LLMConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap_or_default();

        Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            max_retries: config.max_retries,
            retry_delays: config.retry_delays.clone(),
        }
    }

    /// 发送单次提示，带指数退避重试
    pub async fn complete(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        let mut messages = Vec::new();

        if let Some(system_prompt) = system {
            messages.push(Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            });
        }

        messages.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let payload = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            temperature: 0.3,
            // MiniMax M 系列：把 thinking 拆到 reasoning_content，content 只留最终答案，
            // 否则 content 内嵌 <think> 标签会破坏卡片增强的严格 JSON 解析。
            reasoning_split: Some(true),
        };

        let url = format!("{}/chat/completions", self.base_url);
        let mut last_error = None;
        let delays = self.retry_delays.clone();

        for attempt in 0..self.max_retries {
            let result: Result<String> = async {
                let response = self
                    .client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send()
                    .await
                    .context("发送 LLM 请求失败")?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_default();
                    anyhow::bail!("LLM API 返回错误 {}: {}", status, error_body);
                }

                let resp: ChatCompletionResponse =
                    response.json().await.context("解析 LLM 响应失败")?;
                resp.choices
                    .into_iter()
                    .next()
                    .map(|c| c.message.content)
                    .ok_or_else(|| anyhow::anyhow!("LLM 响应中没有内容"))
            }
            .await;

            match result {
                Ok(content) => return Ok(content),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries - 1 {
                        let delay_idx = (attempt as usize).min(delays.len().saturating_sub(1));
                        let delay = delays.get(delay_idx).copied().unwrap_or(1);
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("未知错误")))
    }
}

/// 解析 LLM 返回的 JSON 结果
pub fn parse_llm_result(content: &str) -> Result<crate::models::LLMResult> {
    // 尝试直接解析 JSON
    if let Ok(result) = serde_json::from_str(content) {
        return Ok(result);
    }

    // 尝试解析 markdown 代码块中的 JSON
    let re = regex::Regex::new(r"```(?:json)?\s*(\{[\s\S]*?\})\s*```").unwrap();
    if let Some(caps) = re.captures(content) {
        if let Some(json_str) = caps.get(1) {
            if let Ok(result) = serde_json::from_str(json_str.as_str()) {
                return Ok(result);
            }
        }
    }

    // 尝试提取 JSON 对象
    let re = regex::Regex::new(r"\{[\s\S]*\}").unwrap();
    if let Some(caps) = re.captures(content) {
        if let Some(json_str) = caps.get(0) {
            if let Ok(result) = serde_json::from_str(json_str.as_str()) {
                return Ok(result);
            }
        }
    }

    Err(anyhow::anyhow!("无法解析 LLM 响应为 JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_llm_result_direct_json() {
        let content = r#"{"explanation": "test", "tags": ["a", "b"], "question": "what?"}"#;
        let result = parse_llm_result(content).unwrap();
        assert_eq!(result.explanation, "test");
        assert_eq!(result.tags, vec!["a", "b"]);
    }

    #[test]
    fn test_parse_llm_result_markdown_block() {
        let content = r#"```json
{"explanation": "test", "tags": ["a"], "question": "what?"}
```"#;
        let result = parse_llm_result(content).unwrap();
        assert_eq!(result.explanation, "test");
    }

    fn request_with(reasoning_split: Option<bool>) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "MiniMax-M3".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            temperature: 0.3,
            reasoning_split,
        }
    }

    /// 回归防线：MiniMax M 系列默认把 thinking 内嵌进 `content`，会带崩卡片增强的
    /// 严格 JSON 解析。请求体必须显式要求拆到 `reasoning_content`。
    #[test]
    fn request_body_asks_minimax_to_split_thinking_out_of_content() {
        let value = serde_json::to_value(request_with(Some(true))).unwrap();
        assert_eq!(value["reasoning_split"], serde_json::json!(true));
    }

    /// 非 MiniMax 供应商不认这个参数，所以关掉时要整字段省略，不能发 `false`。
    #[test]
    fn reasoning_split_is_omitted_when_disabled() {
        let value = serde_json::to_value(request_with(None)).unwrap();
        assert!(value.get("reasoning_split").is_none());
    }

    /// 响应体只有 `content`（不带 `reasoning_content`）时不能解析失败：
    /// 其他供应商以及关掉拆分时的正常形状。
    #[test]
    fn response_without_reasoning_content_still_parses() {
        let raw = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
        let resp: ChatCompletionResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.choices[0].message.content, "ok");
    }
}
