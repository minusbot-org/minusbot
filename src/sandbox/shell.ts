import fs from "node:fs/promises";
import { v4 as uuidv4 } from "uuid";

import { SandboxManager } from "./container";
import { SandboxInstance } from "./instance";
import { WorkspaceManager } from "../data/workspaces";

interface ShellSession {
    id: string;
    userId: string;
    workspaceId: string;
    instance: SandboxInstance;
    createdAt: Date;
    command: string;
    isFinished: boolean;
}

export class ShellManager {
    public static verbose: boolean = false;
    private static sessions: Map<string, ShellSession> = new Map();
    private static sessionCounter = 1;

    static async create(
        userId: string,
        workspaceId: string | null | undefined,
        command: string,
        isBg: boolean = false,
        timeoutMs: number = 30000,
        chatId?: string
    ): Promise<string> {
        const image = "alpine:latest";
        await SandboxManager.ensureImage(image);

        // Prepare workspace mount
        const workspaceDir = WorkspaceManager.resolveContentPath(userId, workspaceId, chatId);
        await fs.mkdir(workspaceDir, { recursive: true });

        // Create container with proper bindings
        const result = await SandboxManager.runContainer(
            image,
            ["/bin/sh", "-c", command],
            {
                interactive: isBg,
                binds: [
                    {
                        host: workspaceDir,
                        mount: "/workspace",
                        writable: true
                    }
                ],
                networkMode: "none", // Security: default to no network access
                openStdin: true,
                workingDir: "/workspace",
                maxBufferSize: 1024 * 1024, // 1MB buffer limit
                timeout: isBg ? undefined : timeoutMs
            }
        );

        if (!isBg) {
            return result as string;
        }

        const instance = result as SandboxInstance;
        const id = (this.sessionCounter++).toString();

        const session: ShellSession = {
            id,
            userId,
            workspaceId: workspaceId || "chat",
            instance,
            createdAt: new Date(),
            command,
            isFinished: false
        };

        this.sessions.set(id, session);

        // Monitor container completion
        const monitorCompletion = async () => {
            try {
                await instance.wait();
            } catch (e: any) {
                // Ignore 404 which means auto-removed
                if (e.statusCode !== 404 && !e.message.includes("404")) {
                    instance.write(`\n[System] Container wait error: ${e.message}\n`);
                }
            }

            // Give stream time to flush
            await new Promise(r => setTimeout(r, 500));

            // Ensure removal
            await instance.remove(true);

            session.isFinished = true;

            // Auto-cleanup after 1 hour for background shells
            setTimeout(() => this.sessions.delete(id), 3600000);
        };

        // Fire and forget monitoring for background shells
        monitorCompletion().catch(err => {
            console.error(`Background shell ${id} error:`, err);
        });

        return id;
    }

    static async stdout(userId: string, id: string, waitSeconds: number = 0, tailBytes: number = 0): Promise<string> {
        const session = this.sessions.get(id);
        if (!session) throw new Error("Shell not found. It may have finished or timed out.");

        if (session.userId !== userId && userId !== "root") {
            throw new Error("Access denied.");
        }

        // Timed read: Wait X seconds for NEW output
        if (waitSeconds > 0) {
            return await session.instance.waitAndRead(waitSeconds * 1000);
        }

        // Buffer read: Return last X bytes
        if (tailBytes > 0) {
            return session.instance.getStdoutTail(tailBytes);
        }

        // Default: Return full output history (up to buffer limit)
        return session.instance.getStdout();
    }

    static async stdin(userId: string, id: string, input: string): Promise<string> {
        const session = this.sessions.get(id);
        if (!session) throw new Error("Shell not found.");

        if (session.userId !== userId && userId !== "root") {
            throw new Error("Access denied.");
        }

        // Write to stream
        await session.instance.write(input + "\n");
        return "Input sent.";
    }

    static async kill(userId: string, id: string): Promise<string> {
        const session = this.sessions.get(id);
        if (!session) throw new Error("Shell not found.");

        if (session.userId !== userId && userId !== "root") {
            throw new Error("Access denied.");
        }

        await session.instance.kill();
        await session.instance.remove(true);
        this.sessions.delete(id);
        return "Shell killed.";
    }

    // List user shells
    static list(userId: string, workspaceId?: string): ShellSession[] {
        return Array.from(this.sessions.values()).filter(s => {
            if (s.userId !== userId) return false;
            if (workspaceId && s.workspaceId === workspaceId) return true;
            if (!workspaceId) return true;
            return false;
        });
    }

    // List all shells (sysadmin)
    static listAll(): ShellSession[] {
        return Array.from(this.sessions.values());
    }
}
