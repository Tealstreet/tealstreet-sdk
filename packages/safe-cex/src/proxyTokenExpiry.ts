import jwt from 'jsonwebtoken';

import { virtualClock } from './utils/virtualClock';

/** Shared proxy-JWT expiry contract for both token access paths. */
export function isTokenExpired(token: string): boolean {
  if (!token) return true;

  try {
    const decoded = jwt.decode(token, { json: true });
    if (!decoded) return true;

    const expiration = (decoded.exp || 0) * 1000;
    return expiration < virtualClock.getCurrentTimeMs();
  } catch {
    return true;
  }
}
