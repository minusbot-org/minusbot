use minus_api::Tool;

mod chat;
mod memory;
mod schedule;

pub use chat::*;
pub use memory::*;
pub use schedule::*;

/// Returns all built-in tool instances.
pub fn all_builtin_tools() -> Vec<Box<dyn Tool>> {
    vec![
        // Memory tools
        Box::new(MemoryWriteTool),
        Box::new(MemoryReadTool),
        Box::new(MemorySearchTool),
        Box::new(MemoryDeleteTool),
        
        // Chat tools
        Box::new(ChatListTool),
        Box::new(ChatReadTool),
        Box::new(ChatSearchTool),
        
        // Schedule tools
        Box::new(ScheduleCreateTool),
        Box::new(ScheduleListTool),
        Box::new(ScheduleDeleteTool),
        Box::new(ScheduleUpdateTool),
    ]
}
