use anyhow::Result;
use async_trait::async_trait;
use minus_core::*;

/// OpenRouter provider — wraps the OpenAI-compatible provider with OpenRouter's base URL.
pub struct OpenRouterProvider {
    inner: minus_provider_openai::OpenAiProvider,
}

impl OpenRouterProvider {
    pub fn new(api_key: Option<String>, base_url: Option<String>) -> Self {
        let base = base_url.unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string());
        Self {
            inner: minus_provider_openai::OpenAiProvider::new(api_key, Some(base)),
        }
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    fn id(&self) -> &'static str {
        "openrouter"
    }

    fn name(&self) -> &'static str {
        "OpenRouter"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.inner.capabilities()
    }

    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.inner.complete(request).await
    }

    async fn list_models(&self, secret_key: Option<String>) -> Result<Vec<String>> {
        self.inner.list_models(secret_key).await
    }

    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }
}
