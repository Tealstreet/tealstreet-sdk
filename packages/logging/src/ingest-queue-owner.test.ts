// @vitest-environment node
import { generateKeyPairSync, sign, verify } from 'node:crypto';
import { gunzipSync } from 'node:zlib';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const A = 'synthetic-queue-A';
const B = 'synthetic-queue-B';
const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });

function tokenResponse(owner: string): Response {
  const expiresAt = Date.now() + 600_000;
  const header = Buffer.from(JSON.stringify({ alg: 'RS256' })).toString('base64url');
  const claims = Buffer.from(JSON.stringify({ sub: owner, exp: expiresAt / 1000 })).toString('base64url');
  const message = `${header}.${claims}`;
  const signature = sign('RSA-SHA256', Buffer.from(message), privateKey).toString('base64url');
  return Response.json({ expiresAt, token: `${message}.${signature}` });
}

function storedLines(payload: unknown): string[] {
  if (!payload || typeof payload !== 'object' || !('streams' in payload) || !Array.isArray(payload.streams)) {
    throw new Error('Invalid streams');
  }
  return payload.streams.flatMap((stream: unknown) => {
    if (!stream || typeof stream !== 'object' || !('values' in stream) || !Array.isArray(stream.values)) {
      throw new Error('Invalid values');
    }
    return stream.values.map((row: unknown) => {
      if (!Array.isArray(row) || typeof row[1] !== 'string') throw new Error('Invalid log line');
      return row[1];
    });
  });
}

