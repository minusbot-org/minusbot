use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct GoogleAiProvider {
    api_key: Option<String>,
    client: reqwest::Client,
    config: Option<Arc<dyn ConfigProvider>>,
}

impl GoogleAiProvider {
    pub fn new(api_key: Option<String>, config_path: Option<std::path::PathBuf>) -> Self {
        let id = "provider-google";
        let config = config_path
            .map(|p| Arc::new(FileConfigProvider::new(id, p)) as Arc<dyn ConfigProvider>);

        Self {
            api_key,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            config,
        }
    }
}

#[async_trait]
impl Provider for GoogleAiProvider {
    fn id(&self) -> &'static str {
        "google"
    }

    fn name(&self) -> &'static str {
        "Google AI (Gemini)"
    }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        self.config.clone()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            llm: true,
            tools: true,
            vision: true,
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
impl TextProvider for GoogleAiProvider {
    async fn generate_text(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let mut contents = Vec::new();
        let mut system_instruction = None;

        let mut messages_it = request.messages.iter().peekable();
        while let Some(msg) = messages_it.next() {
            match msg.role {
                Role::System => {
                    system_instruction = Some(GeminiContent {
                        role: None,
                        parts: vec![GeminiPart {
                            text: msg.content.clone(),
                            thought_signature: None,
                            function_call: None,
                            function_response: None,
                        }],
                    });
                }
                Role::User => {
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts: vec![GeminiPart {
                            text: msg.content.clone(),
                            thought_signature: None,
                            function_call: None,
                            function_response: None,
                        }],
                    });
                }
                Role::Assistant => {
                    let mut parts = Vec::new();
                    if let Some(text) = &msg.content {
                        parts.push(GeminiPart {
                            text: Some(text.clone()),
                            thought_signature: None,
                            function_call: None,
                            function_response: None,
                        });
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            parts.push(GeminiPart {
                                text: None,
                                function_call: Some(GeminiFunctionCall {
                                    name: tc.name.clone(),
                                    args: tc.arguments.clone(),
                                }),
                                thought_signature: tc
                                    .metadata
                                    .as_ref()
                                    .and_then(|m| m.get("thought_signature"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                function_response: None,
                            });
                        }
                    }
                    contents.push(GeminiContent {
                        role: Some("model".into()),
                        parts,
                    });
                }
                Role::Tool => {
                    // Group consecutive tool responses into a single content
                    let mut parts = Vec::new();

                    // Add the first one
                    parts.push(GeminiPart {
                        text: None,
                        thought_signature: None,
                        function_call: None,
                        function_response: Some(GeminiFunctionResponse {
                            name: msg.name.clone().unwrap_or_default(),
                            response: serde_json::from_str(
                                &msg.content.clone().unwrap_or_else(|| "{}".to_string()),
                            )
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                        }),
                    });

                    // Peek for more consecutive tool messages
                    while let Some(next_msg) = messages_it.peek() {
                        if next_msg.role == Role::Tool {
                            let next_msg = messages_it.next().unwrap();
                            parts.push(GeminiPart {
                                text: None,
                                thought_signature: None,
                                function_call: None,
                                function_response: Some(GeminiFunctionResponse {
                                    name: next_msg.name.clone().unwrap_or_default(),
                                    response: serde_json::from_str(
                                        &next_msg
                                            .content
                                            .clone()
                                            .unwrap_or_else(|| "{}".to_string()),
                                    )
                                    .unwrap_or(serde_json::Value::Object(Default::default())),
                                }),
                            });
                        } else {
                            break;
                        }
                    }

                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts,
                    });
                }
            }
        }

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(vec![GeminiTool {
                function_declarations: request
                    .tools
                    .iter()
                    .map(|t| GeminiFunctionDeclaration {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    })
                    .collect(),
            }])
        };

        let body = GeminiRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(request.options.temperature),
                max_output_tokens: request.options.max_tokens,
                top_p: Some(request.options.top_p),
                ..Default::default()
            }),
        };

        let api_key = &request.options.api_key;
        if api_key.is_empty() {
            anyhow::bail!("API key missing for Google AI. Set PROVIDER_GOOGLE_API_KEY.");
        }

        let model = if request.options.model.is_empty() {
            "gemini-1.5-flash"
        } else {
            &request.options.model
        };
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model, api_key
        );

        tracing::debug!(url = %url, model = %model, "Google AI API request");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Google AI API")?;

        let status = resp.status();
        let raw_text = resp.text().await.context("Failed to read response body")?;
        tracing::debug!(status = %status, "Google AI API response");

        if !status.is_success() {
            anyhow::bail!("Google AI API error ({}): {}", status, raw_text);
        }

        let raw: serde_json::Value =
            serde_json::from_str(&raw_text).context("Failed to parse Google AI response JSON")?;
        let api_resp: GeminiResponse = serde_json::from_value(raw.clone())
            .context("Failed to deserialize Google AI response")?;

        let candidate = api_resp
            .candidates
            .first()
            .context("No candidates in Google AI response")?;

        let mut content = None;
        let mut tool_calls = Vec::new();

        for part in &candidate.content.parts {
            if let Some(text) = &part.text {
                content = Some(text.clone());
            }
            if let Some(fc) = &part.function_call {
                let metadata = part
                    .thought_signature
                    .as_ref()
                    .map(|s| serde_json::json!({ "thought_signature": s }));
                tool_calls.push(ToolCall {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: fc.name.clone(),
                    arguments: fc.args.clone(),
                    metadata,
                });
            }
        }

        let usage = api_resp.usage_metadata.map(|u| ProviderUsage {
            prompt_tokens: u.prompt_token_count,
            completion_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
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
            anyhow::bail!("API key is missing for Google AI. Use '/apikey google <key>'.");
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models?key={}",
            api_key
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to Google AI API to list models")?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error listing models ({}): {}", status, err_text);
        }

        let json: serde_json::Value = resp.json().await?;
        let models: Vec<String> = json["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = m["name"].as_str()?;
                        // Strip the "models/" prefix if present
                        Some(name.strip_prefix("models/").unwrap_or(name).to_string())
                    })
                    .filter(|m| m.contains("gemini")) // Filter for Gemini models
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

// --- Gemini API types ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: u32,
    candidates_token_count: u32,
    total_token_count: u32,
}
