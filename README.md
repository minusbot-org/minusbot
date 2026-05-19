# Minusbot 🤖

**Minusbot** is a powerful, secure, and modular AI Agent platform built with **Bun**, **TypeScript**, and **Docker**. It transforms LLMs into autonomous assistants capable of managing files, executing code, and integrating with third-party services—all while keeping your sensitive data protected.

---

## 🎨 Dashboard
Experience a state-of-the-art management interface with:
- **Rich Aesthetics**: Deep dark mode with glassmorphism and vibrant accents.
- **Real-time Telemetry**: Monitor token usage, message exchange, and efficiency heuristics.
- **Full Control**: Manage users, vaults, and update system channels (Stable/Nightly) directly from the browser.

## 🚀 Key Features

### 🌐 Multi-Channel Communication
Minusbot isn't bound to one interface. Connect your AI to:
- **Telegram Bot**: Native bridge with file support.
- **Discord Bot**: Slash command support and channel syncing.
- **Web Chat**: Modern, real-time messaging interface.

### �️ Isolated Environments (Workspaces)
Safety first. Minusbot provides:
- **Isolated Filesystems**: Each chat has its own root directory.
- **Isolated Shells**: Execute Bash, Python, or Node code inside hardened Docker containers.
- **Resource Limits**: Configurable sandbox constraints to prevent runaway processes.

### 🔐 Secure Vaults & Privacy
Stop sharing raw API keys with LLMs.
- **Secret Proxying**: Skills use keys stored in encrypted vaults.
- **Zero-Exposure**: The LLM interacts with skills handles, never seeing the raw secrets.
- **Private Telemetry**: Usage statistics are stored locally and never sold or shared.

### 🛠️ Extensible Skill System
- **LLM Tools**: Native support for Function Calling.
- **Skills Directory**: Easily install new capabilities (Web Search, Image Gen, Browser Automation).
- **Docker-native**: Skills run in their own containers for absolute isolation.

### ⏰ Cron & Automation
- **Scheduled Jobs**: Run commands or AI prompts on a schedule (Cron syntax).
- **Auto-Maintenance**: System updates and heartbeat monitoring.

---

## 🔌 Integrations

Minusbot supports a variety of channels and external services:

### Channels
-   [**Telegram**](docs/channels/telegram.md): Chat with your agent on mobile.
-   [**Discord**](docs/channels/discord.md): Full Discord bot with slash commands.
-   [**Web Dashboard**](docs/channels/web.md): Rich web interface for management and chat.

### External Services
-   [**SerpApi**](docs/integrations/serpapi.md): Google Search capabilities.
-   **More coming soon...**

---

## 🚦 Quick Start

### ⚡ Automated Installation (Recommended)
```bash
# Run the global installer
curl -sSL https://raw.githubusercontent.com/minusbot-org/minusbot/stable/install.sh | bash
```

### 🛠️ Manual Setup
```bash
# Install dependencies
bun run install:all

# Install default skills
bun run skills:install

# Start development server
bun run dev
```

---

## 🛠️ Tech Stack
- **Runtime**: [Bun](https://bun.sh) (Ultra-fast JS runtime)
- **Language**: TypeScript
- **Frontend**: React + TailwindCSS + Vite
- **Sandboxing**: Docker Engine
- **Communication**: WebSockets/Socket.IO

## 📄 License
MIT © sammwy
