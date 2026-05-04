use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::*;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::{mpsc, Mutex, RwLock};

#[derive(Debug, Serialize, Deserialize)]
pub struct CliRequest {
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CliPacket {
    Message(MessagePacket),
    Notification(NotificationPacket),
    ToolCall(ToolCallPacket),
    ChatHistory {
        chat_id: ChatId,
        messages: Vec<Message>,
    },
    Disconnect {
        reason: String,
        error: String,
    },
    Welcome {
        version: String,
        chat_id: String,
        provider: String,
        model: String,
    },
}

#[derive(Clone, Debug)]
pub enum CliProtocol {
    #[cfg(unix)]
    Unix(PathBuf),
    Tcp(SocketAddr),
}

/// The CLI channel listens on a Unix domain socket or TCP socket.
#[derive(Clone)]
pub struct CliChannel {
    message_tx: mpsc::Sender<IncomingMessage>,
    active_streams: Arc<Mutex<Vec<(ChatId, mpsc::Sender<String>)>>>,
    config: Arc<dyn ConfigProvider>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
    active_chat_id: Arc<RwLock<Option<ChatId>>>,
    context: Arc<RwLock<Option<ChannelContext>>>,
}

impl CliChannel {
    pub fn new(message_tx: mpsc::Sender<IncomingMessage>, config_dir: std::path::PathBuf) -> Self {
        let config_path = config_dir.join("channel-cli.toml");
        let config = Arc::new(FileConfigProvider::new("channel.cli", config_path));

        Self {
            message_tx,
            active_streams: Arc::new(Mutex::new(Vec::new())),
            config,
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            active_chat_id: Arc::new(RwLock::new(None)),
            context: Arc::new(RwLock::new(None)),
        }
    }

    async fn handle_stream<S>(
        &self,
        stream: S,
        expected_secret: Option<String>,
        provider_display: String,
        model_display: String,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        let (resp_tx, mut resp_rx) = mpsc::channel::<String>(32);
        let mut current_chat_id = self.get_active_chat().await.unwrap_or(ChatId("chat-cli".into()));

        // Register this stream initially
        {
            let mut streams = self.active_streams.lock().await;
            streams.push((current_chat_id.clone(), resp_tx.clone()));
        }

        let welcome = CliPacket::Welcome {
            version: "0.1.0".to_string(),
            chat_id: current_chat_id.0.clone(),
            provider: provider_display,
            model: model_display,
        };
        let _ = writer.write_all(format!("{}\n", serde_json::to_string(&welcome).unwrap()).as_bytes()).await;

        let active_streams_inner = self.active_streams.clone();

        // Spawn a task to send responses back to the client
        tokio::spawn(async move {
            while let Some(msg) = resp_rx.recv().await {
                if let Err(e) = writer.write_all(format!("{}\n", msg).as_bytes()).await {
                    tracing::error!(error = %e, "Failed to write to socket");
                    break;
                }
            }
        });

        // Read messages from client
        while let Ok(Some(line)) = lines.next_line().await {
            let req: CliRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(_) => CliRequest {
                    chat_id: current_chat_id.0.clone(),
                    content: line,
                    secret: None,
                },
            };

            // Check secret if expected
            if let Some(ref exp) = expected_secret {
                if req.secret.as_deref() != Some(exp.as_str()) {
                    tracing::warn!("Invalid secret from client");
                    continue;
                }
            }

            // If chat_id changed, update our registration
            if req.chat_id != current_chat_id.0 {
                let mut streams = active_streams_inner.lock().await;
                streams
                    .retain(|(cid, tx)| !(cid.0 == current_chat_id.0 && tx.same_channel(&resp_tx)));

                current_chat_id = ChatId(req.chat_id.clone());
                streams.push((current_chat_id.clone(), resp_tx.clone()));
            }

            let msg = IncomingMessage::new(
                current_chat_id.clone(),
                ChannelId("cli".into()),
                req.content,
            );
            if self.message_tx.send(msg).await.is_err() {
                break;
            }
        }

        // Final cleanup
        let mut streams = active_streams_inner.lock().await;
        streams.retain(|(cid, tx)| !(cid.0 == current_chat_id.0 && tx.same_channel(&resp_tx)));

        Ok(())
    }

