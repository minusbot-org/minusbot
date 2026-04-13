import { toolManager } from "./tools";
import { ShellManager } from "../sandbox/shell";
import { commandManager } from "../commands";
import { UserManager } from "../data/users";

// Allowed commands whitelist for security
const ALLOWED_COMMANDS = new Set([
    'ls', 'cat', 'echo', 'pwd', 'date', 'whoami', 'id',
    'grep', 'find', 'head', 'tail', 'wc', 'sort', 'uniq',
    'mkdir', 'rm', 'cp', 'mv', 'chmod', 'chown',
    'df', 'du', 'top', 'ps', 'netstat', 'ss', 'ip',
    'curl', 'wget', 'ping', 'dig', 'host', 'nslookup',
    'tar', 'gzip', 'unzip', 'zip', 'diff', 'cmp',
    'readlink', 'file', 'stat', 'hexdump', 'od',
    'who', 'w', 'uptime', 'free', 'vmstat', 'iostat',
    'hostname', 'uname', 'env', 'printenv', 'which', 'whereis'
]);

const MAX_OUTPUT_SIZE = 1024 * 1024 * 10; // 10MB
const MAX_INPUT_SIZE = 1024 * 10; // 10KB

// Commands

commandManager.register({
    name: "debug",
    description: "Toggle verbose debug mode for shells.",
    usage: "/debug on | off",
    handler: async (args) => {
        const sub = args[0]?.toLowerCase();
        if (sub === "on") {
            ShellManager.verbose = true;
            return "Debug mode enabled.";
        }
        if (sub === "off") {
            ShellManager.verbose = false;
            return "Debug mode disabled.";
        }
        return `Debug mode is currently ${ShellManager.verbose ? "ON" : "OFF"}. Usage: /debug on | off`;
    }
});

commandManager.register({
    name: "shell",
    description: "Manage interactive shells. Only whitelisted commands allowed. Available: " + Array.from(ALLOWED_COMMANDS).sort().join(", "),
    usage: "/shell exec [workspaceId] <cmd> | execbg [workspaceId] <cmd> | read <id> [wait] | readbuf <id> <bytes> | write <id> <input> | kill <id> | ls [workspaceId]",
    handler: async (args, { user, chat }) => {
        const sub = args[0];

        try {
            if (sub === "exec") {
                let workspaceId = args[1];
                let cmd = args.slice(2).join(" ");
                if (!cmd && workspaceId) {
                    cmd = workspaceId;
                    workspaceId = "chat";
                }
                if (!cmd) return "Usage: /shell exec [workspaceId] <command>";
                
                // Validate command
                const parts = cmd.split(' ');
                const baseCommand = parts[0];
                if (!ALLOWED_COMMANDS.has(baseCommand)) {
                    return `Error: Command '${baseCommand}' is not allowed. Allowed commands: ${Array.from(ALLOWED_COMMANDS).sort().slice(0, 10).join(", ")}...`;
                }
                
                return await ShellManager.create(user.id, workspaceId, cmd, false, 30000, chat.meta.id);
            }
            if (sub === "execbg") {
                let workspaceId = args[1];
                let cmd = args.slice(2).join(" ");
                if (!cmd && workspaceId) {
                    cmd = workspaceId;
                    workspaceId = "chat";
                }
                if (!cmd) return "Usage: /shell execbg [workspaceId] <command>";
                
                // Validate command
                const parts = cmd.split(' ');
                const baseCommand = parts[0];
                if (!ALLOWED_COMMANDS.has(baseCommand)) {
                    return `Error: Command '${baseCommand}' is not allowed. Allowed commands: ${Array.from(ALLOWED_COMMANDS).sort().slice(0, 10).join(", ")}...`;
                }
                
                const id = await ShellManager.create(user.id, workspaceId, cmd, true, 30000, chat.meta.id);
                return `Shell started in background. ID: ${id}`;
            }
            if (sub === "read" && args[1]) {
                const wait = args[2] ? parseInt(args[2]) : 0;
                const out = await ShellManager.stdout(user.id, args[1], wait);
                return out || "(No output)";
            }
            if (sub === "readbuf" && args[1] && args[2]) {
                const id = args[1];
                const bytes = parseInt(args[2]);
                if (isNaN(bytes) || bytes <= 0) {
                    return "Usage: /shell readbuf <id> <bytes> - bytes must be a positive number";
                }
                const out = await ShellManager.stdout(user.id, id, 0, bytes);
                return out || "(No output)";
            }
            if (sub === "write" && args[1]) {
                const id = args[1];
                const input = args.slice(2).join(" ");
                return await ShellManager.stdin(user.id, id, input);
            }
            if (sub === "kill" && args[1]) {
                return await ShellManager.kill(user.id, args[1]);
            }
            if (sub === "ls") {
                const workspaceId = args[1] || "chat";
                const shells = ShellManager.list(user.id, workspaceId);
                if (shells.length === 0) return "No active shells.";
                return shells.map(s => `[${s.id}] ${s.workspaceId} - ${s.command} (${s.isFinished ? "Finished" : "Running"})`).join("\n");
            }
        } catch (e: any) {
            return `Shell Error: ${e.message}`;
        }

        return "Usage: /shell exec [workspaceId] <cmd> | execbg [workspaceId] <cmd> | read <id> [wait] | readbuf <id> <bytes> | write <id> <input> | kill <id> | ls [workspaceId]";
    }
});


