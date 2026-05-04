use std::collections::HashMap;

use minus_api::{CommandSpec, ValueType};

pub fn is_slash_command(input: &str) -> bool {
    input.trim().starts_with('/')
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedInvocation {
    pub name: String,
    pub resolved_name: String,
    pub args: Vec<String>,
    pub subcommand_path: Vec<String>,
    pub positionals: Vec<String>,
    pub flags: HashMap<String, Option<String>>,
}

#[derive(Debug, Clone)]
pub enum CommandParseError {
    NotSlashCommand,
    UnknownCommand {
        name: String,
        available: Vec<String>,
    },
    UnknownSubcommand {
        command: String,
        subcommand: String,
        help: String,
    },
    MissingArgument {
        command: String,
        argument: String,
        help: String,
    },
    InvalidArgument {
        command: String,
        argument: String,
        expected: ValueType,
        value: String,
        help: String,
    },
    UnknownFlag {
        command: String,
        flag: String,
        help: String,
    },
    MissingFlagValue {
        command: String,
        flag: String,
        help: String,
    },
    UnexpectedArgument {
        command: String,
        argument: String,
        help: String,
    },
}

impl CommandParseError {
    pub fn render(&self) -> String {
        match self {
            CommandParseError::NotSlashCommand => "Not a slash command.".to_string(),
            CommandParseError::UnknownCommand { name, available } => {
                if available.is_empty() {
                    format!("Unknown command: /{}", name)
                } else {
                    format!(
                        "Unknown command: /{}\n\nAvailable commands:\n{}",
                        name,
                        available
                            .iter()
                            .map(|cmd| format!("  /{}", cmd))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            CommandParseError::UnknownSubcommand {
                command,
                subcommand,
                help,
            } => format!(
                "Unknown subcommand for /{}: {}\n\n{}",
                command, subcommand, help
            ),
            CommandParseError::MissingArgument {
                command,
                argument,
                help,
            } => format!(
                "Missing required argument for /{}: {}\n\n{}",
                command, argument, help
            ),
            CommandParseError::InvalidArgument {
                command,
                argument,
                expected,
                value,
                help,
            } => format!(
                "Invalid argument for /{}: {} expected {:?}, got '{}'\n\n{}",
                command, argument, expected, value, help
            ),
            CommandParseError::UnknownFlag {
                command,
                flag,
                help,
            } => format!("Unknown flag for /{}: {}\n\n{}", command, flag, help),
            CommandParseError::MissingFlagValue {
                command,
                flag,
                help,
            } => format!("Missing value for /{} flag: {}\n\n{}", command, flag, help),
            CommandParseError::UnexpectedArgument {
                command,
                argument,
                help,
            } => format!(
                "Unexpected argument for /{}: {}\n\n{}",
                command, argument, help
            ),
        }
    }
}

pub fn parse_slash_command(input: &str) -> ParsedCommand {
    let input = input.trim();
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd_with_slash = parts[0].to_lowercase();
    let name = cmd_with_slash
        .strip_prefix('/')
        .unwrap_or(&cmd_with_slash)
        .to_string();

    let rest = parts.get(1).map(|s| s.trim()).unwrap_or("");
    let args = if rest.is_empty() {
        Vec::new()
    } else {
        parse_quoted_args(rest)
    };

    ParsedCommand { name, args }
}

fn parse_quoted_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                if in_quotes {
                    args.push(current.clone());
                    current.clear();
                    in_quotes = false;
                } else {
                    in_quotes = true;
                }
            }
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }
    args
}

pub fn parse_flags_and_positionals(
    args: &[String],
    spec: &CommandSpec,
) -> Result<(HashMap<String, Option<String>>, Vec<String>), CommandParseError> {
    let mut flags = HashMap::new();
    let mut positionals = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if let Some(name_value) = arg.strip_prefix("--") {
            let (name, inline_value) = if let Some((name, value)) = name_value.split_once('=') {
                (name, Some(value.to_string()))
            } else {
                (name_value, None)
            };
            let flag = spec
                .flags
                .iter()
                .find(|flag| flag.name == name)
                .ok_or_else(|| CommandParseError::UnknownFlag {
                    command: spec.name.clone(),
                    flag: format!("--{}", name),
                    help: spec.render_help(&spec.name),
                })?;

            if flag.takes_value {
                let value = if let Some(value) = inline_value {
                    value
                } else {
                    i += 1;
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| CommandParseError::MissingFlagValue {
                            command: spec.name.clone(),
                            flag: format!("--{}", name),
                            help: spec.render_help(&spec.name),
                        })?
                };
                validate_value(&value, &flag.value_type).map_err(|_| {
                    CommandParseError::InvalidArgument {
                        command: spec.name.clone(),
                        argument: format!("--{}", name),
                        expected: flag.value_type.clone(),
                        value: value.clone(),
                        help: spec.render_help(&spec.name),
                    }
                })?;
                flags.insert(flag.name.clone(), Some(value));
            } else {
                flags.insert(flag.name.clone(), None);
            }
        } else {
            positionals.push(arg.clone());
        }
        i += 1;
    }

    validate_positionals(&positionals, spec)?;
    Ok((flags, positionals))
}

fn validate_positionals(
    positionals: &[String],
    spec: &CommandSpec,
) -> Result<(), CommandParseError> {
    let allows_variadic = spec.args.iter().any(|arg| arg.variadic);
    if !allows_variadic && positionals.len() > spec.args.len() {
        return Err(CommandParseError::UnexpectedArgument {
            command: spec.name.clone(),
            argument: positionals[spec.args.len()].clone(),
            help: spec.render_help(&spec.name),
        });
    }

    for (index, arg) in spec.args.iter().enumerate() {
        if arg.required && positionals.get(index).is_none() {
            return Err(CommandParseError::MissingArgument {
                command: spec.name.clone(),
                argument: arg.name.clone(),
                help: spec.render_help(&spec.name),
            });
        }

        if let Some(value) = positionals.get(index) {
            validate_value(value, &arg.value_type).map_err(|_| {
                CommandParseError::InvalidArgument {
                    command: spec.name.clone(),
                    argument: arg.name.clone(),
                    expected: arg.value_type.clone(),
                    value: value.clone(),
                    help: spec.render_help(&spec.name),
                }
            })?;
        }

        if arg.variadic {
            break;
        }
    }

    Ok(())
}

fn validate_value(value: &str, value_type: &ValueType) -> Result<(), ()> {
    match value_type {
        ValueType::String => Ok(()),
        ValueType::Integer => value.parse::<i64>().map(|_| ()).map_err(|_| ()),
        ValueType::Float => value.parse::<f64>().map(|_| ()).map_err(|_| ()),
        ValueType::Boolean => match value {
            "true" | "false" | "1" | "0" | "yes" | "no" => Ok(()),
            _ => Err(()),
        },
    }
}
