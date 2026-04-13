import express from "express";
import jwt from "jsonwebtoken";
import bcrypt from "bcryptjs";

import { UserManager } from "@/data/users";
import { getJWTSecret } from "@/data/storage";
import { authenticate } from "../middleware/auth.middleware";
import { incrementFailedLogin, resetLoginAttempts, authRateLimit } from "../middleware/rateLimit.middleware";

import { validate } from "../middleware/validate.middleware";
import { LoginDTO } from "../dto/auth.dto";

const router = express.Router();

router.get("/me", authenticate, async (req: any, res) => {
    const user = UserManager.getUserById(req.user.id);
    if (!user) {
        return res.status(404).json({ message: "User not found" });
    }
    res.json({ id: user.id, username: user.username, role: user.role });
});

router.post("/login", authRateLimit, validate(LoginDTO), async (req, res) => {
    const { username, password } = req.body;
    const user = UserManager.getUserByUsername(username);
    const clientIp = req.ip || req.connection?.remoteAddress || 'unknown';

    if (!user) {
        incrementFailedLogin(clientIp, username);
        return res.status(401).json({ message: "Invalid credentials" });
    }

    if (!(await bcrypt.compare(password, user.passwordHash))) {
        incrementFailedLogin(clientIp, username);
        return res.status(401).json({ message: "Invalid credentials" });
    }

    resetLoginAttempts(clientIp, username);
    
    const sessionId = Math.random().toString(36).substring(2) + Math.random().toString(36).substring(2);
    const expiresAt = Date.now() + 1000 * 60 * 60 * 24; // 24h

    await UserManager.saveSession({ id: sessionId, userId: user.id, expiresAt });

    const secret = await getJWTSecret();
    const token = jwt.sign({ userId: user.id, sessionId }, secret, { expiresIn: "24h" });
    res.json({ token, user: { id: user.id, username: username, role: user.role } });
});

router.post("/login", validate(LoginDTO), async (req, res) => {
    const { username, password } = req.body;
    const user = UserManager.getUserByUsername(username);

    if (!user) {
        return res.status(401).json({ message: "User not found" });
    }

    if (!(await bcrypt.compare(password, user.passwordHash))) {
        return res.status(401).json({ message: "Incorrect password" });
    }

    const sessionId = Math.random().toString(36).substring(2);
    const expiresAt = Date.now() + 1000 * 60 * 60 * 24; // 24h

    await UserManager.saveSession({ id: sessionId, userId: user.id, expiresAt });

    const secret = await getJWTSecret();
    const token = jwt.sign({ userId: user.id, sessionId }, secret, { expiresIn: "24h" });
    res.json({ token, user: { id: user.id, username: user.username, role: user.role } });
});

router.delete("/session/:id", authenticate, async (req: any, res) => {
    if (req.params.id === req.sessionId || req.user.role !== "user") {
        await UserManager.deleteSession(req.params.id);
        res.send("Session deleted");
    } else {
        res.status(403).send("Forbidden");
    }
});

export default router;
