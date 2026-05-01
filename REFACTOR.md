# REFACTOR.md — Minusbot Codebase Audit & Proposal

> **Date**: 2026-05-01  
> **Scope**: Full codebase audit of the `minusbot` workspace  
> **Status**: Proposal — awaiting review

---

## Table of Contents

1. [Current Architecture Map](#1-current-architecture-map)
2. [Dependency Graph Analysis](#2-dependency-graph-analysis)
3. [Issues Found](#3-issues-found)
4. [Potentially Unused / WIP Features](#4-potentially-unused--wip-features)
5. [Refactoring Proposals](#5-refactoring-proposals)
6. [Proposed New Crate Layout](#6-proposed-new-crate-layout)
7. [Migration Order](#7-migration-order)

---

## 1. Current Architecture Map

The workspace has **22 members** organized in 4 top-level directories:

```
apps/
  minusd          — daemon binary (336 LOC) - wires everything, runs message loop
  minusc          — CLI client binary (339 LOC) - connects via Unix/TCP socket

crates/
  minus-api       — types, traits, events, permissions (800 LOC) ← the REAL contract
  minus-core      — 1 line: `pub use minus_api::*;` ← HOLLOW PROXY
  minus-db        — SQLite persistence (954 LOC)
  minus-env       — AppConfig, SecretsManager, DataDir (520 LOC)
  minus-vault     — AES-GCM encrypted secret storage (308 LOC)
  minus-agent     — Conversation loop + tool calling (356 LOC)
  minus-runtime   — Runtime struct, trait impls for API (309 LOC)
  minus-providers — ProviderRegistry (140 LOC)
  minus-channels  — ChannelRegistry (43 LOC)
  minus-commands  — CommandRegistry + parser + 12 builtins (~700 LOC)
  minus-tools     — ToolRegistry + 3 builtin tool groups (~650 LOC)
  minus-skills    — SkillManager + Markdown loader (247 LOC)
  minus-scheduler — Scheduler + cron/delay/interval (509 LOC)
  minus-policy    — PolicyEngine (136 LOC)
  minus-memory    — MemoryStore trait + SqliteMemoryStore (109 LOC) ← DEAD?
  minus-integrations — IntegrationRegistry + DemoIntegration (146 LOC)
  minus-addons    — AddonManager (38 LOC)
  minus-lua       — Stub (24 LOC)

channels/
  minus-channel-cli — CLI/socket channel (376 LOC)

providers/
  minus-provider-openai      — OpenAI-compatible provider (260 LOC)
  minus-provider-openrouter  — Wraps openai with different URL (57 LOC)
```

**Total Rust LOC**: ~7,574 across 71 files

---

## 2. Dependency Graph Analysis

### What depends on what

```
minus-api  ← THE CONTRACT (zero internal deps)
   │
minus-core ← hollow re-export of minus-api
   │
   ├── minus-db, minus-policy, minus-scheduler, minus-memory, minus-skills,
   │   minus-tools, minus-providers, minus-channels, minus-integrations,
   │   minus-addons, minus-lua, minus-commands, minus-agent
   │
   └── minus-runtime (depends on 16 crates)
          │
          └── minusd (depends on 18 crates)
```

### Key dependency observations

| Problem | Details |
|---------|---------|
| **`minus-core` is hollow** | It's literally `pub use minus_api::*;`. 21 crates depend on it, but it adds zero value. It exists only as an indirection layer that makes the dependency graph confusing. |
| **`minus-core` ≠ `minus-api` but they're the same thing** | Some crates use `minus_core::*`, others use `minus_api::traits::*`. This creates inconsistency — you can't tell which is the "canonical" import. |
| **`minus-runtime` depends on 16 crates** | It's a God Object that implements ~7 API traits by delegating to inner fields. |
| **`minus-agent` depends on 9 concrete crates** | Instead of depending on API traits, it takes `Database`, `ProviderRegistry`, `ToolRegistry`, etc. directly. |
| **`minus-channel-cli` depends on `minus-core` AND `minus-db`** | The channel shouldn't need to know about the concrete DB. This couples the channel to implementation details. |

---

## 3. Issues Found

### 🔴 Critical — Architecture

#### 3.1 `minus-core` is a useless indirection
```rust
// crates/minus-core/src/lib.rs — THE ENTIRE FILE
pub use minus_api::*;
```
Every crate that depends on `minus-core` could (and should) depend on `minus-api` directly. `minus-core` adds a compilation unit, a Cargo.toml, and conceptual confusion for zero benefit.

> **Action**: Kill `minus-core`. Replace all `minus_core::` imports with `minus_api::`.

#### 3.2 `Agent` holds concrete types, not API traits
```rust
// crates/minus-agent/src/agent.rs
pub struct Agent {
    db: Arc<Database>,          // ← concrete type from minus-db
    config: Arc<RwLock<AppConfig>>,  // ← concrete type from minus-env
    secrets: Arc<RwLock<SecretsManager>>, // ← concrete
    vault: Option<Arc<Vault>>,   // ← concrete from minus-vault
    providers: Arc<RwLock<ProviderRegistry>>, // ← concrete from minus-providers
    tools: Arc<RwLock<ToolRegistry>>,   // ← concrete from minus-tools
    skills: Arc<SkillManager>,   // ← concrete from minus-skills
    policy: Arc<PolicyEngine>,   // ← concrete from minus-policy
    scheduler: Arc<Scheduler>,   // ← concrete from minus-scheduler
}
```
This means `minus-agent` depends on 9 concrete crates. If you ever want a second Agent implementation or test the agent in isolation, you can't — it's coupled to everything.

> **Action**: Agent should take `Arc<dyn MinusDatabase>`, `Arc<dyn MinusProviders>`, etc. from `minus-api`. Move missing trait abstractions into `minus-api`.

#### 3.3 `Runtime` is a God Object
`Runtime` holds ~15 fields and implements 7 different API traits (`MinusDatabase`, `MinusScheduler`, `MinusChannels`, `MinusSecrets`, `MinusProviders`, `MinusConfigRegistry`, `MinusTools`), but each impl just delegates to an inner field. This is classic "mediator bloat".

> **Action**: Instead of `Runtime` implementing all traits, pass the already-trait-implementing components directly into `CommandContext`. The individual crates already implement the traits (`Database` implements `MinusDatabase`, `Scheduler` implements `MinusScheduler`, etc).

#### 3.4 Duplicate struct definitions
`minus-db` has its own row structs (`ChatRecord`, `MessageRecord`, `MemoryRecord`) and then `minus-api` has its own DTO structs (`Chat`, `Message`, `Memory`). The `MinusDatabase` impl on `Database` manually maps field-by-field between them:
```rust
// crates/minus-db/src/db.rs — 80 lines of boilerplate mapping
async fn list_chats(&self) -> Result<Vec<minus_api::Chat>> {
    let records = self.list_chats().await?;
    Ok(records.into_iter().map(|r| minus_api::Chat {
        id: r.id,
        channel_id: r.channel_id,
        // ...same fields copied one by one
    }).collect())
}
```
> **Action**: Make `minus-db` use `minus-api` types directly in its queries (via `sqlx::FromRow` on the API types), or implement `From<DbRecord> for ApiType`.

### 🟡 Moderate — Consolidation opportunities

#### 3.5 Three nearly identical registries
`ChannelRegistry`, `IntegrationRegistry`, and `AddonManager` are the same pattern:
```rust
pub struct XRegistry {
    items: HashMap<String, Arc<dyn X>>,
}
impl XRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, item: Arc<dyn X>) { ... }
    pub fn get(&self, id: &str) -> Option<Arc<dyn X>> { ... }
    pub fn list(&self) -> Vec<(String, String)> { ... }
}
```

| Crate | Total LOC | Unique logic |
|-------|-----------|-------------|
| `minus-channels` | 43 | 0 |
| `minus-integrations/registry.rs` | 54 | `all_tools()` method |
| `minus-addons` | 38 | 0 |

> **Action**: Either create a generic `Registry<T>` in `minus-api`, or absorb these into `minus-runtime` which already manages them via `HashMap<String, Arc<dyn Channel>>` etc.

#### 3.6 `minus-providers` vs `minus-api::MinusProviders`
`ProviderRegistry` is 140 LOC and already implements `MinusProviders`. But `Runtime` wraps it in another `MinusProviders` impl that adds config persistence. This means there are two competing `MinusProviders` impls that can confuse consumers.

> **Action**: `ProviderRegistry` should be the single impl. Add config persistence to it directly (accept a config writer callback), or keep Runtime's wrapper but remove the one on ProviderRegistry.

### 🟢 Minor — Cleanup

#### 3.7 Inconsistent import style
Some builtins use `minus_api::traits::{Command, CommandContext, CommandDefinition}`, others use `minus_api::{CommandDefinition, traits::{Command, CommandContext}}`. The commands crate depends on both `minus-core` and `minus-api`:
```toml
# minus-commands/Cargo.toml
minus-core = { path = "../minus-core" }
minus-api = { path = "../minus-api" }
```
And uses different imports in different files. This will get worse over time.

#### 3.8 `#[async_trait]` declared twice
```rust
// channels/minus-channel-cli/src/channel.rs line 169-170
#[async_trait]
#[async_trait]  // ← duplicate
impl Channel for CliChannel {
```

#### 3.9 `minusd::main` has ~100 lines of wiring code
The daemon's `main()` manually constructs 15+ objects and wires them together. This should be a `Runtime::builder()` or `RuntimeBuilder`.

---

## 4. Potentially Unused / WIP Features

> ⚠️ These look unused but may be WIP. Marking for awareness, not deletion.

| Item | Status | Evidence |
|------|--------|---------|
| **`minus-memory` crate** | ⚠️ Likely dead | Has its own `MemoryStore` trait and `SqliteMemoryStore` with `add_memory`, `search_memory` methods. But the Agent uses `minus-db` memory methods directly (`get_important_memories`, `save_memory`). `MemoryItem` in `minus-memory` has `tags: Vec<String>` — the DB memory system uses `kind`/`brief` instead. **The two memory systems are parallel and incompatible.** `minus-memory` is listed in runtime deps but I couldn't find any usage of `MemoryStore` trait or `SqliteMemoryStore` anywhere outside the crate itself. |
| **`minus-lua` crate** | ⚠️ Stub | Explicitly documented as a placeholder: _"Lua scripting support (stubbed for MVP)"_. The `LuaRuntime` struct does nothing. No usage anywhere. |
| **`minus-addons`** | ⚠️ Minimal | `AddonManager` stores `AddonManifest`s but nothing registers or queries addons at runtime. The manifest type exists in `minus-api`, the manager is 38 LOC with no consumers. |
| **`RuntimeEvent` enum** | ⚠️ Unused | Defined in `minus-api/src/events.rs` with variants like `MessageReceived`, `ToolCalled`, `ProviderError` — but nothing emits or subscribes to these events. |
| **`AddonRegistry` struct** (in traits.rs) | ⚠️ Unused | Defined in `minus-api/src/traits.rs` with `tools`, `integrations`, `commands` fields but never used anywhere. Different from `AddonManager` in `minus-addons`. |
| **`SubVault`** | ⚠️ Unused | Defined in `minus-vault/src/subvault.rs`, provides prefix-based isolation. `RuntimeSecretStore` in runtime.rs does the same thing manually. |
| **`SkillDefinition` / `SkillCall` / `SkillResult`** (API types) | ⚠️ Unused | Defined in `minus-api` but never used. The actual skills system uses `SkillMeta` from `minus-skills`. |
| **`DemoIntegration`** | 🟡 Testing | Registered in `minusd` but only useful for testing the integration system. |
| **`Policy` checks** | 🟡 Stubbed | Most `PolicyEngine::evaluate()` branches return `Allow`. It's wired in but doesn't actually restrict anything yet. |

---

## 5. Refactoring Proposals

### Phase 1: Kill `minus-core` (low risk, high clarity)

**Goal**: Eliminate the hollow proxy crate.

1. In every `Cargo.toml` that depends on `minus-core`, replace with `minus-api`
2. In every `.rs` file, replace `use minus_core::` with `use minus_api::`
3. Delete `crates/minus-core/`
4. Remove from workspace `Cargo.toml`

**Impact**: 21 files changed, 0 behavior change.

### Phase 2: Expand `minus-api` as the single contract layer

**Goal**: Everything that channels, providers, and commands need should come from `minus-api` alone. No downstream crate should need to import concrete implementation crates.

Missing abstractions to add to `minus-api::traits`:

```rust
// Currently missing — Agent has no trait
#[async_trait]
pub trait MinusAgent: Send + Sync {
    async fn handle_message(&self, msg: &IncomingMessage, channel: Arc<dyn Channel>) -> Result<String>;
}

// Currently missing — Skills has no trait  
#[async_trait]
pub trait MinusSkills: Send + Sync {
    async fn list(&self) -> Result<Vec<SkillDefinition>>;
    async fn load_for_chat(&self, chat_id: &str, name: &str) -> Result<String>;
    async fn unload_from_chat(&self, chat_id: &str, name: &str) -> Result<String>;
    async fn get_loaded_content(&self, chat_id: &str) -> Result<Vec<String>>;
}

// Currently missing — Vault has no trait
#[async_trait]
pub trait MinusVault: Send + Sync {
    fn get_secret(&self, key: &str) -> Result<Vec<u8>>;
    fn put_secret(&self, key: &str, value: &[u8]) -> Result<()>;
    fn has_secret(&self, key: &str) -> bool;
    fn list_keys(&self) -> Vec<String>;
}

// Currently missing — Policy has no trait
pub trait MinusPolicy: Send + Sync {
    fn evaluate(&self, action: &PolicyAction) -> PolicyDecision;
}
```

### Phase 3: Decouple `minus-agent` from concrete types

**Goal**: `minus-agent` should depend on `minus-api` only (plus `serde_json`, `uuid`, etc.).

```rust
// AFTER refactor
pub struct Agent {
    db: Arc<dyn MinusDatabase>,
    providers: Arc<dyn MinusProviders>,
    tools: Arc<dyn MinusTools>,
    skills: Arc<dyn MinusSkills>,
    policy: Arc<dyn MinusPolicy>,
    vault: Option<Arc<dyn MinusVault>>,
    scheduler: Arc<dyn MinusScheduler>,
    // config values extracted at construction time, not the whole AppConfig
    agent_config: AgentConfig,  // move this struct to minus-api
}
```

**Impact**: `minus-agent/Cargo.toml` goes from 9 internal deps to 1 (`minus-api`).

### Phase 4: Consolidate registries

**Goal**: Merge `minus-channels`, `minus-integrations`, `minus-addons` into `minus-runtime` (or a new `minus-registries` crate).

These three crates are <150 LOC combined and all follow the exact same `HashMap<String, Arc<dyn T>>` pattern. They don't deserve to be separate compilation units.

### Phase 5: Remove `minus-channel-cli`'s dependency on `minus-db`

The CLI channel imports `minus_db::Database` to call `get_messages()` during `on_chat_switch`. Instead:

1. Add a `get_messages` method to `CommandContext` or pass message history through the `Channel` trait
2. Or have the runtime fetch history and pass it to the channel via the existing `send_message` mechanism

### Phase 6: `RuntimeBuilder` pattern

Replace the 100-line manual wiring in `minusd::main` with:

```rust
let runtime = Runtime::builder()
    .config(data_dir.config_path())
    .database(&data_dir.database_url())
    .vault(data_dir.vault_dir())
    .provider(OpenAiProvider::new(...))
    .provider(OpenRouterProvider::new(...))
    .channel(CliChannel::new(...))
    .integration(DemoIntegration)
    .build()
    .await?;

runtime.run().await?;
```

---

## 6. Proposed New Crate Layout

After all phases, the workspace would look like:

```
apps/
  minusd              — thin binary, uses RuntimeBuilder
  minusc              — CLI client (unchanged)

crates/
  minus-api           — ALL types, traits, events, permissions (expanded)
  minus-db            — Database impl of MinusDatabase
  minus-env           — AppConfig, SecretsManager, DataDir
  minus-vault         — Vault impl of MinusVault
  minus-agent         — Agent impl of MinusAgent (depends on minus-api only)
  minus-runtime       — Runtime, RuntimeBuilder, registries, wiring
  minus-commands      — CommandRegistry + builtins (depends on minus-api only)
  minus-tools         — ToolRegistry + builtins
  minus-skills        — SkillManager
  minus-scheduler     — Scheduler impl of MinusScheduler
  minus-policy        — PolicyEngine impl of MinusPolicy

channels/
  minus-channel-cli   — depends on minus-api only

providers/
  minus-provider-openai
  minus-provider-openrouter
```

**Eliminated crates** (6):
- `minus-core` — hollow re-export
- `minus-channels` — 43 LOC registry, absorbed into runtime
- `minus-integrations` — registry absorbed into runtime, `DemoIntegration` → example or test
- `minus-addons` — 38 LOC, unused
- `minus-memory` — dead parallel system
- `minus-lua` — empty stub

**Net result**: 22 → 16 workspace members. Cleaner dependency graph. Channels and providers depend only on `minus-api`.

---

## 7. Migration Order

> **Important**: Each phase should be a separate PR that compiles and passes tests before moving to the next.

| Priority | Phase | Risk | Effort | Compile breaks |
|----------|-------|------|--------|---------------|
| **P0** | Fix duplicate `#[async_trait]` in cli channel | None | 5 min | 0 |
| **P1** | Kill `minus-core` → replace with `minus-api` | Low | 1-2h | Many files, mechanical |
| **P2** | Expand `minus-api` with missing traits | Low | 2-3h | 0 (additive) |
| **P3** | Decouple `minus-agent` from concrete types | Medium | 3-4h | Agent + Runtime |
| **P4** | Consolidate registries into runtime | Low | 1-2h | Imports only |
| **P5** | Remove `minus-memory` / `minus-lua` / `minus-addons` | Low | 30 min | Remove deps |
| **P6** | Decouple `minus-channel-cli` from `minus-db` | Medium | 2h | Channel + Runtime |
| **P7** | `RuntimeBuilder` pattern | Medium | 3-4h | minusd main |

---

## Appendix: Duplicate/Dead Code Specific Locations

### Duplicate `CliRequest` struct
Defined in **both**:
- `channels/minus-channel-cli/src/channel.rs:16`
- `apps/minusc/src/main.rs:18`

Same for `CliPacket`. Should be in `minus-api` or a shared `minus-channel-cli-protocol` module.

### Duplicate API key resolution logic
The pattern of resolving `PROVIDER_<ID>_API_KEY` from vault → fallback to secrets is duplicated in:
- `crates/minus-agent/src/agent.rs:202-214`
- `crates/minus-runtime/src/runtime.rs:260-272`

Should be a single utility function or method on the Provider trait.

### `AddonRegistry` vs `AddonManager`
- `minus-api::traits::AddonRegistry` — struct with `tools`, `integrations`, `commands` fields
- `minus-addons::AddonManager` — struct with `manifests` HashMap

Two different things with confusingly similar names. Neither is used.
