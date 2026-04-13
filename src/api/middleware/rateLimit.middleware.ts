import { Request, Response, NextFunction } from "express";

export const rateLimitedIps: Record<string, number> = {};
export const loginAttempts: Record<string, number> = {};

export const authRateLimit = (req: Request, res: Response, next: NextFunction) => {
    const clientIp = req.ip || req.connection?.remoteAddress || 'unknown';
    const username = (req.body as any)?.username || 'none';
    const userKey = `${clientIp}:${username}`;
    
    // Check if IP is globally rate limited
    if (rateLimitedIps[clientIp] && rateLimitedIps[clientIp] > Date.now()) {
        return res.status(429).json({ 
            error: 'Too many attempts. Please try again later.' 
        });
    }
    
    // Check per-user attempts
    if (loginAttempts[userKey] && loginAttempts[userKey] >= 5) {
        rateLimitedIps[clientIp] = Date.now() + 15 * 60 * 1000; // 15 minutes
        return res.status(429).json({ 
            error: 'Account locked due to too many failed attempts. Try again later.' 
        });
    }
    
    next();
};

export const incrementFailedLogin = (ip: string, username: string) => {
    const userKey = `${ip}:${username}`;
    loginAttempts[userKey] = (loginAttempts[userKey] || 0) + 1;
};

export const resetLoginAttempts = (ip: string, username: string) => {
    const userKey = `${ip}:${username}`;
    loginAttempts[userKey] = 0;
};

export const globalRateLimit = (windowMs = 60000, max = 100) => {
    const requests: Record<string, number[]> = {};
    
    return (req: Request, res: Response, next: NextFunction) => {
        const clientIp = req.ip || req.connection?.remoteAddress || 'unknown';
        const now = Date.now();
        
        if (!requests[clientIp]) {
            requests[clientIp] = [];
        }
        
        // Remove old requests outside window
        requests[clientIp] = requests[clientIp].filter(timestamp => now - timestamp < windowMs);
        
        if (requests[clientIp].length >= max) {
            return res.status(429).json({ error: 'Too many requests. Please try again later.' });
        }
        
        requests[clientIp].push(now);
        next();
    };
};
