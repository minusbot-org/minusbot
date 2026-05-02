use anyhow::Result;
use minus_env::DataDir;
use rustyline::error::ReadlineError;

use rustyline::ExternalPrinter;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use colored::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::str::FromStr;

// Import packets from our shared types (or re-define if they are part of the protocol)
// For simplicity in the CLI, we can re-define them or use the ones from minus-api if it's a dependency.
// minusc depends on minus-env which might depend on minus-core/api.
use minus_api::types::*;

#[derive(Debug, Serialize, Deserialize)]
struct CliRequest {
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CliPacket {
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

struct AppState {
    pub current_chat_id: String,
}

#[derive(PartialEq)]
enum Protocol {
    #[cfg(unix)]
    Unix,
    Tcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = DataDir::resolve()?;
    
    // Parse arguments
    let mut args = std::env::args().skip(1);
    
    #[cfg(unix)]
    let mut protocol = Protocol::Unix;
    #[cfg(not(unix))]
    let mut protocol = Protocol::Tcp;
    
    let mut host = "127.0.0.1:10000".to_string();
    let mut secret_override: Option<String> = None;
    
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--protocol" | "-p" => {
                if let Some(val) = args.next() {
                    match val.to_lowercase().as_str() {
                        "tcp" => protocol = Protocol::Tcp,
                        "unix" => {
                            #[cfg(unix)]
                            { protocol = Protocol::Unix; }
                            #[cfg(not(unix))]
                            { eprintln!("Unix sockets not supported on this OS"); std::process::exit(1); }
                        }
                        _ => { eprintln!("Unknown protocol: {}", val); std::process::exit(1); }
                    }
                }
            }
            "--host" | "-h" => {
                if let Some(val) = args.next() {
                    host = val;
                }
            }
            "--secret" | "-s" => {
                if let Some(val) = args.next() {
                    secret_override = Some(val);
                }
            }
            _ => {}
        }
    }
    
    let secret = if protocol == Protocol::Tcp {
        if let Some(s) = secret_override {
            Some(s)
        } else {
            let pass_file = data_dir.root.join(".tcp_socket_passwd");
            if pass_file.exists() {
                Some(std::fs::read_to_string(&pass_file).unwrap_or_default().trim().to_string())
            } else {
                None
            }
        }
    } else {
        None
    };

    let (reader, mut writer): (
        Box<dyn AsyncRead + Unpin + Send>,
        Box<dyn AsyncWrite + Unpin + Send>,
    ) = match protocol {
        #[cfg(unix)]
        Protocol::Unix => {
            let socket_path = data_dir.root.join("minusd.sock");
            if !socket_path.exists() {
                eprintln!(
                    "{}",
                    "Error: minusbot daemon is not running (socket missing).".red()
                );
                std::process::exit(1);
            }
            let stream = match tokio::net::UnixStream::connect(&socket_path).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "{} Failed to connect to {}",
                        "Error:".red().bold(),
                        socket_path.display()
                    );
                    eprintln!("{} Is the minusd daemon running?", "Hint:".yellow().bold());
                    eprintln!("\nCaused by:\n    {}", e);
                    std::process::exit(1);
                }
            };
            let (r, w) = tokio::io::split(stream);
            (Box::new(r), Box::new(w))
        }
        Protocol::Tcp => {
            let addr = SocketAddr::from_str(&host).unwrap_or_else(|_| {
                eprintln!("{} Invalid host format: {}", "Error:".red().bold(), host);
                std::process::exit(1);
            });
            let stream = match tokio::net::TcpStream::connect(addr).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{} Failed to connect to TCP {}", "Error:".red().bold(), addr);
                    eprintln!(
                        "{} Is the minusd daemon running on that port?",
                        "Hint:".yellow().bold()
                    );
                    eprintln!("\nCaused by:\n    {}", e);
                    std::process::exit(1);
                }
            };
            let (r, w) = tokio::io::split(stream);
            (Box::new(r), Box::new(w))
        }
    };

    let state = Arc::new(Mutex::new(AppState {
        current_chat_id: "chat-cli".into(),
    }));

    // Print startup banner
    {
        let config_raw = minus_env::AppConfig::load(&data_dir.config_path()).unwrap_or_default();
        let provider_display = config_raw.provider.default.as_deref().unwrap_or("(none)");
        
        let mut model_display = config_raw.provider.text_model.clone().unwrap_or_else(|| "(none)".into());
        if let Some(id) = &config_raw.provider.default {
            let provider_cfg_path = data_dir.component_config_path("provider", id);
            if provider_cfg_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&provider_cfg_path) {
                    if let Ok(val) = toml::from_str::<toml::Value>(&content) {
                        if let Some(m) = val.get("text_model").and_then(|v| v.as_str()) {
                            model_display = m.to_string();
                        }
                    }
                }
            }
        }
        let version = "0.1.0";

        let proto_str = match protocol {
            #[cfg(unix)]
            Protocol::Unix => "Unix Socket",
            Protocol::Tcp => "TCP",
        };

        println!();
        println!("\x1b[36m      ██    ██    \x1b[0m");
        println!("\x1b[36m      ██    ██    \x1b[0m");
        println!("\x1b[36m     ██████████   \x1b[0m");
        println!("\x1b[36m    ███ ████ ███  \x1b[0m  \x1b[1;35mMinusc v{}\x1b[0m", version);
        println!("\x1b[36m     ██████████   \x1b[0m  CLI Client for Minusbot");
        println!("\x1b[36m       ██████     \x1b[0m");
        println!("\x1b[36m      ███████     \x1b[0m");
        println!("\x1b[36m       ██  ██     \x1b[0m");
        println!();
        
        println!("  \x1b[36mProtocol\x1b[0m   -> \x1b[32m{}\x1b[0m", proto_str);
        if protocol == Protocol::Tcp {
            println!("  \x1b[36mHost\x1b[0m       -> \x1b[32m{}\x1b[0m", host);
        } else {
            #[cfg(unix)]
            println!("  \x1b[36mSocket\x1b[0m     -> \x1b[32m{}\x1b[0m", data_dir.root.join("minusd.sock").display());
        }
        println!("  \x1b[36mAuth\x1b[0m       -> \x1b[32m{}\x1b[0m", if secret.is_some() { "Secret key active" } else { "None" });
        println!("  \x1b[36mProvider\x1b[0m   -> \x1b[33m{}\x1b[0m", provider_display);
        println!("  \x1b[36mModel\x1b[0m      -> \x1b[33m{}\x1b[0m", model_display);
        println!();
    }

    let state_clone = state.clone();
    
    // Request initial chat history
    {
        let s = state.lock().unwrap();
        let initial_req = CliRequest {
            chat_id: s.current_chat_id.clone(),
            content: format!("/chat switch {}", s.current_chat_id),
            secret: secret.clone(),
        };
        let json = serde_json::to_string(&initial_req)?;
        writer.write_all(format!("{}\n", json).as_bytes()).await?;
    }

    let config = rustyline::Config::builder()
        .edit_mode(rustyline::EditMode::Emacs)
        .auto_add_history(true)
        .build();
    let mut rl = rustyline::Editor::<(), _>::with_config(config)?;
    let printer = rl.create_external_printer()?;

    // Task to read from server and print to stdout
    tokio::spawn(async move {
        let mut printer = printer;
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let packet: CliPacket = match serde_json::from_str(&line) {
                Ok(p) => p,
                Err(_) => continue, // Ignore garbage
            };

            match packet {
                CliPacket::Message(p) => {
                    let prefix = match p.role.as_str() {
                        "user" => "\x1b[1;34muser\x1b[0m  \x1b[90m>\x1b[0m".to_string(),
                        "assistant" => "\x1b[1;35mbot\x1b[0m   \x1b[90m>\x1b[0m".to_string(),
                        "system" => "\x1b[1;33msystem\x1b[0m\x1b[90m>\x1b[0m".to_string(),
                        _ => "\x1b[90m>\x1b[0m".to_string(),
                    };
                    let _ = printer.print(format!("{} {}", prefix, p.content));
                }
                CliPacket::Notification(p) => {
                    let label = match p.severity {
                        NotificationSeverity::Info => "[INFO]".blue().bold(),
                        NotificationSeverity::Warning => "[WARN]".yellow().bold(),
                        NotificationSeverity::Error => "[ERROR]".red().bold(),
                        NotificationSeverity::Success => "[OK]".green().bold(),
                    };
                    let _ = printer.print(format!("{} {}", label, p.content));
                }
                CliPacket::ToolCall(p) => {
                    let _ = printer.print(format!("\x1b[1;36m[TOOL]\x1b[0m Calling: \x1b[33m{}\x1b[0m \x1b[90m({})\x1b[0m", p.name, p.brief));
                }
                CliPacket::ChatHistory { chat_id, messages } => {
                    let mut s = state_clone.lock().unwrap();
                    if s.current_chat_id != chat_id.0 {
                        s.current_chat_id = chat_id.0.clone();
                        let _ = printer.print(format!("\n\x1b[1;33m[SYSTEM]\x1b[0m Switched to chat: \x1b[1;32m{}\x1b[0m\n", chat_id.0));
                    }
                    
                    let _ = printer.print(format!("\n\x1b[1;36m--- Chat History ---\x1b[0m"));
                    for m in messages {
                        let prefix = match m.role.as_str() {
                            "user" => "\x1b[1;34muser\x1b[0m".to_string(),
                            "assistant" => "\x1b[1;35mbot\x1b[0m ".to_string(),
                            "system" => "\x1b[1;33msys\x1b[0m  ".to_string(),
                            _ => "\x1b[90m???\x1b[0m  ".to_string(),
                        };
                        let _ = printer.print(format!("{} \x1b[90m[{}]\x1b[0m {}", prefix, m.created_at, m.content));
                    }
                    let _ = printer.print(format!("\x1b[1;36m--------------------\x1b[0m\n"));
                }
                CliPacket::Disconnect { reason, error } => {
                    let mut msg = format!("\n\x1b[1;31m[DISCONNECTED]\x1b[0m Reason: {}", reason);
                    if !error.is_empty() {
                        msg.push_str(&format!(" (Error: {})", error));
                    }
                    let _ = printer.print(msg);
                    std::process::exit(0);
                }
            }
        }
    });
    
    loop {
        let chat_id = {
            let s = state.lock().unwrap();
            s.current_chat_id.clone()
        };
        
        let prompt = format!("\x1b[1;34muser\x1b[0m (\x1b[1;32m{}\x1b[0m) \x1b[90m>\x1b[0m ", chat_id);
        let readline = rl.readline(&prompt);
        
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "/exit" || trimmed == "/quit" {
                    break;
                }
                
                rl.add_history_entry(trimmed)?;
                
                let req = CliRequest {
                    chat_id: chat_id.clone(),
                    content: trimmed.to_string(),
                    secret: secret.clone(),
                };
                
                let json = serde_json::to_string(&req)?;
                if let Err(e) = writer.write_all(format!("{}\n", json).as_bytes()).await {
                    eprintln!("{} {}", "Error:".red(), e);
                    break;
                }
            }
            Err(ReadlineError::Interrupted) => {
                break;
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                eprintln!("{} {:?}", "Error:".red(), err);
                break;
            }
        }
    }

    Ok(())
}
