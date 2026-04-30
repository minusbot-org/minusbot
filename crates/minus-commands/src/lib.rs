pub mod registry;
pub mod builtin;
pub mod parser;

pub use parser::{is_slash_command, parse_slash_command};
pub use registry::{CommandRegistry, CommandMetadata};
