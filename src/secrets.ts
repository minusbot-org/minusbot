import path from "node:path";
import fs from "node:fs/promises";
import { createCipheriv, createDecipheriv, randomBytes } from "node:crypto";
import { SHARED_SECRETS_DIR, getUserDir, getConfigDir } from "./data/storage";

export const VAULT_DEFAULTS: Record<string, string[]> = {
    "agent": ["API_KEY"]
};

// Security: Get encryption key from environment or generate secure key
let encryptionKeyCache: Buffer | null = null;

async function getEncryptionKey(): Promise<Buffer> {
    if (encryptionKeyCache) {
        return encryptionKeyCache;
    }

    const envKey = process.env.SECRET_ENCRYPTION_KEY;
    if (envKey && envKey.length >= 32) {
        encryptionKeyCache = Buffer.from(envKey, 'hex');
        return encryptionKeyCache;
    }
    
    // Generate and persist key on first run
    const configDir = getConfigDir();
    const keyFile = path.join(configDir, ".encryption-key");
    
    try {
        const existingKey = await fs.readFile(keyFile, 'utf-8');
        encryptionKeyCache = Buffer.from(existingKey, 'hex');
        return encryptionKeyCache;
    } catch {
        const newKey = randomBytes(32);
        await fs.mkdir(configDir, { recursive: true });
        await fs.writeFile(keyFile, newKey.toString('hex'));
        // Set restrictive permissions
        await fs.chmod(keyFile, 0o600).catch(() => {});
        encryptionKeyCache = newKey;
        return newKey;
    }
}

const ENCRYPTION_KEY = await getEncryptionKey();

export class Vault {
    private data: Record<string, string> = {};

    constructor(private filePath: string, private name?: string) { }

    async load() {
        if (this.name) {
            const defaults = VAULT_DEFAULTS[this.name];
            if (defaults) {
                for (const key of defaults) {
                    this.data[key] = "";
                }
            }
        }

        try {
            const content = await fs.readFile(this.filePath, "utf-8");
            
            // Check if content is encrypted (starts with encrypted marker)
            if (content.trim().startsWith("encrypted:")) {
                // Decrypt all values
                const lines = content.split("\n").slice(1); // Skip encryption marker
                for (const line of lines) {
                    const [key, ...rest] = line.split("=");
                    if (key && rest.length > 0) {
                        try {
                            this.data[key.trim()] = this.decryptValue(rest.join("=").trim());
                        } catch {
                            this.data[key.trim()] = "";
                        }
                    }
                }
            } else {
                // Plain text (legacy - migrate on next save)
                content.split("\n").forEach((line) => {
                    const [key, ...rest] = line.split("=");
                    if (key && rest.length > 0) {
                        this.data[key.trim()] = rest.join("=").trim();
                    }
                });
            }
        } catch { }
    }

    async get(key: string): Promise<string | undefined> {
        return this.data[key];
    }

    async allValues(): Promise<Record<string, string>> {
        return { ...this.data };
    }

    async maskedValues(): Promise<Record<string, boolean>> {
        const result: Record<string, boolean> = {};
        for (const [key, value] of Object.entries(this.data)) {
            result[key] = value !== undefined && value !== "";
        }
        return result;
    }

    listKeys(): string[] {
        return Object.keys(this.data);
    }

    async set(key: string, value: string) {
        this.data[key] = value;
        await this.save();
    }

    async delete(key: string) {
        this.data[key] = "";
        await this.save();
    }

    private encryptValue(value: string): string {
        const iv = randomBytes(12);
        const cipher = createCipheriv('aes-256-gcm', ENCRYPTION_KEY, iv);
        let encrypted = cipher.update(value, 'utf8', 'base64');
        encrypted += cipher.final('base64');
        const authTag = cipher.getAuthTag();
        
        return JSON.stringify({
            iv: iv.toString('base64'),
            authTag: authTag.toString('base64'),
            data: encrypted
        });
    }

    private decryptValue(encryptedData: string): string {
        try {
            const parsed = JSON.parse(encryptedData);
            const { iv, authTag, data } = parsed;
            const decipher = createDecipheriv('aes-256-gcm', ENCRYPTION_KEY, Buffer.from(iv, 'base64'));
            decipher.setAuthTag(Buffer.from(authTag, 'base64'));
            let decrypted = decipher.update(data, 'base64', 'utf8');
            decrypted += decipher.final('utf8');
            return decrypted;
        } catch {
            return "";
        }
    }

    private async save() {
        await fs.mkdir(path.dirname(this.filePath), { recursive: true });
        
        // Encrypt all values when saving
        const encryptedEntries = Object.entries(this.data)
            .map(([k, v]) => `${k}=${this.encryptValue(v)}`);
        
        const content = `encrypted:\n${encryptedEntries.join("\n")}`;
        await fs.writeFile(this.filePath, content, "utf-8");
    }
}

export class MergedVault {
    constructor(private userVault: Vault, private globalVault: Vault) { }

    get(key: string): string | undefined {
        const val = this.userVault.get(key);
        // If empty or undefined, use global
        if (val === undefined || val === "") return this.globalVault.get(key);
        return val;
    }

    listKeys(): string[] {
        return Array.from(new Set([...this.userVault.listKeys(), ...this.globalVault.listKeys()]));
    }

    allValues(): Record<string, string> {
        const data: Record<string, string> = {};
        for (const k of this.listKeys()) {
            data[k] = this.get(k) || "";
        }
        return data;
    }

    maskedValues(): Record<string, boolean> {
        const result: Record<string, boolean> = {};
        for (const k of this.listKeys()) {
            const val = this.get(k);
            result[k] = val !== undefined && val !== "";
        }
        return result;
    }

    async set(key: string, value: string) {
        // By default, we set to user vault
        await this.userVault.set(key, value);
    }

    async delete(key: string) {
        await this.userVault.delete(key);
    }
}

export const secrets = {
    async userVault(userId: string, name: string): Promise<Vault> {
        const filePath = path.join(getUserDir(userId), "secrets", `${name}.env`);
        const v = new Vault(filePath, name);
        await v.load();
        return v;
    },

    async globalVault(name: string): Promise<Vault> {
        const filePath = path.join(SHARED_SECRETS_DIR, `${name}.env`);
        const v = new Vault(filePath, name);
        await v.load();
        return v;
    },

    async vault(userId: string, name: string): Promise<MergedVault> {
        const uv = await this.userVault(userId, name);
        const gv = await this.globalVault(name);
        return new MergedVault(uv, gv);
    }
};
