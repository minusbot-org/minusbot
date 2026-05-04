use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub struct OpenAiClient {
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub async fn create_chat_completion(
        &self,
        api_key: &str,
        endpoint: Option<&str>,
        body: &OpenAiChatRequest,
    ) -> Result<(OpenAiChatResponse, serde_json::Value)> {
        let base_url = endpoint.unwrap_or(&self.base_url);
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        tracing::debug!(url = %url, model = %body.model, "OpenAI API request");

        let resp = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .json(body)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        let status = resp.status();
        let raw_text = resp.text().await.context("Failed to read response body")?;
        tracing::debug!(status = %status, "OpenAI API response");

        if !status.is_success() {
            anyhow::bail!("OpenAI API error ({}): {}", status, raw_text);
        }

        let raw: serde_json::Value =
            serde_json::from_str(&raw_text).context("Failed to parse OpenAI response JSON")?;
        let response =
            serde_json::from_value(raw.clone()).context("Failed to deserialize OpenAI response")?;
        Ok((response, raw))
    }

    pub async fn list_models(&self, api_key: &str) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(api_key)
            .send()
            .await
            .context("Failed to connect to provider API to list models")?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error listing models ({}): {}", status, err_text);
        }

        let json: OpenAiModelsResponse = resp.json().await?;
        let mut models: Vec<String> = json.data.into_iter().map(|model| model.id).collect();
        models.sort();
        Ok(models)
    }
}

#[derive(Debug, Serialize)]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAiTool {
    pub r#type: String,
    pub function: OpenAiToolFunction,
}

#[derive(Debug, Serialize)]
pub struct OpenAiToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatResponse {
    pub choices: Vec<OpenAiChoice>,
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChoice {
    pub message: OpenAiResponseMessage,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponseMessage {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
}
