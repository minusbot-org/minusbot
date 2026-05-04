use crate::anthropic::*;
use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use std::sync::Arc;

pub struct AnthropicProvider {
    api_key: Option<String>,
    client: AnthropicClient,
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
            client: AnthropicClient::new(base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string())),
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

        if request.options.api_key.is_empty() {
            anyhow::bail!(
                "API key missing for {}. Set PROVIDER_{}_API_KEY.",
                self.name(),
                self.id().to_uppercase()
            );
        }

        let (api_resp, raw) = self
            .client
            .create_message(
                &request.options.api_key,
                request.options.endpoint.as_deref(),
                &body,
            )
            .await?;

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

        self.client.list_models(api_key).await
    }
}
