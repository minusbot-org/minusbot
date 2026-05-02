use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use minus_api::ChatId;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use teloxide::{Bot, dptree};
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
use teloxide::types::{BotCommand, ChatAction, ChatId as TgChatId, Message as TgMessage, ParseMode, Update};
use teloxide::requests::Requester;
use teloxide::payloads::SendMessageSetters;
use rand::{distributions::Alphanumeric, Rng};

pub struct TelegramChannel {
    message_tx: mpsc::Sender<IncomingMessage>,
    secrets: Arc<dyn MinusSecretStore>,
    config: Arc<dyn ConfigProvider>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
    setup_pin: Arc<RwLock<Option<String>>>,
    bot: Arc<RwLock<Option<Bot>>>,
    whitelisted_user_id: Arc<RwLock<Option<Option<String>>>>,
    pending_commands: Arc<RwLock<Option<Vec<CommandDefinition>>>>,
}

impl TelegramChannel {
    pub fn new(
        message_tx: mpsc::Sender<IncomingMessage>,
        config_dir: std::path::PathBuf,
        secrets: Arc<dyn MinusSecretStore>,
    ) -> Self {
        let config_path = config_dir.join("channel-telegram.toml");
        let config = Arc::new(FileConfigProvider::new("channel.telegram", config_path));

        Self {
            message_tx,
            secrets,
            config,
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            setup_pin: Arc::new(RwLock::new(None)),
            bot: Arc::new(RwLock::new(None)),
            whitelisted_user_id: Arc::new(RwLock::new(None)),
            pending_commands: Arc::new(RwLock::new(None)),
        }
    }

    async fn get_token(&self) -> Result<String> {
        let bytes = self.secrets.get_secret("BOT_TOKEN").await?
            .context("Telegram BOT_TOKEN not found in secrets (channel:telegram:BOT_TOKEN)")?;
        Ok(String::from_utf8(bytes)?.trim().to_string())
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn id(&self) -> &'static str { "telegram" }
    fn name(&self) -> &'static str { "Telegram Channel" }

    fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn set_enabled(&self, flag: bool) -> Result<bool> {
        let old = self.enabled.swap(flag, std::sync::atomic::Ordering::Relaxed);
        Ok(old != flag)
    }

    async fn is_ready(&self) -> bool {
        self.bot.read().await.is_some()
    }

    async fn get_active_chat(&self) -> Option<ChatId> {
        None
    }

    async fn set_active_chat(&self, _chat_id: ChatId) -> Result<()> {
        Ok(())
    }

    async fn is_chat_active(&self, chat_id: ChatId) -> bool {
        // Any numeric chat ID is valid for Telegram
        chat_id.0.chars().all(|c| c.is_ascii_digit() || c == '-')
    }

    fn has_available_setup(&self) -> bool {
        true
    }

    async fn setup(&self) -> Result<String> {
        let pin = {
            let mut rng = rand::thread_rng();
            let part1: String = (&mut rng)
                .sample_iter(&Alphanumeric)
                .take(4)
                .map(char::from)
                .collect::<String>()
                .to_uppercase();
            let part2: String = rng
                .sample_iter(&Alphanumeric)
                .take(4)
                .map(char::from)
                .collect::<String>()
                .to_uppercase();
            format!("{}-{}", part1, part2)
        };
        
        let mut setup = self.setup_pin.write().await;
        *setup = Some(pin.clone());
        
        Ok(pin)
    }


    async fn start(&self, _ctx: ChannelContext) -> Result<()> {
        let token = match self.get_token().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Telegram channel could not start: {}", e);
                return Ok(());
            }
        };

        let bot = Bot::new(token);
        {
            let mut b = self.bot.write().await;
            *b = Some(bot.clone());
        }

        // Apply pending commands
        if let Some(cmds) = self.pending_commands.write().await.take() {
            let tg_commands: Vec<BotCommand> = cmds.into_iter()
                .map(|c| BotCommand {
                    command: c.name,
                    description: c.description,
                })
                .collect();
            let _ = bot.set_my_commands(tg_commands).await;
        }

        let message_tx = self.message_tx.clone();
        let setup_pin = self.setup_pin.clone();
        let whitelisted_user_id = self.whitelisted_user_id.clone();
        let config = self.config.clone();
        let enabled = self.enabled.clone();

        tracing::info!("Telegram channel starting...");

