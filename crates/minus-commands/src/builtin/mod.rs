pub mod help;
pub mod status;
pub mod chat;
pub mod core;
pub mod providers;
pub mod models;
pub mod secrets;
pub mod apikey;
pub mod tools;

use crate::registry::CommandRegistry;
use std::sync::Arc;

pub fn register_all(reg: &mut CommandRegistry) {
    reg.register(Arc::new(help::HelpCommand));
    reg.register(Arc::new(status::StatusCommand));
    reg.register(Arc::new(chat::ChatListCommand));
    reg.register(Arc::new(core::ShutdownCommand));
    reg.register(Arc::new(core::ClearCommand));
    reg.register(Arc::new(providers::ProviderCommand));
    reg.register(Arc::new(models::ModelCommand));
    reg.register(Arc::new(secrets::SecretCommand));
    reg.register(Arc::new(apikey::ApikeyCommand));
    reg.register(Arc::new(tools::ToolsCommand));
}
