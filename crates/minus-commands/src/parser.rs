/// Check if a message is a slash command.
pub fn is_slash_command(input: &str) -> bool {
    input.trim().starts_with('/')
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub name: String,
    pub args: Vec<String>,
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
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
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
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}