    async fn broadcast(&self, chat_id: &ChatId, packet: CliPacket) -> Result<()> {
        let json = serde_json::to_string(&packet)?;
        let streams = self.active_streams.lock().await;
        for (cid, tx) in streams.iter() {
            if cid.0 == chat_id.0 {
                let _ = tx.send(json.clone()).await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Channel for CliChannel {
    fn id(&self) -> &'static str {
        "cli"
    }
    fn name(&self) -> &'static str {
        "CLI Channel"
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn set_enabled(&self, flag: bool) -> Result<bool> {
        let old = self
            .enabled
            .swap(flag, std::sync::atomic::Ordering::Relaxed);
        if old != flag {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn is_ready(&self) -> bool {
        true
    }

    async fn get_active_chat(&self) -> Option<ChatId> {
        let mut cache = self.active_chat_id.write().await;
        if cache.is_none() {
            if let Ok(Some(id)) = self.config.read_config("active_chat_id").await {
                if id.is_empty() {
                    let default_id = format!("chat-{}", minus_api::Channel::id(self));
                    *cache = Some(ChatId(default_id.clone()));
                    let _ = self.config.set_config("active_chat_id", &default_id).await;
                } else {
                    *cache = Some(ChatId(id));
                }
            } else {
                let default_id = format!("chat-{}", minus_api::Channel::id(self));
                *cache = Some(ChatId(default_id.clone()));
                let _ = self.config.set_config("active_chat_id", &default_id).await;
            }
        }
        cache.clone()
    }

    async fn set_active_chat(&self, chat_id: ChatId) -> Result<()> {
        let mut active = self.active_chat_id.write().await;
        *active = Some(chat_id.clone());
        self.config.set_config("active_chat_id", &chat_id.0).await?;
        Ok(())
    }

    async fn is_chat_active(&self, chat_id: ChatId) -> bool {
        // Check active_chat_id
        if let Some(active) = self.active_chat_id.read().await.as_ref() {
            if active.0 == chat_id.0 {
                return true;
            }
        }

        // Check active streams
        let streams = self.active_streams.lock().await;
        streams.iter().any(|(cid, _)| cid.0 == chat_id.0)
    }

    async fn start(&self, ctx: ChannelContext) -> Result<()> {
        {
            let mut context = self.context.write().await;
            *context = Some(ctx.clone());
        }

        let mut provider_display = "(none)".to_string();
        let mut model_display = "(none)".to_string();
        
        let config_path = ctx.config_dir.parent().unwrap().join("config.toml");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(val) = toml::from_str::<toml::Value>(&content) {
                if let Some(p) = val.get("provider").and_then(|v| v.as_table()) {
                    if let Some(d) = p.get("default").and_then(|v| v.as_str()) {
                        provider_display = d.to_string();
                    }
                    if let Some(m) = p.get("text_model").and_then(|v| v.as_str()) {
                        model_display = m.to_string();
                    }
                }
            }
        }
        
        if provider_display != "(none)" {
            let provider_cfg_path = ctx.config_dir.join(format!("provider-{}.toml", provider_display));
            if let Ok(content) = std::fs::read_to_string(&provider_cfg_path) {
                if let Ok(val) = toml::from_str::<toml::Value>(&content) {
                    if let Some(m) = val.get("text_model").and_then(|v| v.as_str()) {
                        model_display = m.to_string();
                    }
                }
            }
        }

        let message_tx = self.message_tx.clone();
        let active_streams = self.active_streams.clone();

        let is_windows = cfg!(target_os = "windows");

        let (protocol, secret): (CliProtocol, Option<String>) = if is_windows {
            let port = 10000;
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            let pass_file = ctx.config_dir.parent().unwrap().join(".tcp_socket_passwd");
            let sec = if pass_file.exists() {
                std::fs::read_to_string(&pass_file)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                use rand::{distributions::Alphanumeric, Rng};
                let s: String = rand::thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(32)
                    .map(char::from)
                    .collect();
                let _ = std::fs::write(&pass_file, &s);
                s
            };
            (CliProtocol::Tcp(addr), Some(sec))
        } else {
            #[cfg(unix)]
            {
                let socket_path = ctx.config_dir.parent().unwrap().join("minusd.sock");
                (CliProtocol::Unix(socket_path), None)
            }
            #[cfg(not(unix))]
            unreachable!()
        };

        match protocol {
            #[cfg(unix)]
            CliProtocol::Unix(socket_path) => {
                if socket_path.exists() {
                    std::fs::remove_file(&socket_path)?;
                }

                if let Some(parent) = socket_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let listener = UnixListener::bind(&socket_path).with_context(|| {
                    format!("Failed to bind to unix socket: {}", socket_path.display())
                })?;

                tracing::info!(path = %socket_path.display(), "Unix CLI channel listening");

                let mut shutdown = ctx.shutdown.subscribe();
                loop {
                    if !self.is_enabled() {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    tokio::select! {
                        res = listener.accept() => {
                            match res {
                                Ok((stream, _)) => {
                                    let s = secret.clone();
                                    let c = self.clone();
                                    let p = provider_display.clone();
                                    let m = model_display.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = c.handle_stream(stream, s, p, m).await {
                                            tracing::error!(error = %e, "Unix client error");
                                        }
                                    });
                                }
                                Err(e) => tracing::error!(error = %e, "Unix listener accept error"),
                            }
                        }
                        _ = shutdown.recv() => {
                            tracing::info!("Unix CLI channel shutting down...");
                            break;
                        }
                    }
                }
                Ok(())
            }
            CliProtocol::Tcp(addr) => {
                let listener = TcpListener::bind(addr)
                    .await
                    .with_context(|| format!("Failed to bind to TCP socket: {}", addr))?;

                tracing::info!(addr = %addr, "TCP CLI channel listening");

                let mut shutdown = ctx.shutdown.subscribe();
                loop {
                    if !self.is_enabled() {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    tokio::select! {
                        res = listener.accept() => {
                            match res {
                                Ok((stream, _)) => {
                                    let s = secret.clone();
                                    let c = self.clone();
                                    let p = provider_display.clone();
                                    let m = model_display.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = c.handle_stream(stream, s, p, m).await {
                                            tracing::error!(error = %e, "TCP client error");
                                        }
                                    });
                                }
                                Err(e) => tracing::error!(error = %e, "TCP listener accept error"),
                            }
                        }
                        _ = shutdown.recv() => {
                            tracing::info!("TCP CLI channel shutting down...");
                            break;
                        }
                    }
                }
                Ok(())
            }
        }
    }

    async fn stop(&self) -> Result<()> {
        let packet = CliPacket::Disconnect {
            reason: "daemon_shutdown".into(),
            error: "".into(),
        };
        let json = serde_json::to_string(&packet)?;
        let streams = self.active_streams.lock().await;
        for (_, tx) in streams.iter() {
            let _ = tx.send(json.clone()).await;
        }
        Ok(())
    }

    async fn send_message(&self, packet: MessagePacket) -> Result<()> {
        let chat_id = packet.chat_id.clone();
        self.broadcast(&chat_id, CliPacket::Message(packet)).await
    }

    async fn send_notification(&self, packet: NotificationPacket) -> Result<()> {
        let chat_id = packet.chat_id.clone();
        self.broadcast(&chat_id, CliPacket::Notification(packet))
            .await
    }

    async fn send_tool_call(&self, packet: ToolCallPacket) -> Result<()> {
        let chat_id = packet.chat_id.clone();
        self.broadcast(&chat_id, CliPacket::ToolCall(packet)).await
    }

    async fn send_command_feedback(&self, feedback: CommandFeedback) -> Result<()> {
        let text = if feedback.is_error {
            format!("Error executing /{}: {}", feedback.command, feedback.result)
        } else {
            feedback.result
        };
        self.broadcast(
            &feedback.chat_id,
            CliPacket::Message(MessagePacket {
                chat_id: feedback.chat_id.clone(),
                content: text,
                role: "assistant".into(),
                metadata: None,
            }),
        )
        .await
    }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        Some(Arc::new(self.clone()))
    }

    async fn on_chat_switch(&self, old_chat_id: &ChatId, new_chat_id: &ChatId, messages: Vec<Message>) -> Result<()> {
        let packet = CliPacket::ChatHistory {
            chat_id: new_chat_id.clone(),
            messages,
        };
        self.broadcast(old_chat_id, packet).await
    }
}

#[async_trait]
impl ConfigProvider for CliChannel {
    fn id(&self) -> &'static str {
        self.config.id()
    }

    fn list_keys(&self) -> Vec<String> {
        let mut keys = self.config.list_keys();
        if !keys.contains(&"active_chat_id".to_string()) {
            keys.push("active_chat_id".to_string());
        }
        keys
    }

    async fn read_config(&self, key: &str) -> Result<Option<String>> {
        if key == "active_chat_id" {
            return Ok(self.get_active_chat().await.map(|id| id.0));
        }
        self.config.read_config(key).await
    }

    async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        if key == "active_chat_id" {
            self.set_active_chat(ChatId(value.to_string())).await?;
            return Ok(());
        }
        self.config.set_config(key, value).await
    }
}