// Tools definitions

toolManager.registerTool({
    type: "function",
    function: {
        name: "shell_create",
        description: "Create a new shell session. If bg=false, waits for output. Use workspaceId='chat' to use the current chat space. Only allows whitelisted commands. IMPORTANT: For interactive commands (ssh, python, node, etc) requiring input or long running, use bg=true and interact via shell_stdout/shell_stdin.",
        parameters: {
            type: "object",
            properties: {
                workspaceId: { type: "string" },
                command: { 
                    type: "string",
                    description: "List of whitelisted commands: " + Array.from(ALLOWED_COMMANDS).sort().join(", ")
                },
                bg: { type: "boolean", description: "Run in background? Default false for oneshot commands, true for tty/stdin based commands like ssh and TUIs." },
                timeout: { type: "number", description: "Timeout in ms if bg=false. Default 30000." }
            },
            required: ["command"]
        }
    }
}, async ({ workspaceId, command, bg, timeout }, { chat }) => {
    try {
        // CRITICAL: Validate command against whitelist
        const cmd = command.trim();
        const parts = cmd.split(' ');
        const baseCommand = parts[0];
        
        if (!ALLOWED_COMMANDS.has(baseCommand)) {
            return `Error: Command '${baseCommand}' is not allowed. Allowed commands: ${Array.from(ALLOWED_COMMANDS).sort().slice(0, 10).join(", ")}...`;
        }

        // Check input size
        if (cmd.length > MAX_INPUT_SIZE) {
            return `Error: Command too long. Maximum ${MAX_INPUT_SIZE} characters.`;
        }

        const result = await ShellManager.create(user.id, workspaceId, cmd, bg, timeout, chat.meta.id);
        return result;
    } catch (e: any) {
        return `Error: ${e.message}`;
    }
});

toolManager.registerTool({
    type: "function",
    function: {
        name: "shell_stdout",
        description: "Read stdout from a shell session.",
        parameters: {
            type: "object",
            properties: {
                id: { type: "string" },
                wait: { type: "number", description: "Wait seconds for new output. Default 0." },
                tail: { type: "number", description: "Read last N characters from history. Ignored if wait > 0." }
            },
            required: ["id"]
        }
    }
}, async ({ id, wait, tail }, { chat }) => {
    try {
        return await ShellManager.stdout(chat.meta.owner, id, wait, tail);
    } catch (e: any) {
        return `Error: ${e.message}`;
    }
});

toolManager.registerTool({
    type: "function",
    function: {
        name: "shell_stdin",
        description: "Write input to a shell session.",
        parameters: {
            type: "object",
            properties: {
                id: { type: "string" },
                input: { type: "string" }
            },
            required: ["id", "input"]
        }
    }
}, async ({ id, input }, { chat }) => {
    try {
        return await ShellManager.stdin(chat.meta.owner, id, input);
    } catch (e: any) {
        return `Error: ${e.message}`;
    }
});

toolManager.registerTool({
    type: "function",
    function: {
        name: "shell_kill",
        description: "Kill a shell session.",
        parameters: {
            type: "object",
            properties: {
                id: { type: "string" }
            },
            required: ["id"]
        }
    }
}, async ({ id }, { chat }) => {
    try {
        return await ShellManager.kill(chat.meta.owner, id);
    } catch (e: any) {
        return `Error: ${e.message}`;
    }
});

toolManager.registerTool({
    type: "function",
    function: {
        name: "shell_ls",
        description: "List active shell sessions.",
        parameters: {
            type: "object",
            properties: {
                workspaceId: { type: "string" }
            }
        }
    }
}, async ({ workspaceId }, { chat }) => {
    try {
        const list = ShellManager.list(chat.meta.owner, workspaceId);
        return JSON.stringify(list.map(s => ({
            id: s.id,
            workspaceId: s.workspaceId,
            command: s.command,
            status: s.isFinished ? "finished" : "running"
        })));
    } catch (e: any) {
        return `Error: ${e.message}`;
    }
});
