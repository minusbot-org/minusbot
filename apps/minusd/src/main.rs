use anyhow::Result;
use minus_agent::Agent;
use minus_api::*;
use minus_channel_cli::CliChannel;
use minus_channel_telegram::TelegramChannel;

use minus_db::Database;
use minus_env::{AppConfig, DataDir, SecretsManager};
use minus_policy::PolicyEngine;
use minus_provider_anthropic::AnthropicProvider;
use minus_provider_baseten::BasetenProvider;
use minus_provider_cerebras::CerebrasProvider;
use minus_provider_deepseek::DeepSeekProvider;
use minus_provider_groq::GroqProvider;
use minus_provider_openai::OpenAiProvider;
use minus_provider_openrouter::OpenRouterProvider;
use minus_provider_nvidia::NvidiaProvider;
use minus_provider_together_ai::TogetherAiProvider;
use minus_provider_x_ai::XAiProvider;
use minus_providers::ProviderRegistry;
use minus_runtime::Runtime;
use minus_scheduler::Scheduler;
use minus_skills::SkillManager;
use minus_tools::ToolRegistry;
use minus_vault::Vault;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing_subscriber::EnvFilter;

const VERSION: &str = "0.1.0";

#[tokio::main]
async fn main() -> Result<()> {
    let debug_mode = std::env::var("MINUS_DEBUG")
        .map(|v| v == "1")
        .unwrap_or(false)
        || std::env::args().any(|a| a == "--debug");

    // 1. Setup logging
    let data_dir = DataDir::resolve()?;
    data_dir.ensure_dirs()?;
    let log_file = data_dir.log_file_today();
    let file_appender =
        tracing_appender::rolling::never(log_file.parent().unwrap(), log_file.file_name().unwrap());
    let (_non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = if debug_mode {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(true)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    tracing::info!("Starting minusbot v{} (debug: {})", VERSION, debug_mode);

    // Instance lock check
    #[cfg(unix)]
    let socket_path = data_dir.root.join("minusd.sock");
    #[cfg(unix)]
    if socket_path.exists() {
        if tokio::net::UnixStream::connect(&socket_path).await.is_ok() {
            tracing::error!("Another instance of minusd is already running.");
            eprintln!("Error: Another instance of minusd is already running.");
            std::process::exit(1);
        }
        let _ = std::fs::remove_file(&socket_path);
    }

    let first_run = data_dir.is_first_run();

    // 2. Load config
    let mut config_raw = AppConfig::load(&data_dir.config_path())?;
    if first_run {
        config_raw.core.first_run = false;
        config_raw.save(&data_dir.config_path())?;
    }
    let config = Arc::new(RwLock::new(config_raw.clone()));

    // 3. Load secrets
    let secrets = SecretsManager::load(&data_dir.secrets_env_path())?;
    let secrets = Arc::new(RwLock::new(secrets));

    // 4. Open database
    let db = Database::open(&data_dir.database_url()).await?;

    // 5. Initialize vault
    let vault = match Vault::open(&data_dir.vault_dir()) {
        Ok(v) => Some(Arc::new(v)),
        Err(e) => {
            eprintln!("Warning: Failed to open vault: {}", e);
            None
        }
    };

    // 6. Policy engine
    let policy = {
        let cfg = config.read().await;
        Arc::new(PolicyEngine::new(cfg.clone()))
    };

    // 7. Provider registry
    let mut provider_reg = ProviderRegistry::new();
    {
        let sec = secrets.read().await;
        let openai_key = sec.get("PROVIDER_OPENAI_API_KEY").map(|s| s.to_string());
        let openai_base = sec.get("PROVIDER_OPENAI_ENDPOINT").map(|s| s.to_string());
        let openai_cfg = data_dir.component_config_path("provider", "openai");
        provider_reg.register(Arc::new(OpenAiProvider::new(
            openai_key,
            openai_base,
            Some(openai_cfg),
        )));

        let openrouter_key = sec
            .get("PROVIDER_OPENROUTER_API_KEY")
            .map(|s| s.to_string());
        let openrouter_base = sec
            .get("PROVIDER_OPENROUTER_ENDPOINT")
            .map(|s| s.to_string());
        let openrouter_cfg = data_dir.component_config_path("provider", "openrouter");
        provider_reg.register(Arc::new(OpenRouterProvider::new(
            openrouter_key,
            openrouter_base,
            Some(openrouter_cfg),
        )));

        let nvidia_key = sec
            .get("PROVIDER_NVIDIA_API_KEY")
            .map(|s| s.to_string());
        let nvidia_base = sec
            .get("PROVIDER_NVIDIA_ENDPOINT")
            .map(|s| s.to_string());
        let nvidia_cfg = data_dir.component_config_path("provider", "nvidia");
        provider_reg.register(Arc::new(NvidiaProvider::new(
            nvidia_key,
            nvidia_base,
            Some(nvidia_cfg),
        )));

        let groq_key = sec.get("PROVIDER_GROQ_API_KEY").map(|s| s.to_string());
        let groq_base = sec.get("PROVIDER_GROQ_ENDPOINT").map(|s| s.to_string());
        let groq_cfg = data_dir.component_config_path("provider", "groq");
        provider_reg.register(Arc::new(GroqProvider::new(
            groq_key,
            groq_base,
            Some(groq_cfg),
        )));

        let deepseek_key = sec.get("PROVIDER_DEEPSEEK_API_KEY").map(|s| s.to_string());
        let deepseek_base = sec.get("PROVIDER_DEEPSEEK_ENDPOINT").map(|s| s.to_string());
        let deepseek_cfg = data_dir.component_config_path("provider", "deepseek");
        provider_reg.register(Arc::new(DeepSeekProvider::new(
            deepseek_key,
            deepseek_base,
            Some(deepseek_cfg),
        )));

        let together_ai_key = sec
            .get("PROVIDER_TOGETHER_AI_API_KEY")
            .map(|s| s.to_string());
        let together_ai_base = sec
            .get("PROVIDER_TOGETHER_AI_ENDPOINT")
            .map(|s| s.to_string());
        let together_ai_cfg = data_dir.component_config_path("provider", "together-ai");
        provider_reg.register(Arc::new(TogetherAiProvider::new(
            together_ai_key,
            together_ai_base,
            Some(together_ai_cfg),
        )));

        let baseten_key = sec.get("PROVIDER_BASETEN_API_KEY").map(|s| s.to_string());
        let baseten_base = sec.get("PROVIDER_BASETEN_ENDPOINT").map(|s| s.to_string());
        let baseten_cfg = data_dir.component_config_path("provider", "baseten");
        provider_reg.register(Arc::new(BasetenProvider::new(
            baseten_key,
            baseten_base,
            Some(baseten_cfg),
        )));

        let cerebras_key = sec.get("PROVIDER_CEREBRAS_API_KEY").map(|s| s.to_string());
        let cerebras_base = sec.get("PROVIDER_CEREBRAS_ENDPOINT").map(|s| s.to_string());
        let cerebras_cfg = data_dir.component_config_path("provider", "cerebras");
        provider_reg.register(Arc::new(CerebrasProvider::new(
            cerebras_key,
            cerebras_base,
            Some(cerebras_cfg),
        )));

        let x_ai_key = sec.get("PROVIDER_X_AI_API_KEY").map(|s| s.to_string());
        let x_ai_base = sec.get("PROVIDER_X_AI_ENDPOINT").map(|s| s.to_string());
        let x_ai_cfg = data_dir.component_config_path("provider", "x-ai");
        provider_reg.register(Arc::new(XAiProvider::new(
            x_ai_key,
            x_ai_base,
            Some(x_ai_cfg),
        )));

        let anthropic_key = sec.get("PROVIDER_ANTHROPIC_API_KEY").map(|s| s.to_string());
        let anthropic_base = sec
            .get("PROVIDER_ANTHROPIC_ENDPOINT")
            .map(|s| s.to_string());
        let anthropic_cfg = data_dir.component_config_path("provider", "anthropic");
        provider_reg.register(Arc::new(AnthropicProvider::new(
            anthropic_key,
            anthropic_base,
            Some(anthropic_cfg),
        )));

        let custom_openai_key = sec
            .get("PROVIDER_CUSTOM_OPENAI_API_KEY")
            .map(|s| s.to_string());
        let custom_openai_base = sec
            .get("PROVIDER_CUSTOM_OPENAI_ENDPOINT")
            .map(|s| s.to_string());
        let custom_openai_cfg = data_dir.component_config_path("provider", "custom_openai");
        provider_reg.register(Arc::new(minus_provider_openai::CustomOpenAiProvider::new(
            custom_openai_key,
            custom_openai_base,
            Some(custom_openai_cfg),
        )));

        let google_key = sec.get("PROVIDER_GOOGLE_API_KEY").map(|s| s.to_string());
        let google_cfg = data_dir.component_config_path("provider", "google");
        provider_reg.register(Arc::new(minus_provider_google::GoogleAiProvider::new(
            google_key,
            Some(google_cfg),
        )));

        let mistral_key = sec.get("PROVIDER_MISTRAL_API_KEY").map(|s| s.to_string());
        let mistral_cfg = data_dir.component_config_path("provider", "mistral");
        provider_reg.register(Arc::new(minus_provider_mistral::MistralAiProvider::new(
            mistral_key,
            Some(mistral_cfg),
        )));
    }

    {
        let cfg = config.read().await;
        if let Some(default_id) = &cfg.provider.default {
            let _ = provider_reg.set_default(default_id);
        }

        // Try to load model from the default provider's own config first
        let mut loaded_model = None;
        if let Some(p) = provider_reg.default_provider() {
            if let Some(cp) = p.config() {
                if let Ok(Some(model)) = cp.read_config("text_model").await {
                    loaded_model = Some(model);
                }
            }
        }

        // Fallback to main config if not found in provider config
        if loaded_model.is_none() {
            loaded_model = cfg.provider.text_model.clone();
        }

        if let Some(model) = loaded_model {
            provider_reg.set_default_model(&model);
        }

        // Print startup banner
        let provider_display = provider_reg.default_id().unwrap_or("(none)");
        let model_display = provider_reg.default_model().unwrap_or("(none)");

        eprintln!();
        eprintln!("\x1b[35m      ██    ██    \x1b[0m");
        eprintln!("\x1b[35m      ██    ██    \x1b[0m");
        eprintln!("\x1b[35m     ██████████   \x1b[0m");
        eprintln!(
            "\x1b[35m    ███ ████ ███  \x1b[0m  \x1b[1;36mMinusbot v{}\x1b[0m",
            VERSION
        );
        eprintln!("\x1b[35m     ██████████   \x1b[0m  A self-hosted personal AI assistant");
        eprintln!("\x1b[35m       ██████     \x1b[0m");
        eprintln!("\x1b[35m      ███████     \x1b[0m");
        eprintln!("\x1b[35m       ██  ██     \x1b[0m");
        eprintln!();

        eprintln!(
            "  \x1b[36mData dir\x1b[0m   -> \x1b[32m{}\x1b[0m",
            data_dir.root.display()
        );
        eprintln!(
            "  \x1b[36mDatabase\x1b[0m   -> \x1b[32m{}\x1b[0m",
            data_dir.database_url()
        );
        eprintln!(
            "  \x1b[36mProvider\x1b[0m   -> \x1b[33m{}\x1b[0m",
            provider_display
        );
        eprintln!(
            "  \x1b[36mModel\x1b[0m      -> \x1b[33m{}\x1b[0m",
            model_display
        );
        eprintln!();
    }
    let providers = Arc::new(RwLock::new(provider_reg));

    // 8. Tool registry
    let mut tool_reg = ToolRegistry::new();
    for tool in minus_tools::builtin::all_builtin_tools() {
        tool_reg.register(tool);
    }
    let tools = Arc::new(RwLock::new(tool_reg));

    // 9. Skills manager
    let skills = Arc::new(SkillManager::new(data_dir.skills_dir(), db.clone()));
    skills.scan_and_index().await?;

    // 10. Scheduler
    let (job_tx, mut job_rx) = mpsc::channel::<minus_scheduler::JobTrigger>(64);
    let scheduler = Arc::new(Scheduler::new(db.clone(), job_tx));

    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    scheduler.clone().start(shutdown_rx);

    // 11. Agent
    let agents_dir = data_dir.root.join("agents");
    let registry = Arc::new(minus_agent::registry::AgentRegistry::new(agents_dir));
    registry.scan().await?;

    let agent = Arc::new(Agent::new(
        db.clone(),
        config.clone(),
        secrets.clone(),
        vault.clone(),
        providers.clone(),
        tools.clone(),
        skills.clone(),
        policy.clone(),
        scheduler.clone(),
        registry.clone(),
        data_dir.config_dir(),
    ));

    // 12. Commands registry
    let mut command_reg = minus_commands::CommandRegistry::new();
    minus_commands::builtin::register_all(&mut command_reg);
    let commands = Arc::new(RwLock::new(command_reg));

    // 13. Build Runtime
    let runtime = Arc::new(Runtime {
        db: db.clone(),
        config: config.clone(),
        config_path: data_dir.config_path(),
        config_dir: data_dir.config_dir(),
        secrets: secrets.clone(),
        vault,
        policy,
        providers,
        tools,
        skills,
        scheduler,
        agent: agent.clone(),
        commands,
        config_providers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        channels: Arc::new(RwLock::new(std::collections::HashMap::new())),
        integrations: Arc::new(RwLock::new(Vec::new())),
        shutdown_tx: shutdown_tx.clone(),
    });

    // Circular dependency resolution
    {
        let mut c = agent.channels.write().await;
        *c = Some(Arc::downgrade(&(runtime.clone() as Arc<dyn MinusChannels>)));
    }

    // 14. Start CLI channel
    let (msg_tx, mut msg_rx) = mpsc::channel::<IncomingMessage>(64);
    let config_dir = data_dir.root.join("config");
    let cli_channel = Arc::new(CliChannel::new(msg_tx.clone(), config_dir.clone()));

    // Register channel via Runtime's register method
    runtime.register_channel(cli_channel.clone()).await;

    // Spawn CLI listener
    let chan = cli_channel.clone();
    let ctx = ChannelContext {
        channel_id: ChannelId(minus_api::Channel::id(chan.as_ref()).to_string()),
        config_dir: data_dir.config_dir(),
        shutdown: shutdown_tx.clone(),
    };
    let chan_handle = tokio::spawn(async move {
        if let Err(e) = chan.start(ctx).await {
            tracing::error!(error = %e, "CLI channel error");
        }
    });

    // 14b. Start Telegram channel
    let telegram_secrets = runtime.get_store("channel:telegram").await?;
    let telegram_channel = Arc::new(TelegramChannel::new(
        msg_tx.clone(),
        config_dir.clone(),
        telegram_secrets,
    ));

    runtime.register_channel(telegram_channel.clone()).await;

    let chan_tg = telegram_channel.clone();
    let ctx_tg = ChannelContext {
        channel_id: ChannelId("telegram".into()),
        config_dir: data_dir.config_dir(),
        shutdown: shutdown_tx.clone(),
    };
    let tg_chan_handle = tokio::spawn(async move {
        if let Err(e) = chan_tg.start(ctx_tg).await {
            tracing::error!(error = %e, "Telegram channel error");
        }
    });

    // 14c. Register commands to channels
    {
        let cmds = runtime.commands.read().await.list();
        let _ = cli_channel.register_commands(cmds.clone()).await;
        let _ = telegram_channel.register_commands(cmds).await;
    }

    // 15. Main message loop
    let mut shutdown_rx2 = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            Some(trigger) = job_rx.recv() => {
                if let Some(chat_id_str) = trigger.target_chat_id {
                    let chat_id = ChatId(chat_id_str);

                    for action in trigger.actions {
                        match action {
                            minus_scheduler::JobAction::MessageSend { content, generate } => {
                                let msg = MessagePacket::new(chat_id.clone(), "assistant", content.clone());

                                // Broadcast to all active channels for this chat
                                let channels = runtime.channels.read().await;
                                for channel in channels.values() {
                                    if channel.is_chat_active(chat_id.clone()).await {
                                        let _ = channel.send_message(msg.clone()).await;
                                    }
                                }

                                if generate {
                                    // Use default agent
                                    let mut source_channel = None;
                                    for channel in channels.values() {
                                        if channel.is_chat_active(chat_id.clone()).await {
                                            source_channel = Some(channel.clone());
                                            break;
                                        }
                                    }

                                    if let Some(channel) = source_channel {
                                        let incoming = IncomingMessage::new(chat_id.clone(), ChannelId(channel.id().into()), content);
                                        let response = Runtime::process_message(runtime.clone(), &incoming, channel.clone()).await;

                                        let response_text = match response {
                                            Ok(text) => text,
                                            Err(e) => format!("Error: {}", e),
                                        };
                                        let out = MessagePacket::new(incoming.chat_id.clone(), "assistant", response_text);

                                        for ch in channels.values() {
                                            if ch.is_chat_active(incoming.chat_id.clone()).await {
                                                let _ = ch.send_message(out.clone()).await;
                                            }
                                        }
                                    }
                                }
                            }
                            minus_scheduler::JobAction::AgentPrompt { prompt, agent_id } => {
                                let mut source_channel = None;
                                let channels = runtime.channels.read().await;
                                for channel in channels.values() {
                                    if channel.is_chat_active(chat_id.clone()).await {
                                        source_channel = Some(channel.clone());
                                        break;
                                    }
                                }

                                if let Some(channel) = source_channel {
                                    let incoming = IncomingMessage::new(chat_id.clone(), ChannelId(channel.id().into()), prompt);
                                    let agent_name = agent_id.as_deref().unwrap_or("default");

                                    let response = runtime.agent.clone().handle_message_with_agent(agent_name, &incoming, Some(channel.clone())).await;

                                    let response_text = match response {
                                        Ok(text) => text,
                                        Err(e) => format!("Error: {}", e),
                                    };
                                    let out = MessagePacket::new(incoming.chat_id.clone(), "assistant", response_text);

                                    for ch in channels.values() {
                                        if ch.is_chat_active(incoming.chat_id.clone()).await {
                                            let _ = ch.send_message(out.clone()).await;
                                        }
                                    }
                                }
                            }
                            minus_scheduler::JobAction::UseChat { chat_id: target_cid, content } => {
                                let target_chat_id = ChatId(target_cid);
                                let mut source_channel = None;
                                let channels = runtime.channels.read().await;
                                for channel in channels.values() {
                                    if channel.is_chat_active(target_chat_id.clone()).await {
                                        source_channel = Some(channel.clone());
                                        break;
                                    }
                                }

                                if let Some(channel) = source_channel {
                                    // Send the content first
                                    let msg = MessagePacket::new(target_chat_id.clone(), "assistant", content.clone());
                                    for ch in channels.values() {
                                        if ch.is_chat_active(target_chat_id.clone()).await {
                                            let _ = ch.send_message(msg.clone()).await;
                                        }
                                    }

                                    let incoming = IncomingMessage::new(target_chat_id.clone(), ChannelId(channel.id().into()), content);
                                    let response = Runtime::process_message(runtime.clone(), &incoming, channel.clone()).await;

                                    let response_text = match response {
                                        Ok(text) => text,
                                        Err(e) => format!("Error: {}", e),
                                    };
                                    let out = MessagePacket::new(incoming.chat_id.clone(), "assistant", response_text);

                                    for ch in channels.values() {
                                        if ch.is_chat_active(incoming.chat_id.clone()).await {
                                            let _ = ch.send_message(out.clone()).await;
                                        }
                                    }
                                }
                            }
                            minus_scheduler::JobAction::AskAgent { chat_id: target_cid, content, agent_id } => {
                                let target_chat_id = ChatId(target_cid);
                                let mut source_channel = None;
                                let channels = runtime.channels.read().await;
                                for channel in channels.values() {
                                    if channel.is_chat_active(target_chat_id.clone()).await {
                                        source_channel = Some(channel.clone());
                                        break;
                                    }
                                }

                                if let Some(channel) = source_channel {
                                    let incoming = IncomingMessage::new(target_chat_id.clone(), ChannelId(channel.id().into()), content);
                                    let response = runtime.agent.clone().handle_message_with_agent(&agent_id, &incoming, Some(channel.clone())).await;

                                    let response_text = match response {
                                        Ok(text) => text,
                                        Err(e) => format!("Error: {}", e),
                                    };
                                    let out = MessagePacket::new(incoming.chat_id.clone(), "assistant", response_text);

                                    for ch in channels.values() {
                                        if ch.is_chat_active(incoming.chat_id.clone()).await {
                                            let _ = ch.send_message(out.clone()).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(incoming) = msg_rx.recv() => {
                let channel = runtime.get_channel(&incoming.channel_id.0).await
                    .unwrap_or_else(|| cli_channel.clone() as Arc<dyn Channel>);

                let response = if minus_commands::is_slash_command(&incoming.content) {
                    Runtime::process_command(runtime.clone(), &incoming, channel.clone()).await
                } else {
                    Runtime::process_message(runtime.clone(), &incoming, channel.clone()).await
                };

                let response_text = match response {
                    Ok(text) => text,
                    Err(e) => format!("Error: {}", e),
                };
                let out = MessagePacket::new(incoming.chat_id.clone(), "assistant", response_text);

                // Broadcast response to all active channels for this chat
                let channels = runtime.channels.read().await;
                for ch in channels.values() {
                    if ch.is_chat_active(incoming.chat_id.clone()).await {
                        if let Err(e) = ch.send_message(out.clone()).await {
                            tracing::error!(channel_id = %ch.id(), error = %e, "Failed to broadcast response");
                        }
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nInterrupted by user (Ctrl+C)");
                let _ = shutdown_tx.send(());
                break;
            }
            _ = shutdown_rx2.recv() => {
                eprintln!("Shutting down...");
                break;
            }
        }
    }

    runtime.stop().await;
    chan_handle.abort();
    tg_chan_handle.abort();
    Ok(())
}
