use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
    config: Option<Arc<dyn ConfigProvider>>,
}

impl AnthropicProvider {
    pub fn new(
        api_key: Option<String>,
        base_url: Option<String>,
        config_path: Option<std::path::PathBuf>,
    ) -> Self {
        let id = "provider-anthropic";
        let config = config_path
            .map(|p| Arc::new(FileConfigProvider::new(id, p)) as Arc<dyn ConfigProvider>);

        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            config,
        }
    }

    fn build_messages(
        &self,
        request: &ProviderRequest,
    ) -> Result<(Option<String>, Vec<AnthropicMessage>)> {
        let mut system_parts = Vec::new();
        let mut messages = Vec::new();
        let mut pending_tool_results: Vec<AnthropicContentBlock> = Vec::new();

        for msg in &request.messages {
            match msg.role {
                Role::System => {
                    if let Some(content) = &msg.content {
                        if !content.is_empty() {
                            system_parts.push(content.clone());
                        }
                    }
                }
                Role::User => {
                    flush_tool_results(&mut messages, &mut pending_tool_results);
                    messages.push(AnthropicMessage::text("user", msg.content.clone()));
                }
                Role::Assistant => {
                    flush_tool_results(&mut messages, &mut pending_tool_results);
                    let mut content = Vec::new();
                    if let Some(text) = &msg.content {
                        if !text.is_empty() {
                            content.push(AnthropicContentBlock::Text { text: text.clone() });
                        }
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tool_call in tool_calls {
                            content.push(AnthropicContentBlock::ToolUse {
                                id: tool_call.id.clone(),
                                name: tool_call.name.clone(),
                                input: tool_call.arguments.clone(),
                            });
                        }
                    }
                    if content.is_empty() {
                        content.push(AnthropicContentBlock::Text {
                            text: String::new(),
                        });
                    }
                    messages.push(AnthropicMessage {
                        role: "assistant",
                        content,
                    });
                }
                Role::Tool => {
                    let tool_use_id = msg
                        .tool_call_id
                        .clone()
                        .context("Anthropic tool result is missing tool_call_id")?;
                    pending_tool_results.push(AnthropicContentBlock::ToolResult {
                        tool_use_id,
                        content: msg.content.clone().unwrap_or_default(),
                        is_error: None,
                    });
                }
            }
        }

        flush_tool_results(&mut messages, &mut pending_tool_results);

        let system = if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n\n"))
        };
        Ok((system, messages))
    }
}

fn flush_tool_results(
    messages: &mut Vec<AnthropicMessage>,
    pending_tool_results: &mut Vec<AnthropicContentBlock>,
) {
    if pending_tool_results.is_empty() {
        return;
    }

    messages.push(AnthropicMessage {
        role: "user",
        content: std::mem::take(pending_tool_results),
    });
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn name(&self) -> &'static str {
        "Anthropic"
    }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        self.config.clone()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            llm: true,
            tools: true,
            vision: false,
            embeddings: false,
        }
    }

    fn as_text_provider(&self) -> Option<&dyn TextProvider> {
        Some(self)
    }

    async fn is_ready(&self) -> bool {
        if let Some(cp) = &self.config {
            if let Ok(Some(_)) = cp.read_config("text_model").await {
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl TextProvider for AnthropicProvider {
    async fn generate_text(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let (system, messages) = self.build_messages(&request)?;

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|tool| AnthropicTool {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        input_schema: tool.input_schema.clone(),
                    })
                    .collect(),
            )
        };

        let body = AnthropicRequest {
            model: request.options.model.clone(),
            max_tokens: request.options.max_tokens.unwrap_or(1024),
            messages,
            system,
            tools,
            temperature: Some(request.options.temperature),
            top_p: Some(request.options.top_p),
        };

        let base_url = request
            .options
            .endpoint
            .as_deref()
            .unwrap_or(&self.base_url);
        let url = format!("{}/messages", base_url.trim_end_matches('/'));
        tracing::debug!(url = %url, model = %request.options.model, "Anthropic API request");

        if request.options.api_key.is_empty() {
            anyhow::bail!(
                "API key missing for {}. Set PROVIDER_{}_API_KEY.",
                self.name(),
                self.id().to_uppercase()
            );
        }

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &request.options.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        let status = resp.status();
        let raw_text = resp.text().await.context("Failed to read response body")?;
        tracing::debug!(status = %status, "Anthropic API response");

        if !status.is_success() {
            anyhow::bail!("Anthropic API error ({}): {}", status, raw_text);
        }

        let raw: serde_json::Value =
            serde_json::from_str(&raw_text).context("Failed to parse Anthropic response JSON")?;
        let api_resp: AnthropicResponse = serde_json::from_value(raw.clone())
            .context("Failed to deserialize Anthropic response")?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in api_resp.content {
            match block {
                AnthropicResponseBlock::Text { text } => text_parts.push(text),
                AnthropicResponseBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                        metadata: None,
                    });
                }
                AnthropicResponseBlock::Other => {}
            }
        }

        let usage = api_resp.usage.map(|usage| {
            let total_tokens = usage.input_tokens + usage.output_tokens;
            ProviderUsage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                total_tokens,
            }
        });

        Ok(ProviderResponse {
            content: if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join(""))
            },
            tool_calls,
            usage,
            raw: Some(raw),
        })
    }

    async fn get_text_models(&self, secret_key: Option<String>) -> Result<Vec<String>> {
        let configured_key = self.api_key.as_deref().unwrap_or("");
        let api_key = secret_key.as_deref().unwrap_or(configured_key);

        if api_key.is_empty() {
            anyhow::bail!(
                "API key is missing for '{}'. Use '/apikey {} <key>'.",
                self.name(),
                self.id()
            );
        }

        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("Failed to connect to Anthropic API to list models")?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Anthropic API error listing models ({}): {}",
                status,
                err_text
            );
        }

        let json: AnthropicModelsResponse = resp.json().await?;
        let mut models: Vec<String> = json.data.into_iter().map(|model| model.id).collect();
        models.sort();
        Ok(models)
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: Vec<AnthropicContentBlock>,
}

impl AnthropicMessage {
    fn text(role: &'static str, content: Option<String>) -> Self {
        Self {
            role,
            content: vec![AnthropicContentBlock::Text {
                text: content.unwrap_or_default(),
            }],
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseBlock>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
}
