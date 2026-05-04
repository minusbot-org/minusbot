use crate::openai::*;
use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use std::sync::Arc;

pub struct OpenAiProvider {
    api_key: Option<String>,
    client: OpenAiClient,
    config: Option<Arc<dyn ConfigProvider>>,
}

impl OpenAiProvider {
    pub fn new(
        api_key: Option<String>,
        base_url: Option<String>,
        config_path: Option<std::path::PathBuf>,
    ) -> Self {
        let id = "provider-openai";
        let config = config_path
            .map(|p| Arc::new(FileConfigProvider::new(id, p)) as Arc<dyn ConfigProvider>);

        Self {
            api_key,
            client: OpenAiClient::new(
                base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            ),
            config,
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn name(&self) -> &'static str {
        "OpenAI"
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
impl TextProvider for OpenAiProvider {
    async fn generate_text(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let messages: Vec<OpenAiMessage> = request
            .messages
            .iter()
            .map(|m| {
                let mut msg = OpenAiMessage {
                    role: m.role.to_string(),
                    content: m.content.clone(),
                    tool_calls: None,
                    tool_call_id: m.tool_call_id.clone(),
                    name: m.name.clone(),
                };
                if let Some(tc) = &m.tool_calls {
                    if !tc.is_empty() {
                        msg.tool_calls = Some(
                            tc.iter()
                                .map(|c| OpenAiToolCall {
                                    id: c.id.clone(),
                                    r#type: "function".into(),
                                    function: OpenAiFunctionCall {
                                        name: c.name.clone(),
                                        arguments: c.arguments.to_string(),
                                    },
                                })
                                .collect(),
                        );
                    }
                }
                msg
            })
            .collect();

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|t| OpenAiTool {
                        r#type: "function".into(),
                        function: OpenAiToolFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect(),
            )
        };

        let body = OpenAiChatRequest {
            model: request.options.model.clone(),
            messages,
            tools,
            temperature: Some(request.options.temperature),
            max_tokens: request.options.max_tokens,
            top_p: Some(request.options.top_p),
        };

        let api_key = &request.options.api_key;
        if api_key.is_empty() {
            anyhow::bail!(
                "API key missing for {}. Set PROVIDER_{}_API_KEY.",
                self.name(),
                self.id().to_uppercase()
            );
        }

        let (api_resp, raw) = self
            .client
            .create_chat_completion(api_key, request.options.endpoint.as_deref(), &body)
            .await?;
        let choice = api_resp
            .choices
            .first()
            .context("No choices in OpenAI response")?;

        let content = choice.message.content.clone();
        let tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                        metadata: None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = api_resp.usage.map(|u| ProviderUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(ProviderResponse {
            content,
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
