import express from "express";
import cors from "cors";
import { Server as SocketIOServer } from "socket.io";
import http from "node:http";
import path from "node:path";
import jwt from "jsonwebtoken";

import { getSystemSettings, getJWTSecret, getUserSettings } from "../data/storage";
import { Logger } from "../cli/colors";
import { UserManager } from "../data/users";
import { Storage } from "../data/storage";
import { PubSub } from "../pubsub";
import { ChannelManager, WebChannel } from "../channels";

// Routes
import authRoutes from "./routes/auth";
import chatRoutes from "./routes/chat";
import commandsRoutes from "./routes/commands";

import userAdminRoutes from "./routes/admin/users.admin.routes";
import statsAdminRoutes from "./routes/admin/stats.admin.routes";
import skillsAdminRoutes from "./routes/admin/skills.admin.routes";
import vaultAdminRoutes from "./routes/admin/vault.admin.routes";
import settingsAdminRoutes from "./routes/admin/settings.admin.routes";
import toolsAdminRoutes from "./routes/admin/tools.admin.routes";
import channelsAdminRoutes from "./routes/admin/channels.admin.routes";
import updaterAdminRoutes from "./routes/admin/updater.admin.routes";


import userSkillsRoutes from "./routes/user/skills.routes";
import userSettingsRoutes from "./routes/user/settings.routes";
import userVaultRoutes from "./routes/user/vault.routes";
import userStatsRoutes from "./routes/user/stats.routes";
import userChannelsRoutes from "./routes/user/channels.routes";
import userFilesRoutes from "./routes/user/files.routes";
import providerRoutes from "./routes/providers.routes";

// Middleware
import { authenticate, adminOnly } from "./middleware/auth.middleware";

export async function startServer() {
    const sys = await getSystemSettings();
    const app = express();
    const server = http.createServer(app);

    const isDev = process.env.NODE_ENV === "dev";
    
    // Security: White-list allowed origins instead of using *
    const allowedOrigins = isDev
        ? ['http://localhost:5173', 'http://127.0.0.1:5173']
        : [process.env.FRONTEND_URL || 'https://app.minusbot.ai'];

    const corsOptions = {
        origin: (origin: string | undefined, callback: any) => {
            if (!origin || allowedOrigins.includes(origin)) {
                callback(null, true);
            } else {
                callback(new Error('Not allowed by CORS'));
            }
        },
        credentials: true,
        maxAge: 86400,
        methods: ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'OPTIONS'],
        allowedHeaders: ['Content-Type', 'Authorization']
    };

    // Socket.IO setup
    const io = new SocketIOServer(server, {
        cors: corsOptions
    });

    app.use(cors(corsOptions));
    app.use(express.json());

    // --- API Routes ---
    const api = express.Router();

    api.use("/auth", authRoutes);
    api.use("/", chatRoutes);
    api.use("/", commandsRoutes);

    // User Routes
    const user = express.Router();
    user.use(authenticate);
    user.use("/skills", userSkillsRoutes);
    user.use("/settings", userSettingsRoutes);
    user.use("/vault", userVaultRoutes);
    user.use("/stats", userStatsRoutes);
    user.use("/channels", userChannelsRoutes);
    user.use("/chats", userFilesRoutes);
    user.use("/providers", providerRoutes);
    api.use("/user", user);

    // Admin Routes
    const admin = express.Router();
    admin.use(authenticate, adminOnly);
    admin.use("/users", userAdminRoutes);
    admin.use("/stats", statsAdminRoutes);
    admin.use("/skills", skillsAdminRoutes);
    admin.use("/vault", vaultAdminRoutes);
    admin.use("/settings", settingsAdminRoutes); // includes /global and /system
    admin.use("/tools", toolsAdminRoutes);
    admin.use("/channels", channelsAdminRoutes);
    admin.use("/update", updaterAdminRoutes);
    api.use("/admin", admin);

    app.use("/api", api);

    // --- Socket.IO for Chat ---
    io.on("connection", async (socket) => {
        let authenticated = false;
        let userId = "";

        socket.on("auth", async (data) => {
            try {
                const secret = await getJWTSecret();
                const decoded = jwt.verify(data.token, secret) as any;
                const session = UserManager.getSession(decoded.sessionId);

                if (session && session.userId === decoded.userId) {
                    authenticated = true;
                    userId = decoded.userId;
                    socket.emit("auth_success");

                    // Hand over to WebChannel (System channel, always active)
                    let webChannel = ChannelManager.getInstance(userId, "web") as WebChannel;
                    if (!webChannel) {
                        // Fallback in case it's not in userInstances for some reason
                        const user = UserManager.getUserById(userId);
                        if (user) {
                            webChannel = new WebChannel(user, { enabled: true, settings: {}, secrets: {} });
                            await webChannel.start();
                        }
                    }

                    if (webChannel) {
                        webChannel.handleSocket(socket as any);
                    } else {
                        socket.emit("error", { message: "System failure: Web channel not found." });
                    }
                } else {
                    socket.emit("error", { message: "Session expired" });
                    socket.disconnect();
                }
            } catch (e) {
                socket.emit("error", { message: "Authentication failed" });
                socket.disconnect();
            }
        });
    });

    // Serve Frontend
    const DIST_DIR = path.join(__dirname, "..", "..", "web", "dist");

    if (!isDev) {
        app.use(express.static(DIST_DIR));
        app.use((req, res) => {
            res.sendFile(path.join(DIST_DIR, "index.html"));
        });
    }

    server.listen(sys.web_port, () => {
        Logger.info(`Web API running on http://localhost:${sys.web_port}/api`);
        if (isDev) {
            Logger.info(`Development Frontend should be running on http://localhost:5173`);
        } else {
            Logger.info(`Production Dashboard running on http://localhost:${sys.web_port}`);
        }
    });

    server.on("error", (e: any) => {
        if (e.code === "EADDRINUSE") {
            Logger.warn(`Web API: Port ${sys.web_port} already in use.`);
        } else {
            Logger.error(`Web API Error: ${e.message}`);
        }
    });
}
