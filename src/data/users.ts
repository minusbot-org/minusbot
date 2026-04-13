import fs from "node:fs/promises";
import path from "node:path";
import bcrypt from "bcryptjs";
import { randomBytes } from "node:crypto";

import { getConfigDir } from "./storage";
import { Logger } from "../cli/colors";

export type Role = "root" | "admin" | "user";

export interface User {
    id: string;
    username: string;
    passwordHash: string;
    role: Role;
}

export interface Session {
    id: string;
    userId: string;
    expiresAt: number;
}

function getAuthDir(): string {
    return path.join(getConfigDir(), "auth");
}

function getUsersFile(): string {
    return path.join(getAuthDir(), "users.json");
}

function getSessionsDir(): string {
    return path.join(getAuthDir(), "sessions");
}

export class UserManager {
    private static users: User[] = [];
    private static sessions: Map<string, Session> = new Map();

    static async init() {
        await fs.mkdir(getAuthDir(), { recursive: true });
        await fs.mkdir(getSessionsDir(), { recursive: true });

        // Load users
        try {
            const content = await fs.readFile(getUsersFile(), "utf-8");
            this.users = JSON.parse(content);
        } catch {
            // Create root user if it doesn't exist
            const buffer = randomBytes(16);
            const rootPass = buffer.toString('hex');
            const passwordHash = await bcrypt.hash(rootPass, 12);
            this.users = [{
                id: "root",
                username: "root",
                passwordHash,
                role: "root"
            }];
            await this.saveUsers();
            await Logger.info("Created default root user");
            await Logger.warn(`Root Password: ${rootPass}`);
            await Logger.warn("Please save this password immediately!");
        }

        // Load sessions
        const sessionFiles = await fs.readdir(getSessionsDir());
        for (const file of sessionFiles) {
            if (file.endsWith(".json")) {
                try {
                    const content = await fs.readFile(path.join(getSessionsDir(), file), "utf-8");
                    const session: Session = JSON.parse(content);
                    if (session.expiresAt > Date.now()) {
                        this.sessions.set(session.id, session);
                    } else {
                        await fs.unlink(path.join(getSessionsDir(), file));
                    }
                } catch { }
            }
        }
    }

    private static async saveUsers() {
        await fs.writeFile(getUsersFile(), JSON.stringify(this.users, null, 4), "utf-8");
    }

    static async saveSession(session: Session) {
        this.sessions.set(session.id, session);
        await fs.writeFile(path.join(getSessionsDir(), `${session.id}.json`), JSON.stringify(session, null, 4), "utf-8");
    }

    static async deleteSession(sessionId: string) {
        this.sessions.delete(sessionId);
        try {
            await fs.unlink(path.join(getSessionsDir(), `${sessionId}.json`));
        } catch { }
    }

    static getUsers() {
        return this.users;
    }

    static getUserById(id: string) {
        return this.users.find(u => u.id === id);
    }

    static getUserByUsername(username: string) {
        return this.users.find(u => u.username === username);
    }

    static async createUser(username: string, password: string, role: Role) {
        if (this.getUserByUsername(username)) throw new Error("User already exists");
        const id = Math.random().toString(36).substring(7);
        const passwordHash = await bcrypt.hash(password, 10);
        const newUser: User = { id, username, passwordHash, role: role === "root" ? "admin" : role }; // Only one root
        this.users.push(newUser);
        await this.saveUsers();
        return newUser;
    }

    static async updateUser(id: string, updates: Partial<Pick<User, "username" | "passwordHash" | "role">>) {
        const user = this.getUserById(id);
        if (!user) throw new Error("User not found");

        if (updates.username && id !== "root") user.username = updates.username;
        if (id === "root" && updates.username) user.username = updates.username; // Root can change username

        if (updates.role && id !== "root") user.role = updates.role;
        // Password hash update if provided
        if (updates.passwordHash) user.passwordHash = updates.passwordHash;

        await this.saveUsers();
        return user;
    }

    static async deleteUser(id: string) {
        if (id === "root") throw new Error("Cannot delete root user");
        this.users = this.users.filter(u => u.id !== id);
        await this.saveUsers();
        // Cleanup sessions for this user
        for (const [sid, sess] of this.sessions) {
            if (sess.userId === id) {
                await this.deleteSession(sid);
            }
        }
    }

    static getSession(id: string) {
        const sess = this.sessions.get(id);
        if (sess && sess.expiresAt > Date.now()) return sess;
        return null;
    }
}
