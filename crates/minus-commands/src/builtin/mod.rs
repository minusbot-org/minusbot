pub mod apikey;
pub mod channels;
pub mod chat;
pub mod core;
pub mod help;
pub mod llm;
pub mod memory;
pub mod scheduler;
pub mod secrets;
pub mod status;
pub mod tools;

use crate::registry::CommandRegistry;

pub fn register_all(reg: &mut CommandRegistry) {
    reg.register(help::spec());
    reg.register(status::spec());
    reg.register(chat::spec());
    reg.register(core::shutdown_spec());
    reg.register(core::clear_spec());
    reg.register(llm::spec());
    reg.register(secrets::spec());
    reg.register(apikey::spec());
    reg.register(tools::spec());
    reg.register(memory::spec());
    reg.register(scheduler::spec());
    reg.register(channels::spec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandParseError;

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        register_all(&mut registry);
        registry
    }

    #[test]
    fn all_builtin_commands_have_specs() {
        let registry = registry();
        for command in registry.list() {
            assert!(
                registry.spec(&command.name).is_some(),
                "missing spec for /{}",
                command.name
            );
        }
    }

    #[test]
    fn validates_required_args_for_apikey() {
        let err = registry()
            .parse_invocation("/apikey openrouter")
            .unwrap_err();
        assert!(matches!(err, CommandParseError::MissingArgument { .. }));
    }

    #[test]
    fn validates_unknown_subcommands_for_channels() {
        let err = registry().parse_invocation("/channels nope").unwrap_err();
        assert!(matches!(err, CommandParseError::UnknownSubcommand { .. }));
    }

    #[test]
    fn validates_typed_args_for_chat_read() {
        let err = registry()
            .parse_invocation("/chat read chat-1 not-a-number")
            .unwrap_err();
        assert!(matches!(err, CommandParseError::InvalidArgument { .. }));
    }

    #[test]
    fn accepts_chat_rm_alias_without_listing_it_as_subcommand() {
        let registry = registry();
        let invocation = registry.parse_invocation("/chat rm chat-1").unwrap();
        assert_eq!(invocation.subcommand_path, vec!["rm"]);
        assert_eq!(invocation.positionals, vec!["chat-1"]);

        let help = registry.spec("chat").unwrap().render_help("chat");
        assert!(help.contains("delete"));
        assert!(!help.contains("rm"));
    }

    #[test]
    fn rejects_old_llm_provider_config_shape() {
        let err = registry()
            .parse_invocation("/llm provider test config")
            .unwrap_err();
        assert!(matches!(err, CommandParseError::UnexpectedArgument { .. }));
    }

    #[test]
    fn accepts_active_provider_config_shape() {
        let invocation = registry().parse_invocation("/llm config a b").unwrap();
        assert_eq!(invocation.subcommand_path, vec!["config"]);
        assert_eq!(invocation.positionals, vec!["a", "b"]);
    }

    #[test]
    fn accepts_plural_llm_aliases_without_listing_them_as_commands() {
        let registry = registry();
        let providers = registry.parse_invocation("/llm providers").unwrap();
        assert_eq!(providers.subcommand_path, vec!["providers"]);

        let models = registry.parse_invocation("/llm models").unwrap();
        assert_eq!(models.subcommand_path, vec!["models"]);

        let help = registry.spec("llm").unwrap().render_help("llm");
        assert!(help.contains("provider"));
        assert!(help.contains("model"));
        assert!(!help.contains("/llm providers"));
        assert!(!help.contains("/llm models"));
    }
}
