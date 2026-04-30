/// Lua scripting support (stubbed for MVP).
///
/// Future implementation will use `mlua` to support:
/// - Lua-scriptable job actions
/// - Lua addon registration (tools, triggers, events)
/// - Lua-based integrations
///
/// For now, this crate exists as a placeholder.

pub struct LuaRuntime;

impl LuaRuntime {
    pub fn new() -> Self {
        tracing::debug!("Lua runtime stub initialized (not yet implemented)");
        Self
    }
}

impl Default for LuaRuntime {
    fn default() -> Self {
        Self::new()
    }
}
