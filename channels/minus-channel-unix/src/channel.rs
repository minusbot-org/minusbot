use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_core::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Serialize, Deserialize)]
struct UnixRequest {
    pub chat_id: String,
    pub content: String,
}

/// The Unix channel listens on a Unix domain socket.
pub struct UnixChannel {
    socket_path: PathBuf,
    message_tx: mpsc::Sender<IncomingMessage>,
    active_streams: Arc<Mutex<Vec<(ChatId, mpsc::Sender<String>)>>>,
}

impl UnixChannel {
    pub fn new(socket_path: PathBuf, message_tx: mpsc::Sender<IncomingMessage>) -> Self {
        Self {
            socket_path,
            message_tx,
            active_streams: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn handle_client(
        stream: UnixStream,
        message_tx: mpsc::Sender<IncomingMessage>,
        active_streams: Arc<Mutex<Vec<(ChatId, mpsc::Sender<String>)>>>,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let (resp_tx, mut resp_rx) = mpsc::channel::<String>(32);
        let mut current_chat_id = ChatId("unix:default".into());

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
                    tracing::error!(error = %e, "Failed to write to unix socket");
                    break;
                }
            }
        });

        // Read messages from client
        while let Ok(Some(line)) = lines.next_line().await {
            let req: UnixRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(_) => {
                    // Fallback to raw line if not JSON
                    UnixRequest {
                        chat_id: current_chat_id.0.clone(),
                        content: line,
                    }
                }
            };

            // If chat_id changed, update our registration
            if req.chat_id != current_chat_id.0 {
                let mut streams = active_streams_inner.lock().await;
                // Remove old registration
                streams.retain(|(cid, tx)| !(cid.0 == current_chat_id.0 && tx.same_channel(&resp_tx)));
                
                current_chat_id = ChatId(req.chat_id.clone());
                // Add new registration
                streams.push((current_chat_id.clone(), resp_tx.clone()));
            }

            let msg = IncomingMessage::new(
                current_chat_id.clone(),
                ChannelId("unix".into()),
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
}

#[async_trait]
impl Channel for UnixChannel {
    fn id(&self) -> &'static str {
        "unix"
    }

    fn name(&self) -> &'static str {
        "Unix Domain Socket"
    }

    async fn start(&self, _ctx: ChannelContext) -> Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("Failed to bind to unix socket: {}", self.socket_path.display()))?;
        
        tracing::info!(path = %self.socket_path.display(), "Unix channel listening");

        let message_tx = self.message_tx.clone();
        let active_streams = self.active_streams.clone();

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx = message_tx.clone();
                    let streams = active_streams.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, tx, streams).await {
                            tracing::error!(error = %e, "Unix client error");
                        }
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "Unix listener accept error");
                }
            }
        }
    }

    async fn send(&self, msg: OutgoingMessage) -> Result<()> {
        let streams = self.active_streams.lock().await;
        for (cid, tx) in streams.iter() {
            if cid.0 == msg.chat_id.0 {
                let _ = tx.send(msg.content.clone()).await;
            }
        }
        Ok(())
    }
}
