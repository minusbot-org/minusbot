use anyhow::{bail, Result};
use minus_api::{Provider, ProviderRequest, ProviderResponse};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of all available LLM providers.
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn Provider>>,
    default_id: Option<String>,
    default_model: Option<String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_id: None,
            default_model: None,
        }
    }

    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        let id = provider.id().to_string();
        tracing::info!(provider_id = %id, "Registered provider");
        self.providers.insert(id, provider);
    }

    pub fn set_default(&mut self, id: &str) -> Result<()> {
        if !self.providers.contains_key(id) {
            bail!("Provider '{}' not registered", id);
        }
        self.default_id = Some(id.to_string());
        Ok(())
    }

    pub fn set_default_model(&mut self, model: &str) {
        self.default_model = Some(model.to_string());
    }

    pub fn default_provider(&self) -> Option<Arc<dyn Provider>> {
        self.default_id
            .as_ref()
            .and_then(|id| self.providers.get(id))
            .cloned()
    }

    pub fn default_model(&self) -> Option<&str> {
        self.default_model.as_deref()
    }

    pub fn default_id(&self) -> Option<&str> {
        self.default_id.as_deref()
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(id).cloned()
    }

    pub fn list(&self) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .providers
            .values()
            .map(|p| (p.id().to_string(), p.name().to_string()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    pub fn is_configured(&self) -> bool {
        self.default_id.is_some()
    }

    /// Call the default provider's text completion.
    pub async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let provider = self
            .default_provider()
            .with_context(|| "No default provider configured")?;

        let text_provider = provider
            .as_text_provider()
            .with_context(|| format!("Provider '{}' does not support text generation", provider.id()))?;

        // Use default model if not specified in request
        let mut request = request;
        if request.options.model.is_empty() {
            if let Some(model) = &self.default_model {
                request.options.model = model.clone();
            } else {
                bail!("No model specified and no default text model configured");
            }
        }

        text_provider.generate_text(request).await
    }

    /// Get text models for the default provider.
    pub async fn get_text_models(&self, secret_key: Option<String>) -> Result<Vec<String>> {
        let provider = self
            .default_provider()
            .with_context(|| "No default provider configured")?;

        let text_provider = provider
            .as_text_provider()
            .with_context(|| format!("Provider '{}' does not support text generation", provider.id()))?;

        text_provider.get_text_models(secret_key).await
    }
}


impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

use anyhow::Context as _;

#[minus_api::async_trait]
impl minus_api::traits::MinusProviders for ProviderRegistry {
    async fn list_providers(&self) -> Result<Vec<(String, String)>> {
        Ok(self.list())
    }
    async fn set_default_provider(&self, _id: &str) -> Result<()> {
        anyhow::bail!("Cannot set default provider on immutable registry. Use RwLock wrapper.")
    }
    async fn list_text_models(&self) -> Result<Vec<String>> {
        self.get_text_models(None).await
    }
    async fn get_default_provider_id(&self) -> Result<String> {
        Ok(self.default_id().unwrap_or("").to_string())
    }
    async fn get_default_text_model(&self) -> Result<String> {
        Ok(self.default_model().unwrap_or("").to_string())
    }
    async fn set_default_text_model(&self, _model: &str) -> Result<()> {
        anyhow::bail!("Cannot set default model on immutable registry.")
    }
    async fn resolve_api_key(&self, _provider_id: &str) -> Result<Option<String>> {
        // The registry doesn't have vault access — the Runtime's impl handles this.
        Ok(None)
    }
    async fn get_provider(&self, id: &str) -> Result<Option<Arc<dyn Provider>>> {
        Ok(self.get(id))
    }
}
