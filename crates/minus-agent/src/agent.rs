use anyhow::{Context, Result};
use minus_core::*;
use minus_db::Database;
use minus_env::{AppConfig, SecretsManager};
use minus_policy::{PolicyAction, PolicyEngine};
use minus_providers::ProviderRegistry;
use minus_skills::SkillManager;
use minus_tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// The Agent orchestrates the conversation loop:
/// incoming message → history → skills → tools → provider → response.
pub struct Agent {
    db: Database,
    config: Arc<RwLock<AppConfig>>,
    secrets: Arc<RwLock<SecretsManager>>,
    providers: Arc<RwLock<ProviderRegistry>>,
    tools: Arc<RwLock<ToolRegistry>>,
    skills: Arc<SkillManager>,
    policy: Arc<PolicyEngine>,
}

impl Agent {
    pub fn new(
        db: Database,
        config: Arc<RwLock<AppConfig>>,
        secrets: Arc<RwLock<SecretsManager>>,
        providers: Arc<RwLock<ProviderRegistry>>,
        tools: Arc<RwLock<ToolRegistry>>,
        skills: Arc<SkillManager>,
        policy: Arc<PolicyEngine>,
    ) -> Self {
        Self {
            db,
            config,
            secrets,
            providers,
            tools,
            skills,
            policy,
        }
    }

