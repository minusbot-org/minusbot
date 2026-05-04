use anyhow::Result;
use async_trait::async_trait;
use minus_api::*;
use std::sync::Arc;

/// Baseten provider wraps the OpenAI-compatible provider with Baseten's endpoint.
pub struct BasetenProvider {
    inner: minus_provider_openai::OpenAiProvider,
}

impl BasetenProvider {
    pub fn new(
        api_key: Option<String>,
        base_url: Option<String>,
        config_path: Option<std::path::PathBuf>,
    ) -> Self {
        let base = base_url.unwrap_or_else(|| "https://inference.baseten.co/v1".to_string());
        Self {
            inner: minus_provider_openai::OpenAiProvider::new(api_key, Some(base), config_path),
        }
    }
}

#[async_trait]
impl Provider for BasetenProvider {
    fn id(&self) -> &'static str {
        "baseten"
    }

    fn name(&self) -> &'static str {
        "Baseten"
    }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        self.inner.config()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.inner.capabilities()
    }

    fn as_text_provider(&self) -> Option<&dyn TextProvider> {
        Some(self)
    }

    async fn is_ready(&self) -> bool {
        self.inner.is_ready().await
    }
}

#[async_trait]
impl TextProvider for BasetenProvider {
    async fn generate_text(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.inner.generate_text(request).await
    }

    async fn get_text_models(&self, secret_key: Option<String>) -> Result<Vec<String>> {
        self.inner.get_text_models(secret_key).await
    }
}
