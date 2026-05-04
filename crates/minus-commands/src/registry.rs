use minus_api::async_trait;
use minus_api::{Command, CommandDefinition};
use std::collections::HashMap;
use std::sync::Arc;

pub struct CommandRegistry {
    commands: HashMap<String, Arc<dyn Command>>,
    aliases: HashMap<String, String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Arc<dyn Command>) {
        let def = cmd.definition();
        let name = def.name.to_lowercase();

        for alias in &def.aliases {
            self.aliases.insert(alias.to_lowercase(), name.clone());
        }

        self.commands.insert(name, cmd);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Command>> {
        let name_lc = name.to_lowercase();
        let resolved_name = self.aliases.get(&name_lc).unwrap_or(&name_lc);
        self.commands.get(resolved_name).cloned()
    }

    pub fn list(&self) -> Vec<CommandDefinition> {
        self.commands.values().map(|c| c.definition()).collect()
    }
}

// Re-export CommandMetadata if needed or just use CommandDefinition
pub type CommandMetadata = CommandDefinition;

pub struct IntegrationCommand {
    pub integration: Arc<dyn minus_api::Integration>,
    pub def: CommandDefinition,
}

#[async_trait]
impl Command for IntegrationCommand {
    fn definition(&self) -> CommandDefinition {
        self.def.clone()
    }

    async fn execute(
        &self,
        args: Vec<String>,
        ctx: minus_api::CommandContext,
    ) -> anyhow::Result<String> {
        self.integration
            .execute_command(self.def.name.clone(), args, ctx)
            .await
    }
}
