pub mod builtin;
pub mod parser;
pub mod registry;

pub use parser::{is_slash_command, parse_slash_command};
pub use registry::{CommandMetadata, CommandRegistry};