describe('queued ingest ownership', () => {
  let utils: typeof import('./utils');
  let owner: string;
  let online: { onLine: boolean };
  let stored: Array<{ owner: string; lines: string[] }>;
  let attempts: Array<{ owner: string; lines: string[] }>;
  let refusedLines: Set<string>;
  let minted: string[];
  let stops: Array<() => void>;
  let mint: (owner: string) => Promise<Response>;
  const start = (subject: string) => {
    owner = subject;
    const stop = utils.startLoggingToGrafana(subject);
    stops.push(stop);
    return stop;
  };
  const flush = (line: string) => {
    utils.logInfo(line);
    utils.dumpLogQueue();
  };
  const event = (line: string) => stored.find((entry) => entry.lines.some((value) => value.endsWith(line)));

  beforeEach(async () => {
    vi.resetModules();
    vi.stubEnv('NODE_ENV', 'production');
    vi.stubEnv('NEXT_PUBLIC_LOG_INGEST_ORIGIN', 'https://ingest.example.invalid');
    owner = A;
    online = { onLine: true };
    stored = [];
    attempts = [];
    refusedLines = new Set();
    minted = [];
    stops = [];
    mint = async (subject) => tokenResponse(subject);
    vi.stubGlobal('navigator', online);
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        if (String(input).endsWith('/api/logs/token')) {
          minted.push(owner);
          return mint(owner);
        }
        const authorization = new Headers(init?.headers).get('Authorization');
        const [header, claims, signature] = (authorization?.replace(/^Bearer /, '') ?? '').split('.');
        if (!header || !claims || !signature) throw new Error('Missing signed token');
        expect(
          verify('RSA-SHA256', Buffer.from(`${header}.${claims}`), publicKey, Buffer.from(signature, 'base64url')),
        ).toBe(true);
        const subject: unknown = JSON.parse(Buffer.from(claims, 'base64url').toString());
        if (!subject || typeof subject !== 'object' || !('sub' in subject) || typeof subject.sub !== 'string') {
          throw new Error('Invalid subject');
        }
        if (!(init?.body instanceof ArrayBuffer)) throw new Error('Missing compressed body');
        const payload: unknown = JSON.parse(gunzipSync(Buffer.from(init.body)).toString());
        const received = { owner: subject.sub, lines: storedLines(payload) };
        attempts.push(received);
        if (received.lines.some((line) => [...refusedLines].some((refused) => line.endsWith(refused)))) {
          return new Response(null, { status: 500 });
        }
        stored.push(received);
        return new Response(null, { status: 204 });
      }),
    );
    utils = await import('./utils');
  });

  afterEach(() => {
    utils.resetLoggingStateForTests();
    for (const stop of stops) stop();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it('drops unflushed A entries instead of storing them as B', async () => {
    const stop = start(A);
    utils.logInfo('unflushed-A');
    stop();
    start(B);
    flush('current-B');
    await vi.waitFor(() => expect(event('current-B')?.owner).toBe(B));
    expect(event('unflushed-A')).toBeUndefined();
  });

  it('drops offline A batches instead of storing them as B', async () => {
    online.onLine = false;
    const stop = start(A);
    flush('offline-A');
    stop();
    start(B);
    online.onLine = true;
    flush('online-B');
    await vi.waitFor(() => expect(event('online-B')?.owner).toBe(B));
    expect(event('offline-A')).toBeUndefined();
  });

  it('drops A batches queued behind an in-flight token without removing B work', async () => {
    const pending = Promise.withResolvers<Response>();
    mint = async (subject) => (subject === A ? pending.promise : tokenResponse(subject));
    const stop = start(A);
    flush('inflight-A');
    await vi.waitFor(() => expect(minted).toEqual([A]));
    flush('queued-A');
    stop();
    start(B);
    flush('queued-B');
    pending.resolve(tokenResponse(A));
    await vi.waitFor(() => expect(event('queued-B')?.owner).toBe(B));
    expect(event('inflight-A')).toBeUndefined();
    expect(event('queued-A')).toBeUndefined();
    expect(minted).toEqual([A, B]);
  });

  it('flushes queued A data once with its held token on a direct owner switch', async () => {
    start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    utils.logInfo('last-A');
    start(B);
    flush('current-B');
    await vi.waitFor(() => expect(event('current-B')?.owner).toBe(B));
    await vi.waitFor(() => expect(event('last-A')?.owner).toBe(A));
    expect(minted).toEqual([A, B]);
    expect(attempts.filter((entry) => entry.lines.some((line) => line.endsWith('last-A')))).toHaveLength(1);
  });

  it('drops the retiring batch after one failed flush without retry or token refetch', async () => {
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    const stop = start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    refusedLines.add('refused-A');
    utils.logInfo('refused-A');
    stop();
    start(B);
    flush('current-B');
    await vi.waitFor(() => expect(event('current-B')?.owner).toBe(B));
    await vi.waitFor(() =>
      expect(errors).toHaveBeenCalledWith(expect.stringContaining('Dropping batch after 0 retries')),
    );
    flush('next-B');
    await vi.waitFor(() => expect(event('next-B')?.owner).toBe(B));
    expect(event('refused-A')).toBeUndefined();
    expect(attempts.filter((entry) => entry.lines.some((line) => line.endsWith('refused-A')))).toHaveLength(1);
    expect(minted).toEqual([A, B]);
  });

  it('drops offline leftovers even when A has a held token', async () => {
    const stop = start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    online.onLine = false;
    flush('offline-held-A');
    stop();
    start(B);
    online.onLine = true;
    flush('current-B');
    await vi.waitFor(() => expect(event('current-B')?.owner).toBe(B));
    expect(event('offline-held-A')).toBeUndefined();
    expect(minted).toEqual([A, B]);
  });

  it('preserves pending batches and raw entries on a same-owner restart', async () => {
    online.onLine = false;
    const staleStop = start(A);
    flush('pending-A');
    utils.logInfo('unflushed-A');
    start(A);
    staleStop();
    online.onLine = true;
    utils.dumpLogQueue();
    await vi.waitFor(() => expect(event('pending-A')?.owner).toBe(A));
    await vi.waitFor(() => expect(event('unflushed-A')?.owner).toBe(A));
    expect(minted).toEqual([A]);
  });
});
