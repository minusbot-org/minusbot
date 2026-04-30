# minusbot

A self-hosted autonomous personal AI assistant runtime written in Rust.

## Quick Start

```bash
# Build the project
cargo build -p minusd

# Run the CLI
cargo run -p minusd
```

On first run, minusbot will:
1. Create a data directory at `~/.local/share/minusbot/` (Linux) or equivalent
2. Generate a default `config.toml`
3. Create a `secrets.env` file for API keys
4. Initialize the SQLite database
5. Generate a development vault master key
6. Start the CLI channel

## Configuration

### Setting up a provider

```
minusbot> /env set SECRET_OPENROUTER_API_KEY sk-or-v1-...
minusbot> /providers set-default openrouter
minusbot> /config set provider.model openai/gpt-4.1-mini
```

### Available commands

Type `/help` in the CLI for a full list of commands:

- **Config:** `/config show`, `/config get`, `/config set`
- **Secrets:** `/env list`, `/env set`, `/env get`, `/env unset`
- **Vault:** `/vault list`, `/vault put`, `/vault delete`
- **Providers:** `/providers list`, `/providers set-default`
- **Skills:** `/skills list`, `/skills load`, `/skills unload`, `/skills search`
- **Jobs:** `/jobs list`, `/jobs create`, `/jobs cancel`
- **Debug:** `/tools list`, `/audit tail`

## Architecture

minusbot is a modular monorepo. The core does not depend on concrete implementations:

```
minus-core          → Shared types, traits
minus-db            → SQLite persistence
minus-env           → Config + secrets management
minus-vault         → Encrypted secrets vault
minus-policy        → Permission/policy engine
minus-agent         → Conversation agent loop
minus-runtime       → Central orchestrator
minus-scheduler     → Scheduled jobs
minus-memory        → Memory store (RAG-ready)
minus-skills        → Markdown knowledge files
minus-tools         → Built-in tool implementations
minus-providers     → Provider registry
minus-provider-*    → Concrete LLM providers
minus-channels      → Channel registry
minus-channel-cli   → CLI stdin/stdout channel
minus-integrations  → Integration registry
minus-addons        → Addon system
minus-cli           → Slash command parser
minus-lua           → Lua scripting (stub)
minusd              → Main executable
```

## Data Directory

```
~/.local/share/minusbot/
├── config.toml        # Public configuration
├── secrets.env        # Private API keys
├── data.sqlite        # SQLite database
├── skills/            # Markdown skill files
├── drive/             # Agent-accessible filesystem
├── vault/secrets/     # Encrypted vault secrets
├── logs/              # Daily log files
├── addons/            # Addon directory
└── cache/             # Cache directory
```

## Security

- Secrets are never printed in full (always redacted)
- Secrets are never sent to the LLM
- All vault access requires declarations and approval
- Shell execution is denied by default
- Filesystem writes are restricted to the `drive/` directory
- All actions pass through the policy engine
- All changes are audited

## License

MIT
