use anyhow::Result;
use minus_agent::Agent;
use minus_channel_unix::UnixChannel;
use minus_core::*;
use minus_db::Database;
use minus_env::{AppConfig, DataDir, SecretsManager};
use minus_policy::PolicyEngine;
use minus_provider_openai::OpenAiProvider;
use minus_provider_openrouter::OpenRouterProvider;
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
    let debug_mode = std::env::var("MINUS_DEBUG").map(|v| v == "1").unwrap_or(false)
        || std::env::args().any(|a| a == "--debug");

    // 1. Setup logging
    let data_dir = DataDir::resolve()?;
    data_dir.ensure_dirs()?;
    let log_file = data_dir.log_file_today();
    let file_appender = tracing_appender::rolling::never(
        log_file.parent().unwrap(),
        log_file.file_name().unwrap(),
    );
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
    let socket_path = data_dir.root.join("minusd.sock");
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
    let config = Arc::new(RwLock::new(config_raw));

    // 4. Load secrets
    let secrets_raw = SecretsManager::load(&data_dir.secrets_env_path())?;

    // 5. Open database
    let db = Database::open(&data_dir.database_url()).await?;

    // 6. Initialize vault
    let vault = {
        let master_key = secrets_raw.get("MINUSBOT_VAULT_MASTER_KEY");
        match master_key {
            Some(key) if !key.is_empty() => match Vault::open(&data_dir.vault_dir(), key) {
                Ok(v) => Some(Arc::new(v)),
                Err(e) => {
                    eprintln!("Warning: Failed to open vault: {}", e);
                    None
                }
            },
            _ => {
                if first_run {
                    let key = Vault::generate_master_key();
                    let mut secrets_mut = secrets_raw.clone();
                    secrets_mut.set("MINUSBOT_VAULT_MASTER_KEY", &key)?;
                    eprintln!(
                        "⚠ Generated development vault master key. \
                         For production, set MINUSBOT_VAULT_MASTER_KEY."
                    );
                    match Vault::open(&data_dir.vault_dir(), &key) {
                        Ok(v) => Some(Arc::new(v)),
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
        }
    };

    let secrets = SecretsManager::load(&data_dir.secrets_env_path())?;
    let secrets = Arc::new(RwLock::new(secrets));

    // 7. Policy engine
    let policy = {
        let cfg = config.read().await;
        Arc::new(PolicyEngine::new(cfg.clone()))
    };

    // 8. Provider registry
    let mut provider_reg = ProviderRegistry::new();
    {
        let sec = secrets.read().await;
        let openai_key = sec.get("PROVIDER_OPENAI_API_KEY").map(|s| s.to_string());
        let openai_base = sec.get("PROVIDER_OPENAI_ENDPOINT").map(|s| s.to_string());
        provider_reg.register(Arc::new(OpenAiProvider::new(openai_key, openai_base)));

        let openrouter_key = sec.get("PROVIDER_OPENROUTER_API_KEY").map(|s| s.to_string());
        let openrouter_base = sec.get("PROVIDER_OPENROUTER_ENDPOINT").map(|s| s.to_string());
        provider_reg.register(Arc::new(OpenRouterProvider::new(openrouter_key, openrouter_base)));
    }

    {
        let cfg = config.read().await;
        if let Some(default_id) = &cfg.provider.default {
            let _ = provider_reg.set_default(default_id);
        }
        if let Some(model) = &cfg.provider.model {
            provider_reg.set_default_model(model);
        }
    }
    let providers = Arc::new(RwLock::new(provider_reg));

    // 9. Tool registry
    let mut tool_reg = ToolRegistry::new();
    for tool in minus_tools::builtin::all_builtin_tools() {
        tool_reg.register(tool);
    }
    let tools = Arc::new(RwLock::new(tool_reg));

    // 10. Skills manager
    let skills = Arc::new(SkillManager::new(data_dir.skills_dir(), db.clone()));
    skills.scan_and_index().await?;

    // 11. Scheduler
    let (job_tx, _job_rx) = mpsc::channel::<minus_scheduler::JobTrigger>(64);
    let scheduler = Arc::new(Scheduler::new(db.clone(), job_tx));

    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    scheduler.clone().start(shutdown_rx);

    // 12. Agent
    let agent = Arc::new(Agent::new(
        db.clone(),
        config.clone(),
        secrets.clone(),
        providers.clone(),
        tools.clone(),
        skills.clone(),
        policy.clone(),
    ));

    // 12. Integrations registry
    let mut integration_reg = minus_integrations::IntegrationRegistry::new();
    integration_reg.register(Arc::new(minus_integrations::DemoIntegration));
    let integrations = Arc::new(integration_reg);

    // 13. Commands registry
    let mut command_reg = minus_commands::CommandRegistry::new();
    minus_commands::builtin::register_all(&mut command_reg);
    
    // Register commands from integrations
    for integration in integrations.all() {
        for def in integration.commands() {
            command_reg.register(Arc::new(minus_commands::registry::IntegrationCommand {
                integration: integration.clone(),
                def,
            }));
        }
    }
    
    let commands = Arc::new(RwLock::new(command_reg));

    // 14. Build Runtime
    let runtime = Arc::new(Runtime {
        db: db.clone(),
        config: config.clone(),
        config_path: data_dir.config_path(),
        secrets: secrets.clone(),
        vault,
        policy,
        providers,
        tools,
        skills,
        scheduler,
        agent,
        commands,
        shutdown_tx: shutdown_tx.clone(),
    });

    // 14. Print startup banner
    {
        let cfg = config.read().await;
        let provider_display = cfg.provider.default.as_deref().unwrap_or("(none)");
        let model_display = cfg.provider.model.as_deref().unwrap_or("(none)");

        eprintln!("minusbot {}", VERSION);
        eprintln!("data dir: {}", data_dir.root.display());
        eprintln!("database: {}", data_dir.database_url());
        eprintln!("channel: unix");
        eprintln!("provider: {}", provider_display);
        eprintln!("model: {}", model_display);
        eprintln!();
    }

    // 15. Start Unix channel
    let (msg_tx, mut msg_rx) = mpsc::channel::<IncomingMessage>(64);
    let unix_channel = Arc::new(UnixChannel::new(socket_path, msg_tx));

    // Spawn Unix listener
    let chan = unix_channel.clone();
    let chan_handle = tokio::spawn(async move {
        let ctx = ChannelContext {
            channel_id: ChannelId("unix".into()),
        };
        if let Err(e) = chan.start(ctx).await {
            tracing::error!(error = %e, "Unix channel error");
        }
    });

    // 16. Main message loop
    let mut shutdown_rx2 = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            Some(incoming) = msg_rx.recv() => {
                let response = match minus_runtime::Runtime::process_message(runtime.clone(), &incoming).await {
                    Ok(text) => text,
                    Err(e) => format!("Error: {}", e),
                };
                let out = OutgoingMessage::new(incoming.chat_id, response);
                if let Err(e) = unix_channel.send(out).await {
                    tracing::error!(error = %e, "Failed to send response");
                }
            }
            _ = shutdown_rx2.recv() => {
                eprintln!("Shutting down...");
                break;
            }
        }
    }

    chan_handle.abort();
    Ok(())
}
