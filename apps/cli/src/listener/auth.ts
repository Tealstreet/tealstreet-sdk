import { loadConfig } from '../config';

export const ALLOWED_ORIGINS = [
  'https://v3.tealstreet.io',
  'https://preview.tealstreet.io',
  'https://disrupt.trade',
  'https://app.disrupt.trade',
  'http://localhost:3000',
  'http://localhost:8123',
] as const;

export function isOriginAllowed(origin: string | null | undefined): boolean {
  if (!origin) return false;
  return (ALLOWED_ORIGINS as readonly string[]).includes(origin);
}

function decodeJwt(token: string): Record<string, unknown> | null {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    let payload = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    while (payload.length % 4) payload += '=';
    return JSON.parse(Buffer.from(payload, 'base64').toString());
  } catch {
    return null;
  }
}

export function getUserEmail(): string | undefined {
  const config = loadConfig();
  const token = config.auth?.accessToken;
  if (token) {
    const payload = decodeJwt(token);
    const email = payload?.email;
    if (typeof email === 'string' && email.length > 0) return email;
  }
  return config.auth?.userId;
}
