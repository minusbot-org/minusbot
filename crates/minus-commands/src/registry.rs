use crate::parser::{
    parse_flags_and_positionals, parse_slash_command, CommandParseError, ParsedInvocation,
};
use minus_api::async_trait;
use minus_api::{Command, CommandContext, CommandDefinition, CommandSpec};
use std::collections::HashMap;
use std::sync::Arc;

pub struct CommandRegistry {
    commands: HashMap<String, Arc<dyn Command>>,
    aliases: HashMap<String, String>,
    specs: HashMap<String, CommandSpec>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            aliases: HashMap::new(),
            specs: HashMap::new(),
        }
    }

    pub fn register(&mut self, spec: CommandSpec) {
        let name = spec.name.to_lowercase();

        for alias in &spec.command_aliases {
            self.aliases.insert(alias.to_lowercase(), name.clone());
        }

        let cmd = Arc::new(SpecCommand { spec: spec.clone() });
        self.specs.insert(name.clone(), spec);
        self.commands.insert(name, cmd);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Command>> {
        let name_lc = name.to_lowercase();
        let resolved_name = self.aliases.get(&name_lc).unwrap_or(&name_lc);
        self.commands.get(resolved_name).cloned()
    }

    pub fn parse_invocation(&self, input: &str) -> Result<ParsedInvocation, CommandParseError> {
        if !crate::parser::is_slash_command(input) {
            return Err(CommandParseError::NotSlashCommand);
        }

        let parsed = parse_slash_command(input);
        let resolved_name =
            self.resolve_name(&parsed.name)
                .ok_or_else(|| CommandParseError::UnknownCommand {
                    name: parsed.name.clone(),
                    available: self.command_names(),
                })?;

        let Some(spec) = self.specs.get(&resolved_name) else {
            return Ok(ParsedInvocation {
                name: parsed.name,
                resolved_name,
                args: parsed.args.clone(),
                subcommand_path: Vec::new(),
                positionals: parsed.args,
                flags: HashMap::new(),
            });
        };

        let (subcommand_path, remaining_args, active_spec) =
            self.resolve_subcommands(spec, &parsed.args, &resolved_name)?;
        let (flags, positionals) = parse_flags_and_positionals(remaining_args, active_spec)?;

        Ok(ParsedInvocation {
            name: parsed.name,
            resolved_name,
            args: parsed.args,
            subcommand_path,
            positionals,
            flags,
        })
    }

    pub fn list(&self) -> Vec<CommandDefinition> {
        self.commands.values().map(|c| c.definition()).collect()
    }

    pub fn spec(&self, name: &str) -> Option<&CommandSpec> {
        let resolved = self.resolve_name(name)?;
        self.specs.get(&resolved)
    }

    fn resolve_name(&self, name: &str) -> Option<String> {
        let name_lc = name.to_lowercase();
        if self.commands.contains_key(&name_lc) {
            return Some(name_lc);
        }
        self.aliases.get(&name_lc).cloned()
    }

    fn command_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys().cloned().collect();
        names.sort();
        names
    }

    fn resolve_subcommands<'a>(
        &self,
        root: &'a CommandSpec,
        args: &'a [String],
        command_name: &str,
    ) -> Result<(Vec<String>, &'a [String], &'a CommandSpec), CommandParseError> {
        let mut active = root;
        let mut path = Vec::new();
        let mut index = 0;

        while let Some(next) = args.get(index) {
            if active.subcommands.is_empty() {
                break;
            }

            if next.starts_with('-') {
                break;
            }

            if let Some(subcommand) = active.subcommands.iter().find(|subcommand| {
                subcommand.name == *next || subcommand.aliases.iter().any(|alias| alias == next)
            }) {
                active = subcommand;
                path.push(next.clone());
                index += 1;
                continue;
            }

            if active.allow_unknown_subcommands {
                break;
            }

            return Err(CommandParseError::UnknownSubcommand {
                command: command_name.to_string(),
                subcommand: next.clone(),
                help: active.render_help(command_name),
            });
        }

        Ok((path, &args[index..], active))
    }
}

// Re-export CommandMetadata if needed or just use CommandDefinition
pub type CommandMetadata = CommandDefinition;

pub struct SpecCommand {
    pub spec: CommandSpec,
}

#[async_trait]
impl Command for SpecCommand {
    fn spec(&self) -> CommandSpec {
        self.spec.clone()
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> anyhow::Result<String> {
        let (active, remaining) = resolve_spec_command(&self.spec, &args);
        if let Some(handler) = &active.handler {
            return handler(remaining.to_vec(), ctx).await;
        }
        Ok(active.render_help(&self.spec.name))
    }
}

fn resolve_spec_command<'a>(
    root: &'a CommandSpec,
    args: &'a [String],
) -> (&'a CommandSpec, &'a [String]) {
    let mut active = root;
    let mut index = 0;

    while let Some(next) = args.get(index) {
        if let Some(subcommand) = active.subcommands.iter().find(|subcommand| {
            subcommand.name == *next || subcommand.aliases.iter().any(|alias| alias == next)
        }) {
            active = subcommand;
            index += 1;
        } else {
            break;
        }
    }

    (active, &args[index..])
}

pub struct IntegrationCommand {
    pub integration: Arc<dyn minus_api::Integration>,
    pub def: CommandDefinition,
}

#[async_trait]
impl Command for IntegrationCommand {
    fn spec(&self) -> CommandSpec {
        let mut spec = CommandSpec::new(
            self.def.name.clone(),
            self.def.usage.clone(),
            self.def.description.clone(),
        )
        .category(self.def.category.clone())
        .min_args(self.def.min_args);
        for alias in &self.def.aliases {
            spec = spec.command_alias(alias.clone());
        }
        spec
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandSpec;

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        let spec = CommandSpec::new("llm", "/llm provider(s) <args>", "Manage LLM")
            .strict_subcommands()
            .subcommand(
                CommandSpec::new("provider", "/llm provider [id]", "Provider ops")
                    .alias("providers"),
            );
        registry.register(spec);
        registry
    }

    #[test]
    fn parses_llm_without_args() {
        let invocation = registry().parse_invocation("/llm").unwrap();
        assert_eq!(invocation.resolved_name, "llm");
        assert!(invocation.args.is_empty());
    }

    #[test]
    fn detects_unknown_command() {
        let err = registry().parse_invocation("/missing").unwrap_err();
        assert!(matches!(err, CommandParseError::UnknownCommand { .. }));
    }

    #[test]
    fn detects_unknown_root_subcommand() {
        let err = registry().parse_invocation("/llm wat").unwrap_err();
        assert!(matches!(err, CommandParseError::UnknownSubcommand { .. }));
        assert!(err.render().contains("Subcommands"));
    }
}
