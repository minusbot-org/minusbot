pub mod builtin;
pub mod parser;
pub mod registry;

pub use minus_api::{ArgSpec, CommandSpec, FlagSpec, ValueType};
pub use parser::{
    is_slash_command, parse_slash_command, CommandParseError, ParsedCommand, ParsedInvocation,
};
pub use registry::{CommandMetadata, CommandRegistry, SpecCommand};