        let handler = dptree::entry()
            .branch(
                Update::filter_message()
                    .endpoint(move |bot: Bot, msg: TgMessage| {
                        let message_tx = message_tx.clone();
                        let setup_pin = setup_pin.clone();
                        let whitelisted_user_id = whitelisted_user_id.clone();
                        let config = config.clone();
                        let enabled = enabled.clone();
                        
                        async move {
                            if !enabled.load(std::sync::atomic::Ordering::Relaxed) {
                                return Ok::<(), anyhow::Error>(());
                            }

                            let text = match msg.text() {
                                Some(t) => t,
                                None => return Ok::<(), anyhow::Error>(()),
                            };

                            let user_id = msg.from.as_ref().map(|u| u.id.to_string()).unwrap_or_default();
                            let chat_id = msg.chat.id.to_string();

                            // Handle /link command
                            if text.starts_with("/link") {
                                let parts: Vec<&str> = text.split_whitespace().collect();
                                if let Some(code) = parts.get(1) {
                                    let mut setup = setup_pin.write().await;
                                    if let Some(ref pin) = *setup {
                                        if pin == *code {
                                            *setup = None;
                                            
                                            // Whitelist this user
                                            config.set_config("whitelisted_user_id", &user_id).await.ok();
                                            let mut cache = whitelisted_user_id.write().await;
                                            *cache = Some(Some(user_id.clone()));
                                            
                                            bot.send_message(msg.chat.id, "✨ *Successfully linked!* \nYou are now whitelisted and can use the bot.")
                                                .parse_mode(ParseMode::MarkdownV2)
                                                .await.ok();
                                            return Ok::<(), anyhow::Error>(());
                                        }
                                    }
                                }
                                bot.send_message(msg.chat.id, "❌ *Invalid or expired link code.*")
                                    .parse_mode(ParseMode::MarkdownV2)
                                    .await.ok();
                                return Ok::<(), anyhow::Error>(());
                            }

                            // Check whitelist
                            let is_whitelisted = {
                                let mut cache = whitelisted_user_id.write().await;
                                let val = if let Some(ref v) = *cache {
                                    v.clone()
                                } else {
                                    let v = config.read_config("whitelisted_user_id").await.ok().flatten();
                                    *cache = Some(v.clone());
                                    v
                                };
                                val.map(|id| id == user_id).unwrap_or(false)
                            };

                            if !is_whitelisted {
                                bot.send_message(msg.chat.id, "🔒 *Access Denied*\nPlease use `/link <pin>` to authorize this account.")
                                    .parse_mode(ParseMode::MarkdownV2)
                                    .await.ok();
                                return Ok::<(), anyhow::Error>(());
                            }

                            // Route to daemon
                            let incoming = IncomingMessage::new(
                                ChatId(chat_id),
                                ChannelId("telegram".into()),
                                text.to_string(),
                            );
                            
                            if let Err(e) = message_tx.send(incoming).await {
                                tracing::error!("Failed to route Telegram message to daemon: {}", e);
                            }

                            Ok::<(), anyhow::Error>(())
                        }
                    }),
            );

        Dispatcher::builder(bot, handler)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }

    async fn send_message(&self, packet: MessagePacket) -> Result<()> {
        let bot = self.bot.read().await;
        if let Some(bot) = bot.as_ref() {
            let tg_chat_id: i64 = packet.chat_id.0.parse()
                .context("Invalid Telegram chat ID")?;
            bot.send_message(TgChatId(tg_chat_id), packet.content).await?;
        }
        Ok(())
    }

    async fn send_notification(&self, packet: NotificationPacket) -> Result<()> {
        self.send_message(MessagePacket::new(packet.chat_id, "system", packet.content)).await
    }

    async fn send_tool_call(&self, _packet: ToolCallPacket) -> Result<()> {
        // Tool calls might not be directly representable as simple text in TG easily without formatting
        Ok(())
    }

    async fn send_command_feedback(&self, feedback: CommandFeedback) -> Result<()> {
        let text = if feedback.is_error {
            format!("❌ Error: {}", feedback.result)
        } else {
            feedback.result
        };
        self.send_message(MessagePacket::new(feedback.chat_id, "system", text)).await
    }

    async fn register_commands(&self, commands: Vec<CommandDefinition>) -> Result<()> {
        let bot = self.bot.read().await;
        if let Some(bot) = bot.as_ref() {
            let tg_commands: Vec<BotCommand> = commands.into_iter()
                .map(|c| BotCommand {
                    command: c.name,
                    description: c.description,
                })
                .collect();
            
            bot.set_my_commands(tg_commands).await?;
        } else {
            let mut pending = self.pending_commands.write().await;
            *pending = Some(commands);
        }
        Ok(())
    }

    async fn set_typing(&self, chat_id: ChatId, flag: bool) -> Result<()> {
        if !flag { return Ok(()); } // Telegram typing expires automatically
        
        let bot = self.bot.read().await;
        if let Some(bot) = bot.as_ref() {
            let tg_chat_id: i64 = chat_id.0.parse()
                .context("Invalid Telegram chat ID")?;
            bot.send_chat_action(TgChatId(tg_chat_id), ChatAction::Typing).await?;
        }
        Ok(())
    }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        Some(self.config.clone())
    }
}
