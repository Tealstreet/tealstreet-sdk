// @vitest-environment node
import { generateKeyPairSync, sign, verify } from 'node:crypto';
import { gunzipSync } from 'node:zlib';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const A = 'auth0|synthetic-A';
const B = 'disrupt:auth0|synthetic-B';
const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });

type StoredEvent = { owner: string; streamOwner: string; lines: string[] };
type LogPayload = { streams: Array<{ stream: { userId: string }; values: Array<[string, string]> }> };

function tokenResponse(owner: string): Response {
  const expiresAt = Date.now() + 10 * 60_000;
  const header = Buffer.from(JSON.stringify({ alg: 'RS256', typ: 'JWT' })).toString('base64url');
  const claims = Buffer.from(JSON.stringify({ sub: owner, exp: Math.floor(expiresAt / 1000) })).toString('base64url');
  const message = `${header}.${claims}`;
  const signature = sign('RSA-SHA256', Buffer.from(message), privateKey).toString('base64url');
  return Response.json({ expiresAt, token: `${message}.${signature}` });
}

describe('ingest token ownership', () => {
  let utils: typeof import('./utils');
  let owner: string;
  let tokenOwners: string[];
  let stored: StoredEvent[];
  let stops: Array<() => void>;
  let mintToken: (subject: string) => Promise<Response>;

  const start = (subject: string, username?: string) => {
    owner = subject;
    const stop = utils.startLoggingToGrafana(subject, username);
    stops.push(stop);
    return stop;
  };
  const flush = (message: string) => {
    utils.logInfo(message);
    utils.dumpLogQueue();
  };
  const event = (message: string) => stored.find((entry) => entry.lines.some((line) => line.includes(message)));

  beforeEach(async () => {
    vi.resetModules();
    vi.stubEnv('NODE_ENV', 'production');
    vi.stubEnv('NEXT_PUBLIC_LOG_INGEST_ORIGIN', 'https://ingest.example.invalid');
    owner = A;
    tokenOwners = [];
    stored = [];
    stops = [];
    mintToken = async (subject) => tokenResponse(subject);
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url.endsWith('/api/logs/token')) {
          tokenOwners.push(owner);
          return mintToken(owner);
        }
        expect(url).toBe('https://ingest.example.invalid/v1/logs');
        const authorization = new Headers(init?.headers).get('Authorization');
        const [header, claims, signature] = (authorization?.replace(/^Bearer /, '') ?? '').split('.');
        expect(
          verify('RSA-SHA256', Buffer.from(`${header}.${claims}`), publicKey, Buffer.from(signature, 'base64url')),
        ).toBe(true);
        const subject = JSON.parse(Buffer.from(claims, 'base64url').toString()) as { sub: string; exp: number };
        expect(subject.exp * 1000).toBeGreaterThan(Date.now());
        const payload = JSON.parse(gunzipSync(Buffer.from(init?.body as ArrayBuffer)).toString()) as LogPayload;
        // Like the ingest Worker, the receiving boundary trusts the verified JWT,
        // never the caller's stream label, when assigning stored ownership.
        for (const stream of payload.streams) {
          stored.push({
            owner: subject.sub,
            streamOwner: stream.stream.userId,
            lines: stream.values.map((row) => row[1]),
          });
        }
        return new Response(null, { status: 204 });
      }),
    );
    utils = await import('./utils');
  });

  afterEach(() => {
    for (const stop of stops) stop();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it('stores a new B event as B after A cleanup and B start', async () => {
    const stopA = start(A);
    flush('first A event');
    await vi.waitFor(() => expect(event('first A event')?.owner).toBe(A));
    stopA();
    start(B);
    flush('new B event');
    await vi.waitFor(() => expect(event('new B event')).toBeDefined());
    expect(event('new B event')).toMatchObject({ owner: B, streamOwner: B });
    expect(tokenOwners).toEqual([A, B]);
  });

  it('refetches on a direct owner switch without requiring cleanup', async () => {
    start(A);
    flush('first A event');
    await vi.waitFor(() => expect(event('first A event')).toBeDefined());
    start(B);
    flush('direct B event');
    await vi.waitFor(() => expect(event('direct B event')).toBeDefined());
    expect(event('direct B event')).toMatchObject({ owner: B, streamOwner: B });
    expect(tokenOwners).toEqual([A, B]);
  });

  it('treats the same subject in different tenants as different owners', async () => {
    const firstTenant = 'auth0|synthetic-shared-subject';
    const secondTenant = 'disrupt:auth0|synthetic-shared-subject';
    start(firstTenant);
    flush('first tenant event');
    await vi.waitFor(() => expect(event('first tenant event')?.owner).toBe(firstTenant));
    start(secondTenant);
    flush('second tenant event');
    await vi.waitFor(() => expect(event('second tenant event')).toBeDefined());
    expect(event('second tenant event')).toMatchObject({ owner: secondTenant, streamOwner: secondTenant });
    expect(tokenOwners).toEqual([firstTenant, secondTenant]);
  });

  it('discards an A token that resolves after cleanup and a switch to B', async () => {
    const pendingA = Promise.withResolvers<Response>();
    mintToken = async (subject) => (subject === A ? pendingA.promise : tokenResponse(subject));
    const stopA = start(A);
    flush('obsolete A event');
    await vi.waitFor(() => expect(tokenOwners).toEqual([A]));
    stopA();
    start(B);
    flush('current B event');
    pendingA.resolve(tokenResponse(A));
    await vi.waitFor(() => expect(event('current B event')).toBeDefined());
    expect(event('current B event')).toMatchObject({ owner: B, streamOwner: B });
    expect(event('obsolete A event')).toBeUndefined();
    expect(tokenOwners).toEqual([A, B]);
  });

  it('discards a pending token even when the owner switches away and back to A', async () => {
    const pendingA = Promise.withResolvers<Response>();
    let requests = 0;
    mintToken = async (subject) => (++requests === 1 ? pendingA.promise : tokenResponse(subject));
    start(A);
    flush('obsolete A event');
    await vi.waitFor(() => expect(tokenOwners).toEqual([A]));
    start(B);
    start(A);
    flush('fresh A event');
    pendingA.resolve(tokenResponse(A));
    await vi.waitFor(() => expect(event('fresh A event')).toBeDefined());
    expect(event('fresh A event')?.owner).toBe(A);
    expect(event('obsolete A event')).toBeUndefined();
    expect(tokenOwners).toEqual([A, A]);
  });

  it('reuses the token on a same-owner restart and ignores its stale cleanup', async () => {
    const stopOld = start(A, 'Old handle');
    flush('first A event');
    await vi.waitFor(() => expect(event('first A event')).toBeDefined());
    start(A, 'New handle');
    stopOld();
    flush('same owner event');
    await vi.waitFor(() => expect(event('same owner event')).toBeDefined());
    expect(event('same owner event')).toMatchObject({ owner: A, streamOwner: A });
    expect(tokenOwners).toEqual([A]);
  });

  it('clears the token on an owning cleanup even if the next login is A again', async () => {
    const stopA = start(A);
    flush('first A event');
    await vi.waitFor(() => expect(event('first A event')).toBeDefined());
    stopA();
    start(A);
    flush('new login event');
    await vi.waitFor(() => expect(event('new login event')).toBeDefined());
    expect(event('new login event')?.owner).toBe(A);
    expect(tokenOwners).toEqual([A, A]);
  });

  it('does not let stale A cleanup invalidate B token reuse', async () => {
    const stopA = start(A);
    start(B);
    flush('first B event');
    await vi.waitFor(() => expect(event('first B event')).toBeDefined());
    stopA();
    flush('next B event');
    await vi.waitFor(() => expect(event('next B event')).toBeDefined());
    expect(event('next B event')).toMatchObject({ owner: B, streamOwner: B });
    expect(tokenOwners).toEqual([B]);
  });
});
