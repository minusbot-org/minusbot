<div align="center">
  <h1>minusbot</h1>
  <p><strong>A self-hosted autonomous personal AI assistant runtime written in Rust.</strong></p>

  [![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
  [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
  [![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-lightgrey)]()
</div>

---

> **minusbot** is a modular, secure, and extensible orchestrator for your very own local AI assistant. Keep your keys safe, manage permissions explicitly, and let your agent work autonomously while retaining full control.

## 🚀 Quick Start

Get up and running in seconds:

```bash
# Build the project
cargo build --release

# Run the Daemon
cargo run -p minusd

# Connect with the Client (in another terminal)
cargo run -p minusc
```

### What happens on the first run?

When you first launch `minusd`, it automatically sets up your environment:
1. 📁 Creates a data directory at `~/.local/share/minusbot/` (or your OS equivalent).
2. 📝 Generates a default `config.toml`.
3. 🔑 Creates a `secrets.env` file to store your API keys securely.
4. 🗄️ Initializes a local **SQLite database**.
5. 🔐 Generates a development **vault master key**.
6. 📡 Starts the CLI channel for interaction.

---

## ⚙️ Configuration

### Setting up an AI Provider

minusbot supports multiple LLM providers. Here's how to configure OpenRouter:

```bash
minusbot> /env set SECRET_OPENROUTER_API_KEY sk-or-v1-...
minusbot> /providers set-default openrouter
minusbot> /config set provider.model openai/gpt-4o-mini
```

Groq is available as an OpenAI-compatible provider:

```bash
minusbot> /apikey groq gsk_...
minusbot> /providers set groq
minusbot> /models llama-3.1-8b-instant
```

### ⌨️ Available Commands

Interact with minusbot using slash commands. Type `/help` to see them all:

| Category | Commands | Description |
|----------|----------|-------------|
| **Config** | `/config show`, `/config get`, `/config set` | Manage public configurations |
| **Secrets**| `/env list`, `/env set`, `/env get`, `/env unset` | Manage environment variables securely |
| **Vault**  | `/vault list`, `/vault put`, `/vault delete` | Interact with the encrypted secret vault |
| **Providers**| `/providers list`, `/providers set-default` | Switch between AI providers |
| **Skills** | `/skills list`, `/skills load`, `/skills unload` | Manage agent abilities |
| **Jobs**   | `/jobs list`, `/jobs create`, `/jobs cancel` | Schedule and manage background tasks |
| **Debug**  | `/tools list`, `/audit tail` | Developer and debugging tools |

---

## 🏗️ Architecture & Workspaces

minusbot is built as a highly modular monorepo. The core does not depend on concrete implementations, allowing for immense flexibility.

### 📦 Applications (`apps/`)
- **`minusd`** → Main daemon (runs the agent, database, and background services).
- **`minusc`** → Client executable (connects to the daemon via Unix domain sockets).

### 🧠 Core Crates (`crates/`)
- **`minus-core`** → Shared types and foundational traits.
- **`minus-db`** → SQLite persistence layer.
- **`minus-env`** → Configuration and environment management.
- **`minus-vault`** → Encrypted vault for sensitive secrets.
- **`minus-policy`** → Permission and security policy engine.
- **`minus-agent`** → The central conversation agent loop.
- **`minus-runtime`** → Central orchestrator binding everything together.
- **`minus-scheduler`** → Cron and scheduled jobs engine.
- **`minus-memory`** → Memory store (RAG-ready).
- **`minus-skills`** → Markdown-based knowledge files.
- **`minus-tools`** → Built-in agent tool implementations.
- **`minus-providers`** → Provider registry for LLMs.
- **`minus-channels`** → Communication channel registry.
- **`minus-integrations`** → External integrations registry.
- **`minus-addons`** → Dynamic addon system.
- **`minus-commands`** → Slash command parser and router.
- **`minus-lua`** → Lua scripting engine integration.

### 🔌 Providers (`providers/`)
- **`minus-provider-openai`** → Integration for OpenAI APIs.
- **`minus-provider-openrouter`** → Integration for OpenRouter APIs.
- **`minus-provider-groq`** → Integration for Groq's OpenAI-compatible APIs.

### 💬 Channels (`channels/`)
- **`minus-channel-unix`** → Unix domain sockets / CLI channel implementations.

---

## 📂 Data Directory Layout

Your minusbot instance stores data safely in your local share directory:

```text
~/.local/share/minusbot/
├── config.toml        # Public configuration parameters
├── secrets.env        # Private API keys
├── data.sqlite        # Main SQLite database
├── skills/            # Markdown-formatted skill files
├── drive/             # Secure filesystem accessible by the agent
├── vault/secrets/     # Encrypted vault secrets storage
├── logs/              # Daily rotating log files
├── addons/            # Directory for external addons
└── cache/             # Temporary cache directory
```

---

## 🛡️ Security First

minusbot is designed from the ground up to keep your personal data safe:

- 🔒 **Redaction:** Secrets are never printed in full (always redacted).
- 🛑 **LLM Safety:** Secrets are never sent blindly to the LLM.
- ✅ **Explicit Consent:** All vault access requires declarations and explicit approval.
- 🐚 **No rogue shells:** Shell execution is denied by default.
- 📁 **Sandboxed Filesystem:** Write operations are strictly restricted to the `drive/` directory.
- 👮 **Policy Engine:** All agent actions pass through a strict policy engine.
- 📜 **Auditability:** Every system change is fully audited and logged.

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
