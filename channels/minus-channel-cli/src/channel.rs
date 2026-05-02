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
        messages: Vec<Message> 
    },
    Disconnect {
        reason: String,
        error: String,
    },
}

#[derive(Clone, Debug)]
pub enum CliProtocol {
    #[cfg(unix)]
    Unix(PathBuf),
    Tcp(SocketAddr),
}

/// The CLI channel listens on a Unix domain socket or TCP socket.
pub struct CliChannel {
    message_tx: mpsc::Sender<IncomingMessage>,
    active_streams: Arc<Mutex<Vec<(ChatId, mpsc::Sender<String>)>>>,
    config: Arc<dyn ConfigProvider>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
    active_chat_id: Arc<RwLock<Option<ChatId>>>,
    context: Arc<RwLock<Option<ChannelContext>>>,
}

impl CliChannel {
    pub fn new(
        message_tx: mpsc::Sender<IncomingMessage>,
        config_dir: std::path::PathBuf,
    ) -> Self {
        let config_path = config_dir.join("channel-cli.toml");
        let config = Arc::new(FileConfigProvider::new("channel.cli", config_path));

        Self {
            message_tx,
            active_streams: Arc::new(Mutex::new(Vec::new())),
            config,
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            active_chat_id: Arc::new(RwLock::new(Some(ChatId("cli:default".into())))),
            context: Arc::new(RwLock::new(None)),
        }
    }

    async fn handle_stream<S>(
        stream: S,
        message_tx: mpsc::Sender<IncomingMessage>,
        active_streams: Arc<Mutex<Vec<(ChatId, mpsc::Sender<String>)>>>,
        expected_secret: Option<String>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        let (resp_tx, mut resp_rx) = mpsc::channel::<String>(32);
        let mut current_chat_id = ChatId("cli:default".into());

        // Register this stream initially
        {
            let mut streams = active_streams.lock().await;
            streams.push((current_chat_id.clone(), resp_tx.clone()));
        }

        let active_streams_inner = active_streams.clone();

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
                Err(_) => {
                    CliRequest {
                        chat_id: current_chat_id.0.clone(),
                        content: line,
                        secret: None,
                    }
                }
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
            if message_tx.send(msg).await.is_err() {
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
    fn id(&self) -> &'static str { "cli" }
    fn name(&self) -> &'static str { "CLI Channel" }

    fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn set_enabled(&self, flag: bool) -> Result<bool> {
        let old = self.enabled.swap(flag, std::sync::atomic::Ordering::Relaxed);
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
        self.active_chat_id.read().await.clone()
    }

    async fn set_active_chat(&self, chat_id: ChatId) -> Result<()> {
        let mut active = self.active_chat_id.write().await;
        *active = Some(chat_id);
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

                loop {
                    if !self.is_enabled() {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    match listener.accept().await {
                        Ok((stream, _)) => {
                            let tx = message_tx.clone();
                            let streams = active_streams.clone();
                            let s = secret.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_stream(stream, tx, streams, s).await {
                                    tracing::error!(error = %e, "Unix client error");
                                }
                            });
                        }
                        Err(e) => tracing::error!(error = %e, "Unix listener accept error"),
                    }
                }
            }
            CliProtocol::Tcp(addr) => {
                let listener = TcpListener::bind(addr)
                    .await
                    .with_context(|| format!("Failed to bind to TCP socket: {}", addr))?;

                tracing::info!(addr = %addr, "TCP CLI channel listening");

                loop {
                    if !self.is_enabled() {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    match listener.accept().await {
                        Ok((stream, _)) => {
                            let tx = message_tx.clone();
                            let streams = active_streams.clone();
                            let s = secret.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_stream(stream, tx, streams, s).await {
                                    tracing::error!(error = %e, "TCP client error");
                                }
                            });
                        }
                        Err(e) => tracing::error!(error = %e, "TCP listener accept error"),
                    }
                }
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
        self.broadcast(&chat_id, CliPacket::Notification(packet)).await
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
        self.broadcast(&feedback.chat_id, CliPacket::Message(MessagePacket {
            chat_id: feedback.chat_id.clone(),
            content: text,
            role: "assistant".into(),
            metadata: None,
        })).await
    }

    async fn on_chat_switch(&self, chat_id: &ChatId, messages: Vec<Message>) -> Result<()> {
        let packet = CliPacket::ChatHistory {
            chat_id: chat_id.clone(),
            messages,
        };
        self.broadcast(chat_id, packet).await
    }
}

#[async_trait]
impl ConfigProvider for CliChannel {
    fn id(&self) -> &'static str {
        self.config.id()
    }

    fn list_keys(&self) -> Vec<String> {
        self.config.list_keys()
    }

    async fn read_config(&self, key: &str) -> Result<Option<String>> {
        self.config.read_config(key).await
    }

    async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        self.config.set_config(key, value).await
    }
}
