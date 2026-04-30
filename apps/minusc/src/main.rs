use anyhow::Result;
use minus_env::DataDir;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use colored::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize, Deserialize)]
struct UnixRequest {
    pub chat_id: String,
    pub content: String,
}

struct AppState {
    pub current_chat_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = DataDir::resolve()?;
    let socket_path = data_dir.root.join("minusd.sock");

    if !socket_path.exists() {
        eprintln!("{}", "Error: minusbot daemon is not running.".red());
        std::process::exit(1);
    }

    let stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} Failed to connect to {}", "Error:".red().bold(), socket_path.display());
            eprintln!("{} Is the minusd daemon running?", "Hint:".yellow().bold());
            eprintln!("\nCaused by:\n    {}", e);
            std::process::exit(1);
        }
    };

    let (reader, mut writer) = stream.into_split();
    let state = Arc::new(Mutex::new(AppState {
        current_chat_id: "chat-1".into(),
    }));

    let state_clone = state.clone();
    
    // Request initial chat history
    {
        let s = state.lock().unwrap();
        let initial_req = UnixRequest {
            chat_id: s.current_chat_id.clone(),
            content: format!("/chat switch {}", s.current_chat_id),
        };
        let json = serde_json::to_string(&initial_req)?;
        writer.write_all(format!("{}\n", json).as_bytes()).await?;
    }

    // Task to read from server and print to stdout
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // Handle special commands from server
            if line.starts_with("SWITCH_CHAT_ID:") {
                let new_id = line.strip_prefix("SWITCH_CHAT_ID:").unwrap();
                let mut s = state_clone.lock().unwrap();
                s.current_chat_id = new_id.to_string();
                println!("{} {}", "Switched to chat:".green(), new_id.bold());
                continue;
            }
            if line.starts_with("NEW_CHAT_ID:") {
                let new_id = line.strip_prefix("NEW_CHAT_ID:").unwrap();
                let mut s = state_clone.lock().unwrap();
                s.current_chat_id = new_id.to_string();
                println!("{} {}", "Created and switched to:".green(), new_id.bold());
                continue;
            }

            // Clear the current prompt line and print the message
            print!("\r\x1b[2K"); // Carriage return and clear line
            
            if line.starts_with("\x1b[") {
                // Pre-formatted history or colored message
                println!("{}", line);
            } else {
                println!("\x1b[1;35mbot\x1b[0m  \x1b[90m›\x1b[0m {}", line);
            }
            
            // Re-print prompt - this is handled by rustyline after we return to it
            // but for async messages we might need to nudge the user or just let it be.
            // In a better UI we would use a library like `tui-rs`.
        }
    });

    let mut rl = DefaultEditor::new()?;
    
    loop {
        let chat_id = {
            let s = state.lock().unwrap();
            s.current_chat_id.clone()
        };
        
        let prompt = format!("\x1b[1;34muser\x1b[0m (\x1b[1;32m{}\x1b[0m) \x1b[90m›\x1b[0m ", chat_id);
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
                
                let req = UnixRequest {
                    chat_id: chat_id.clone(),
                    content: trimmed.to_string(),
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
