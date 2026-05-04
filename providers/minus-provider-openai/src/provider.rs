use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct OpenAiProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
    config: Option<Arc<dyn ConfigProvider>>,
}

impl OpenAiProvider {
    pub fn new(api_key: Option<String>, base_url: Option<String>, config_path: Option<std::path::PathBuf>) -> Self {
        let id = "provider-openai";
        let config = config_path.map(|p| Arc::new(FileConfigProvider::new(id, p)) as Arc<dyn ConfigProvider>);
        
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
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
        let messages: Vec<ApiMessage> = request.messages.iter().map(|m| {
            let mut msg = ApiMessage {
                role: m.role.to_string(),
                content: m.content.clone(),
                tool_calls: None,
                tool_call_id: m.tool_call_id.clone(),
                name: m.name.clone(),
            };
            if let Some(tc) = &m.tool_calls {
                if !tc.is_empty() {
                    msg.tool_calls = Some(tc.iter().map(|c| ApiToolCall {
                        id: c.id.clone(),
                        r#type: "function".into(),
                        function: ApiFunctionCall {
                            name: c.name.clone(),
                            arguments: c.arguments.to_string(),
                        },
                    }).collect());
                }
            }
            msg
        }).collect();

        let tools: Option<Vec<ApiTool>> = if request.tools.is_empty() {
            None
        } else {
            Some(request.tools.iter().map(|t| ApiTool {
                r#type: "function".into(),
                function: ApiToolFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            }).collect())
        };

        let body = ApiRequest {
            model: request.options.model.clone(),
            messages,
            tools,
            temperature: Some(request.options.temperature),
            max_tokens: request.options.max_tokens,
            top_p: Some(request.options.top_p),
        };

        let base_url = request.options.endpoint.as_deref().unwrap_or(&self.base_url);
        let url = format!("{}/chat/completions", base_url);
        tracing::debug!(url = %url, model = %request.options.model, "OpenAI API request");

        let api_key = &request.options.api_key;
        if api_key.is_empty() {
            anyhow::bail!("API key missing for {}. Set PROVIDER_{}_API_KEY.", self.name(), self.id().to_uppercase());
        }

        let resp = self.client.post(&url).bearer_auth(api_key).json(&body).send().await
            .context("Failed to send request to OpenAI API")?;

        let status = resp.status();
        let raw_text = resp.text().await.context("Failed to read response body")?;
        tracing::debug!(status = %status, "OpenAI API response");

        if !status.is_success() {
            anyhow::bail!("OpenAI API error ({}): {}", status, raw_text);
        }

        let raw: serde_json::Value = serde_json::from_str(&raw_text).context("Failed to parse OpenAI response JSON")?;
        let api_resp: ApiResponse = serde_json::from_value(raw.clone()).context("Failed to deserialize OpenAI response")?;
        let choice = api_resp.choices.first().context("No choices in OpenAI response")?;

        let content = choice.message.content.clone();
        let tool_calls = choice.message.tool_calls.as_ref().map(|tcs| {
            tcs.iter().map(|tc| ToolCall {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
            }).collect()
        }).unwrap_or_default();

        let usage = api_resp.usage.map(|u| ProviderUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(ProviderResponse { content, tool_calls, usage, raw: Some(raw) })
    }

    async fn get_text_models(&self, secret_key: Option<String>) -> Result<Vec<String>> {
        let configured_key = self.api_key.as_deref().unwrap_or("");
        let api_key = secret_key.as_deref().unwrap_or(configured_key);
        
        if api_key.is_empty() {
            anyhow::bail!("API key is missing for '{}'. Use '/apikey {} <key>'.", self.name(), self.id());
        }
        let url = format!("{}/models", self.base_url);
        let resp = self.client.get(&url).bearer_auth(api_key).send().await
            .context("Failed to connect to provider API to list models")?;
        
        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error listing models ({}): {}", status, err_text);
        }

        let json: serde_json::Value = resp.json().await?;
        let mut models: Vec<String> = json["data"].as_array()
            .map(|arr| arr.iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        models.sort();
        Ok(models)
    }
}

// --- API types ---

#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiToolCall {
    id: String,
    r#type: String,
    function: ApiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct ApiTool {
    r#type: String,
    function: ApiToolFunction,
}

#[derive(Debug, Serialize)]
struct ApiToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ApiChoice {
    message: ApiResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ApiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}
