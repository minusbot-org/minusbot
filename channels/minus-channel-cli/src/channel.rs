use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_core::*;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Serialize, Deserialize)]
pub struct CliRequest {
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub secret: Option<String>,
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
}

impl CliChannel {
    pub fn new(message_tx: mpsc::Sender<IncomingMessage>, config_dir: std::path::PathBuf) -> Self {
        let config_path = config_dir.join("channel-cli.toml");
        let config = Arc::new(FileConfigProvider::new("channel.cli", config_path));

        Self {
            message_tx,
            active_streams: Arc::new(Mutex::new(Vec::new())),
            config,
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
                    // Fallback to raw line if not JSON
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
                // Remove old registration
                streams
                    .retain(|(cid, tx)| !(cid.0 == current_chat_id.0 && tx.same_channel(&resp_tx)));

                current_chat_id = ChatId(req.chat_id.clone());
                // Add new registration
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

    async fn handle_notification(&self, chat_id: &ChatId, meta: &serde_json::Value) -> Result<()> {
        let n_kind = meta
            .get("notification_kind")
            .and_then(|k| k.as_str())
            .unwrap_or("unknown");
        let data = meta.get("data").and_then(|d| d.as_str()).unwrap_or("");

        match n_kind {
            "switch_chat" | "new_chat" => {
                let is_new = n_kind == "new_chat";
                // Send a marker to the CLI client
                let marker = format!("NOTIFICATION:{}:{}", n_kind, data);
                let streams = self.active_streams.lock().await;
                for (cid, tx) in streams.iter() {
                    if cid.0 == chat_id.0 {
                        let _ = tx.send(marker.clone()).await;
                    }
                }

                // Automatically request history for the new/switched chat
                let tx_msg = self.message_tx.clone();
                let switch_id = data.to_string();
                tokio::spawn(async move {
                    let req = IncomingMessage::new(
                        ChatId(switch_id),
                        ChannelId("cli".into()),
                        if is_new {
                            "/chat read 1"
                        } else {
                            "/chat read 10"
                        },
                    );
                    let _ = tx_msg.send(req).await;
                });
            }
            _ => {
                tracing::info!(kind = %n_kind, "Unhandled notification in CLI channel");
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

    async fn start(&self, ctx: ChannelContext) -> Result<()> {
        let message_tx = self.message_tx.clone();
        let active_streams = self.active_streams.clone();

        let is_windows = cfg!(target_os = "windows");

        // Default values
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

        // TODO: Read cli.toml from ctx.config_dir to override defaults if present

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

    async fn send_warning(&self, chat_id: &ChatId, msg: &str) -> Result<()> {
        let text = format!("\x1b[33mWarning:\x1b[0m {}", msg);
        let mut out = OutgoingMessage::new(chat_id.clone(), text);
        out.metadata = Some(serde_json::json!({"kind": "formatted"}));
        self.send(out).await
    }

    async fn send_error(&self, chat_id: &ChatId, msg: &str) -> Result<()> {
        let text = format!("\x1b[31mError:\x1b[0m {}", msg);
        let mut out = OutgoingMessage::new(chat_id.clone(), text);
        out.metadata = Some(serde_json::json!({"kind": "formatted"}));
        self.send(out).await
    }

    async fn send(&self, msg: OutgoingMessage) -> Result<()> {
        // Route to specific handler if kind is specified
        if let Some(meta) = &msg.metadata {
            if let Some(kind) = meta.get("kind").and_then(|k| k.as_str()) {
                if kind == "notification" {
                    return self.handle_notification(&msg.chat_id, meta).await;
                }

                // Avoid recursion: if it's already formatted, just send it
                if kind != "formatted" {
                    if kind == "warning" {
                        let raw_msg = msg
                            .content
                            .strip_prefix("WARNING: ")
                            .unwrap_or(&msg.content);
                        return self.send_warning(&msg.chat_id, raw_msg).await;
                    } else if kind == "error" {
                        let raw_msg = msg.content.strip_prefix("ERROR: ").unwrap_or(&msg.content);
                        return self.send_error(&msg.chat_id, raw_msg).await;
                    }
                }
            }
        }

        let streams = self.active_streams.lock().await;
        for (cid, tx) in streams.iter() {
            if cid.0 == msg.chat_id.0 {
                let _ = tx.send(msg.content.clone()).await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl minus_api::traits::ConfigProvider for CliChannel {
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
