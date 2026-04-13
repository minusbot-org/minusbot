import fs from "node:fs/promises";
import path from "node:path";

import { WorkspaceManager } from "../data/workspaces";

export class FileSystem {
    static async resolvePath(userId: string, workspaceId: string | null | undefined, userPath: string, chatId?: string): Promise<string> {
        const workspaceDir = WorkspaceManager.resolveContentPath(userId, workspaceId, chatId);
        await fs.mkdir(workspaceDir, { recursive: true });

        // Normalize path
        let safePath = userPath;
        if (safePath.startsWith("/")) {
            safePath = safePath.substring(1);
        }

        // Handle empty path
        if (!safePath || safePath === "." || safePath === "./") {
            return workspaceDir;
        }

        // Join and resolve
        const joinedPath = path.join(workspaceDir, safePath);
        const resolvedPath = path.resolve(joinedPath);

        // CRITICAL: Validate resolved path is within workspace using relative path
        const relativePath = path.relative(workspaceDir, resolvedPath);
        
        // Prevent path traversal: relative path must NOT start with '..'
        if (relativePath.startsWith('..') || path.isAbsolute(relativePath)) {
            throw new Error("Access denied: Path is outside of the workspace.");
        }

        // Check for null bytes and other injection vectors
        if (userPath.includes('\x00') || userPath.includes('%00')) {
            throw new Error("Access denied: Invalid characters in path.");
        }

        // Validate path components don't contain dangerous patterns
        const parts = userPath.split(path.sep).filter(p => p);
        for (const part of parts) {
            if (part === '..') {
                throw new Error("Access denied: Path traversal detected.");
            }
            if (part === '.') {
                continue;
            }
            // Check for Windows reserved names (case-insensitive)
            const reservedNames = ['CON', 'PRN', 'AUX', 'NUL', 'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9', 'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9'];
            if (reservedNames.includes(part.toUpperCase())) {
                throw new Error("Access denied: Invalid filename.");
            }
        }

        // Check for symlinks outside workspace (resolve symlinks)
        try {
            const lstat = await fs.lstat(resolvedPath).catch(() => null);
            if (lstat?.isSymbolicLink()) {
                const realPath = await fs.realpath(resolvedPath).catch(() => resolvedPath);
                const realRelative = path.relative(workspaceDir, realPath);
                if (realRelative.startsWith('..') || path.isAbsolute(realRelative)) {
                    throw new Error("Access denied: Symbolic link outside workspace.");
                }
            }
        } catch {
            // File doesn't exist yet - that's okay for write operations
        }

        return resolvedPath;
    }

    static async mkdir(userId: string, workspaceId: string | null | undefined, dirPath: string, chatId?: string): Promise<string> {
        const target = await this.resolvePath(userId, workspaceId, dirPath, chatId);
        await fs.mkdir(target, { recursive: true });
        return `Directory '${dirPath}' created.`;
    }

    static async rm(userId: string, workspaceId: string | null | undefined, targetPath: string, chatId?: string): Promise<string> {
        const target = await this.resolvePath(userId, workspaceId, targetPath, chatId);
        await fs.rm(target, { recursive: true, force: true });
        return `Deleted '${targetPath}'.`;
    }

    static async read(userId: string, workspaceId: string | null | undefined, filePath: string, chatId?: string): Promise<string> {
        const target = await this.resolvePath(userId, workspaceId, filePath, chatId);
        const stat = await fs.stat(target);
        if (stat.isDirectory()) {
            throw new Error(`'${filePath}' is a directory.`);
        }
        return await fs.readFile(target, "utf-8");
    }

    static async write(userId: string, workspaceId: string | null | undefined, filePath: string, content: string, chatId?: string): Promise<string> {
        const target = await this.resolvePath(userId, workspaceId, filePath, chatId);

        // Ensure parent directory exists
        const parentDir = path.dirname(target);
        await fs.mkdir(parentDir, { recursive: true });

        await fs.writeFile(target, content, "utf-8");
        return `Written to '${filePath}'.`;
    }

    static async stat(userId: string, workspaceId: string | null | undefined, targetPath: string, chatId?: string): Promise<string> {
        const target = await this.resolvePath(userId, workspaceId, targetPath, chatId);
        try {
            const stat = await fs.stat(target);
            return JSON.stringify({
                path: targetPath,
                size: stat.size,
                created: stat.birthtime,
                modified: stat.mtime,
                isDirectory: stat.isDirectory(),
                isFile: stat.isFile()
            }, null, 2);
        } catch (e: any) {
            if (e.code === 'ENOENT') {
                return `File or directory '${targetPath}' does not exist.`;
            }
            throw e;
        }
    }

    static async ls(userId: string, workspaceId: string | null | undefined, dirPath: string = ".", chatId?: string): Promise<string> {
        const target = await this.resolvePath(userId, workspaceId, dirPath, chatId);
        const stat = await fs.stat(target);

        if (!stat.isDirectory()) {
            return JSON.stringify({
                path: dirPath,
                size: stat.size,
                created: stat.birthtime,
                modified: stat.mtime,
                isDirectory: false,
                isFile: true
            }, null, 2);
        }

        const files = await fs.readdir(target, { withFileTypes: true });
        if (files.length === 0) return "Directory is empty.";

        return files.map(f => {
            const type = f.isDirectory() ? "DIR" : "FILE";
            return `[${type}] ${f.name}`;
        }).join("\n");
    }
}
