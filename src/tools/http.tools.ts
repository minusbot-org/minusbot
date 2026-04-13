import { toolManager } from "./tools";

const ALLOWED_HTTP_DOMAINS = new Set([
    'api.openai.com',
    'openrouter.ai',
    'serpapi.com',
    'api.telegram.org',
    'discord.com',
    'discordapp.com'
]);

const MAX_RESPONSE_SIZE = 1024 * 1024;
const HTTP_TIMEOUT = 10000;

function isAllowedUrl(url: string): boolean {
    try {
        const parsed = new URL(url);
        if (parsed.protocol !== 'https:') return false;
        return ALLOWED_HTTP_DOMAINS.has(parsed.hostname);
    } catch {
        return false;
    }
}

toolManager.registerTool({
    type: "function",
    function: {
        name: "http_request",
        description: "Make an HTTP request to whitelisted domains only.",
        parameters: {
            type: "object",
            properties: {
                method: { type: "string", enum: ["GET", "POST", "PUT", "DELETE", "PATCH"], default: "GET" },
                url: { type: "string", description: "The URL to request (must be from whitelisted domains)" },
                headers: { type: "object" },
                body: { type: "string" }
            },
            required: ["url"]
        }
    }
}, async (args) => {
    try {
        if (!isAllowedUrl(args.url)) {
            return `Error: URL not allowed. Whitelisted: ${Array.from(ALLOWED_HTTP_DOMAINS).join(", ")}`;
        }

        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), HTTP_TIMEOUT);

        const response = await fetch(args.url, {
            method: args.method || "GET",
            headers: args.headers || {},
            body: args.body,
            signal: controller.signal
        });

        clearTimeout(timeoutId);

        const status = response.status;
        const headers = Object.fromEntries(response.headers.entries());
        let data: string;

        const reader = response.body?.getReader();
        if (reader) {
            const chunks: Uint8Array[] = [];
            let totalSize = 0;
            while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                totalSize += value.length;
                if (totalSize > MAX_RESPONSE_SIZE) {
                    return `Error: Response too large. Maximum ${MAX_RESPONSE_SIZE} bytes.`;
                }
                chunks.push(value);
            }
            data = new TextDecoder().decode(Buffer.concat(chunks));
        } else {
            const contentType = response.headers.get("content-type") || "";
            if (contentType.includes("application/json")) {
                data = JSON.stringify(await response.json(), null, 2);
            } else {
                const text = await response.text();
                data = text.length > MAX_RESPONSE_SIZE 
                    ? text.substring(0, MAX_RESPONSE_SIZE) + `\n... [truncated]`
                    : text;
            }
        }

        return JSON.stringify({ status, headers, data });
    } catch (e: any) {
        if (e.name === 'AbortError') return `Error: Request timeout (${HTTP_TIMEOUT}ms)`;
        return `Error: ${e.message}`;
    }
});
