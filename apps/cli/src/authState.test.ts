import { beforeEach, describe, expect, it, vi } from 'vitest';

const config = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));
vi.mock('./config', () => ({ loadConfig: () => config.current, saveConfig: vi.fn() }));

import { getAuthState } from './auth';

const DAY = 86_400_000;
const jwtExpiringIn = (ms: number) =>
  `h.${Buffer.from(JSON.stringify({ sub: 'auth0|user', exp: Math.floor((Date.now() + ms) / 1000) })).toString('base64url')}.s`;

describe('getAuthState', () => {
  beforeEach(() => {
    config.current = {};
  });

  it('reports not_logged_in without a token', () => {
    expect(getAuthState()).toEqual({ state: 'not_logged_in' });
  });

  it('reports signed_in with whole days left', () => {
    config.current = { auth: { accessToken: jwtExpiringIn(3.5 * DAY), expiresAt: Date.now() + 3.5 * DAY } };
    expect(getAuthState()).toMatchObject({ state: 'signed_in', userId: 'auth0|user', expiresInDays: 3 });
  });

  it('reports expired once the EARLIER expiry has passed, even if the other says otherwise', () => {
    config.current = { auth: { accessToken: jwtExpiringIn(-DAY), expiresAt: Date.now() + 10 * DAY } };
    expect(getAuthState().state).toBe('expired');
    config.current = { auth: { accessToken: jwtExpiringIn(10 * DAY), expiresAt: Date.now() - 1000 } };
    expect(getAuthState().state).toBe('expired');
  });
});
