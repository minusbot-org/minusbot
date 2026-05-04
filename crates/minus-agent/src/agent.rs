use crate::registry;
use anyhow::Result;
use minus_api::*;
use minus_db::Database;
use minus_env::{AppConfig, SecretsManager};
use minus_policy::PolicyEngine;
use minus_providers::ProviderRegistry;
use minus_skills::SkillManager;
use minus_tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use minus_vault::Vault;

/// The Agent orchestrates the conversation loop:
/// incoming message → history → skills → tools → provider → response.
pub struct Agent {
    db: Arc<Database>,
    config: Arc<RwLock<AppConfig>>,
    secrets: Arc<RwLock<SecretsManager>>,
    vault: Option<Arc<Vault>>,
    providers: Arc<RwLock<ProviderRegistry>>,
    tools: Arc<RwLock<ToolRegistry>>,
    skills: Arc<SkillManager>,
    policy: Arc<PolicyEngine>,
    scheduler: Arc<minus_scheduler::Scheduler>,
    pub registry: Arc<registry::AgentRegistry>,
    pub channels: Arc<RwLock<Option<std::sync::Weak<dyn MinusChannels>>>>,
    config_dir: std::path::PathBuf,
}

impl Agent {
    pub fn new(
        db: Database,
        config: Arc<RwLock<AppConfig>>,
        secrets: Arc<RwLock<SecretsManager>>,
        vault: Option<Arc<Vault>>,
        providers: Arc<RwLock<ProviderRegistry>>,
        tools: Arc<RwLock<ToolRegistry>>,
        skills: Arc<SkillManager>,
        policy: Arc<PolicyEngine>,
        scheduler: Arc<minus_scheduler::Scheduler>,
        registry: Arc<registry::AgentRegistry>,
        config_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            db: Arc::new(db),
            config,
            secrets,
            vault,
            providers,
            tools,
            skills,
            policy,
            scheduler,
            registry,
            channels: Arc::new(RwLock::new(None)),
            config_dir,
        }
    }

    /// Handle an incoming message and produce a response.
    pub async fn handle_message(
        self: Arc<Self>,
        incoming: &IncomingMessage,
        channel: Arc<dyn traits::Channel>,
    ) -> Result<String> {
        self.handle_message_with_agent("default", incoming, Some(channel))
            .await
    }

    pub async fn handle_message_with_agent(
        self: Arc<Self>,
        agent_id: &str,
        incoming: &IncomingMessage,
        channel: Option<Arc<dyn traits::Channel>>,
    ) -> Result<String> {
        let chat_id = &incoming.chat_id.0;

        // Show typing indicator
        if let Some(ref ch) = channel {
            let _ = ch.set_typing(incoming.chat_id.clone(), true).await;
        }

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
            None => {
                return Ok(
                    "No LLM provider is configured. Use `/providers <id>` to set one.".to_string(),
                )
            }
        };

        if !provider.is_ready().await {
            return Ok(format!(
                "Provider '{}' is not ready. Use '/models' to select a model.",
                provider.id()
            ));
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
        let important_memories = self.db.get_important_memories().await?;
        let mut memory_briefs = String::new();
        if important_memories.is_empty() {
            memory_briefs.push_str("(No important memories saved yet)");
        } else {
            for m in important_memories {
                memory_briefs.push_str(&format!(
                    "- [{}]: {} (created: {})\n",
                    m.id, m.brief, m.created_at
                ));
            }
        }

        // 6b. Fetch available channels
        let mut channels_info = Vec::new();
        if let Some(channels_service) = self
            .channels
            .read()
            .await
            .as_ref()
            .and_then(|w| w.upgrade())
        {
            let statuses = channels_service.list_channels().await;
            for status in statuses {
                if status.is_ready {
                    let chat_str = status
                        .active_chat_id
                        .map(|c| c.0)
                        .unwrap_or_else(|| "none".to_string());
                    channels_info.push(format!(
                        "{} (id: {}, chat: {})",
                        status.name, status.id, chat_str
                    ));
                }
            }
        }
        let available_channels = if channels_info.is_empty() {
            "No other ready channels available.".to_string()
        } else {
            channels_info.join(", ")
        };

        // 7. Build system prompt
        let now = chrono::Local::now();
        let date_str = now.format("%Y-%m-%d").to_string();
        let time_str = now.format("%H:%M:%S").to_string();

        let mut system_prompt = if let Some(agent) = self.registry.get(agent_id).await {
            agent.system_prompt
        } else {
            let custom_prompt_path = self.config_dir.join("system_prompt.md");
            if custom_prompt_path.exists() {
                std::fs::read_to_string(custom_prompt_path)
                    .unwrap_or_else(|_| include_str!("system_prompt.md").to_string())
            } else {
                include_str!("system_prompt.md").to_string()
            }
        };

        // Replace placeholders
        system_prompt = system_prompt
            .replace("{date}", &date_str)
            .replace("{time}", &time_str)
            .replace("{available_channels}", &available_channels)
            .replace("{chat_id}", chat_id)
            .replace("{channel_id}", &incoming.channel_id.0)
            .replace("{memory_briefs}", &memory_briefs);

        if !skills_content.is_empty() {
            system_prompt.push_str("\n\n--- Loaded Skills ---\n");
            for skill in &skills_content {
                system_prompt.push_str(skill);
                system_prompt.push('\n');
            }
        }

        // 8. Build provider messages
        let mut messages = Vec::new();
        messages.push(ProviderMessage {
            role: Role::System,
            content: Some(system_prompt),
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
                .and_then(|tc| serde_json::from_value::<Vec<ToolCall>>(tc.clone()).ok())
                .filter(|v| !v.is_empty());

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

            let content =
                if msg.content.is_empty() && role == Role::Assistant && tool_calls.is_some() {
                    None
                } else {
                    Some(msg.content.clone())
                };

            messages.push(ProviderMessage {
                role,
                content,
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
        let model = providers.default_model().unwrap_or("").to_string();

        let provider_id = providers.default_id().unwrap_or("unknown");
        let key_name = format!("PROVIDER_{}_API_KEY", provider_id.to_uppercase());

        let api_key = if let Some(vault) = &self.vault {
            match vault.get_secret(&key_name) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                Err(_) => {
                    let sec = self.secrets.read().await;
                    sec.get(&key_name)
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                }
            }
        } else {
            let sec = self.secrets.read().await;
            sec.get(&key_name)
                .map(|s| s.to_string())
                .unwrap_or_default()
        };

        let endpoint = if let Some(p) = providers.default_provider() {
            if let Some(cp) = p.config() {
                cp.read_config("endpoint").await.ok().flatten()
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| {
            let endpoint_key = format!("PROVIDER_{}_ENDPOINT", provider_id.to_uppercase());
            if let Some(vault) = &self.vault {
                match vault.get_secret(&endpoint_key) {
                    Ok(bytes) => Some(String::from_utf8_lossy(&bytes).to_string()),
                    Err(_) => {
                        let sec = self.secrets.try_read().ok()?;
                        sec.get(&endpoint_key).map(|s| s.to_string())
                    }
                }
            } else {
                let sec = self.secrets.try_read().ok()?;
                sec.get(&endpoint_key).map(|s| s.to_string())
            }
        });

        let options = TextInferenceOptions {
            api_key,
            model,
            endpoint,
            temperature: prov_cfg.temperature.unwrap_or(0.7),
            max_tokens: prov_cfg.max_tokens,
            top_p: prov_cfg.top_p.unwrap_or(1.0),
        };

        let mut request = ProviderRequest {
            messages,
            tools: tool_defs.clone(),
            options,
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
                    content: if assistant_content.is_empty() {
                        None
                    } else {
                        Some(assistant_content)
                    },
                    tool_calls: Some(response.tool_calls.clone()),
                    tool_call_id: None,
                    name: None,
                });

                // Execute each tool call
                for tc in &response.tool_calls {
                    tracing::info!(tool = %tc.name, "Executing tool call");

                    if let Some(ref ch) = channel {
                        let _ = ch
                            .send_tool_call(ToolCallPacket {
                                chat_id: incoming.chat_id.clone(),
                                call_id: tc.id.clone(),
                                name: tc.name.clone(),
                                brief: String::new(), // TODO: Add actual brief if needed
                            })
                            .await;
                    }

                    // Policy check
                    let tool_decision = self.policy.evaluate(&PolicyAction::ToolCall {
                        tool_name: tc.name.clone(),
                    });

                    let result = if tool_decision.is_allowed() {
                        let channels = self
                            .channels
                            .read()
                            .await
                            .as_ref()
                            .and_then(|w| w.upgrade());

                        if let Some(channels) = channels {
                            let ctx = ToolContext {
                                chat_id: incoming.chat_id.clone(),
                                channel_id: incoming.channel_id.clone(),
                                component_id: ComponentId::new("agent"),
                                db: self.db.clone() as Arc<dyn MinusDatabase>,
                                scheduler: self.scheduler.clone() as Arc<dyn MinusScheduler>,
                                channels,
                                agent: self.clone() as Arc<dyn MinusAgent>,
                            };
                            match tools_lock.execute(tc.clone(), ctx).await {
                                Ok(res) => res,
                                Err(e) => ToolResult {
                                    tool_call_id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    content: format!("Error: {}", e),
                                    is_error: true,
                                },
                            }
                        } else {
                            ToolResult {
                                tool_call_id: tc.id.clone(),
                                name: tc.name.clone(),
                                content: "Error: Channels service not available in agent"
                                    .to_string(),
                                is_error: true,
                            }
                        }
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
                        content: Some(result.content),
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
                .save_message(
                    &assistant_msg_id,
                    chat_id,
                    "assistant",
                    &final_content,
                    None,
                )
                .await?;

            return Ok(final_content);
        }
    }
}

#[async_trait]
impl traits::MinusAgent for Agent {
    async fn list_agents(self: Arc<Self>) -> Result<Vec<AgentStatus>> {
        let agents = self.registry.list().await;
        Ok(agents
            .into_iter()
            .map(|a| AgentStatus {
                id: a.id,
                name: a.name,
                description: None,
            })
            .collect())
    }

    async fn call_agent(
        self: Arc<Self>,
        agent_id: &str,
        content: &str,
        chat_id: ChatId,
        channel_id: ChannelId,
    ) -> Result<String> {
        let incoming = IncomingMessage::new(chat_id, channel_id, content.to_string());
        self.handle_message_with_agent(agent_id, &incoming, None)
            .await
    }
}