    /// Handle an incoming message and produce a response.
    pub async fn handle_message(&self, incoming: &IncomingMessage) -> Result<String> {
        let chat_id = &incoming.chat_id.0;

        // 1. Ensure chat exists
        self.db
            .ensure_chat(chat_id, &incoming.channel_id.0, chat_id, None)
            .await?;

        // 2. Save user message
        let user_msg_id = Uuid::new_v4().to_string();
        self.db
            .save_message(&user_msg_id, chat_id, "user", &incoming.content, None)
            .await?;

        // 3. Check if provider is configured and ready
        let providers = self.providers.read().await;
        let provider = match providers.default_provider() {
            Some(p) => p,
            None => return Ok("No LLM provider is configured. Use `/providers <id>` to set one.".to_string()),
        };
        
        match provider.is_ready().await {
            Ok(ready) => {
                if !ready {
                    return Ok(format!("Provider '{}' is not ready. Have you configured its API key? (e.g., `/secrets`)", provider.id()));
                }
            }
            Err(e) => return Ok(format!("Error checking provider readiness: {}", e)),
        }
        drop(providers);

        // 4. Truncate history based on config
        let (max_msgs, prov_cfg) = {
            let cfg = self.config.read().await;
            (cfg.agent.max_messages, cfg.provider.clone())
        };
        let history = self.db.get_messages(chat_id, max_msgs as i64).await?;

        // 5. Load skills for this chat
        let skills_content = self.skills.get_loaded_content(chat_id).await?;

        // 6. Fetch memories for system prompt
        let short_memories = self.db.get_memories_by_kind("short").await?;
        let long_memories = self.db.get_memories_by_kind("long").await?;

        let mut memory_context = String::new();
        if !short_memories.is_empty() {
            memory_context.push_str("\n\nKnown user profile (Short-term memory):\n");
            for m in short_memories {
                memory_context.push_str(&format!("- {}: {}\n", m.id, m.brief));
            }
        }
        if !long_memories.is_empty() {
            memory_context.push_str("\nLong-term memory available briefs (use memory.search or memory.manage to see full content):\n");
            for m in long_memories {
                memory_context.push_str(&format!("- [{}]: {}\n", m.id, m.brief));
            }
        }

        // 7. Build system prompt
        let mut system_prompt = {
            let cfg = self.config.read().await;
            cfg.agent.system_prompt.clone()
        };
        if !skills_content.is_empty() {
            system_prompt.push_str("\n\n--- Loaded Skills ---\n");
            for skill in &skills_content {
                system_prompt.push_str(skill);
                system_prompt.push('\n');
            }
        }
        system_prompt.push_str(&memory_context);
        system_prompt.push_str("\n\nYou have access to memory tools. Use memory.short_save for quick facts and memory.long_save for detailed info. You can also search conversations using chat.search.");

        // 8. Build provider messages
        let mut messages = Vec::new();
        messages.push(ProviderMessage {
            role: Role::System,
            content: system_prompt,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        for msg in &history {
            let role = match msg.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "system" => Role::System,
                "tool" => Role::Tool,
                _ => Role::User,
            };

            let metadata: Option<serde_json::Value> = msg
                .metadata_json
                .as_ref()
                .and_then(|m| serde_json::from_str(m).ok());

            let tool_calls = metadata
                .as_ref()
                .and_then(|m| m.get("tool_calls"))
                .and_then(|tc| serde_json::from_value::<Vec<ToolCall>>(tc.clone()).ok());

            let tool_call_id = metadata
                .as_ref()
                .and_then(|m| m.get("tool_call_id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());

            let name = metadata
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());

            messages.push(ProviderMessage {
                role,
                content: msg.content.clone(),
                tool_calls,
                tool_call_id,
                name,
            });
        }

        // 9. Get tool definitions
        let tools_lock = self.tools.read().await;
        let tool_defs = tools_lock.definitions();

        // 10. Provider request
        let providers = self.providers.read().await;
        let model = providers
            .default_model()
            .unwrap_or("gpt-4o-mini")
            .to_string();

        let provider_id = providers.default_id().unwrap_or("unknown");
        let secret_key = {
            let sec = self.secrets.read().await;
            let key_name = format!("SECRET_{}_API_KEY", provider_id.to_uppercase());
            sec.get(&key_name).map(|s| s.to_string())
        };

        let mut request = ProviderRequest {
            model,
            messages,
            tools: tool_defs.clone(),
            temperature: prov_cfg.temperature,
            max_tokens: prov_cfg.max_tokens,
            top_p: prov_cfg.top_p,
            secret_key,
            metadata: None,
        };

        // 10. Tool calling loop
        let max_iterations = self.config.read().await.agent.max_tool_iterations;
        let mut iteration = 0;

        loop {
            // Policy check for provider call
            let decision = self.policy.evaluate(&PolicyAction::ProviderCall {
                provider_id: providers.default_id().unwrap_or("unknown").to_string(),
            });
            if !decision.is_allowed() {
                return Ok("Provider call denied by policy.".to_string());
            }

            let response = match providers.complete(request.clone()).await {
                Ok(r) => r,
                Err(e) => return Ok(format!("Provider error: {}", e)),
            };

            // If there are tool calls, execute them
            if !response.tool_calls.is_empty() && iteration < max_iterations {
                iteration += 1;

                // Add assistant message with tool calls
                let assistant_content = response.content.clone().unwrap_or_default();
                let tc_metadata = serde_json::json!({
                    "tool_calls": response.tool_calls
                });
                let assistant_msg_id = Uuid::new_v4().to_string();
                self.db
                    .save_message(
                        &assistant_msg_id,
                        chat_id,
                        "assistant",
                        &assistant_content,
                        Some(&tc_metadata.to_string()),
                    )
                    .await?;

                request.messages.push(ProviderMessage {
                    role: Role::Assistant,
                    content: assistant_content,
                    tool_calls: Some(response.tool_calls.clone()),
                    tool_call_id: None,
                    name: None,
                });

                // Execute each tool call
                for tc in &response.tool_calls {
                    tracing::info!(tool = %tc.name, "Executing tool call");

                    // Policy check
                    let tool_decision = self.policy.evaluate(&PolicyAction::ToolCall {
                        tool_name: tc.name.clone(),
                    });
                    
                    let result = if tool_decision.is_allowed() {
                        let ctx = ToolContext {
                            chat_id: incoming.chat_id.clone(),
                            channel_id: incoming.channel_id.clone(),
                            component_id: ComponentId::new("agent"),
                            store: Some(Arc::new(self.db.clone())),
                        };
                        tools_lock.execute(tc.clone(), ctx).await?
                    } else {
                        ToolResult {
                            tool_call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            content: "Tool call denied by policy.".into(),
                            is_error: true,
                        }
                    };

                    // Save tool result as a message
                    let tool_msg_id = Uuid::new_v4().to_string();
                    let tool_metadata = serde_json::json!({
                        "tool_call_id": result.tool_call_id,
                        "name": result.name,
                    });
                    self.db
                        .save_message(
                            &tool_msg_id,
                            chat_id,
                            "tool",
                            &result.content,
                            Some(&tool_metadata.to_string()),
                        )
                        .await?;

                    request.messages.push(ProviderMessage {
                        role: Role::Tool,
                        content: result.content,
                        tool_calls: None,
                        tool_call_id: Some(result.tool_call_id),
                        name: Some(result.name),
                    });
                }

                // Continue the loop to get the final response
                continue;
            }

            // No tool calls or max iterations reached — return the final response
            let final_content = response
                .content
                .unwrap_or_else(|| "I couldn't generate a response.".to_string());

            // Save assistant response
            let assistant_msg_id = Uuid::new_v4().to_string();
            self.db
                .save_message(&assistant_msg_id, chat_id, "assistant", &final_content, None)
                .await?;

            return Ok(final_content);
        }
    }
}
