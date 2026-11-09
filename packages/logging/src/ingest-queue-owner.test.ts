// @vitest-environment node
import { generateKeyPairSync, sign, verify } from 'node:crypto';
import { gunzipSync } from 'node:zlib';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tealstreet/utils/src/compression', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tealstreet/utils/src/compression')>();
  return { ...actual, compress: vi.fn(actual.compress) };
});

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
  let respond: (entry: { owner: string; lines: string[] }) => Promise<Response>;
  let compression: typeof import('@tealstreet/utils/src/compression');
  let realCompression: typeof import('@tealstreet/utils/src/compression');
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
    respond = async () => new Response(null, { status: 204 });
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
        const response = await respond(received);
        if (response.ok) stored.push(received);
        return response;
      }),
    );
    utils = await import('./utils');
    compression = await import('@tealstreet/utils/src/compression');
    realCompression = await vi.importActual<typeof import('@tealstreet/utils/src/compression')>(
      '@tealstreet/utils/src/compression',
    );
  });

  afterEach(() => {
    utils.resetLoggingStateForTests();
    for (const stop of stops) stop();
    vi.useRealTimers();
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

  it('attempts the final held-token flush even when the page becomes hidden', async () => {
    const stop = start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    vi.stubGlobal('document', Object.assign(new EventTarget(), { visibilityState: 'hidden' }));
    vi.stubGlobal('window', new EventTarget());
    utils.logInfo('hidden-final-A');
    stop();
    await vi.waitFor(() => expect(event('hidden-final-A')?.owner).toBe(A));
    expect(minted).toEqual([A]);
    expect(attempts.filter((entry) => entry.lines.some((line) => line.endsWith('hidden-final-A')))).toHaveLength(1);
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

  it('drops a stale interval callback queued before the switch to B', async () => {
    const intervals = vi.spyOn(globalThis, 'setInterval');
    start(A);
    utils.logInfo('timer-A');
    const oldTick = intervals.mock.calls.at(-1)?.[0];
    if (typeof oldTick !== 'function') throw new Error('A interval not registered');
    start(B);
    utils.logInfo('timer-B');
    oldTick();
    expect(minted).toEqual([]);
    const currentTick = intervals.mock.calls.at(-1)?.[0];
    if (typeof currentTick !== 'function') throw new Error('B interval not registered');
    currentTick();
    await vi.waitFor(() => expect(event('timer-B')?.owner).toBe(B));
    expect(event('timer-A')).toBeUndefined();
  });

  it('drops a stale visibility callback queued before the switch to B', async () => {
    const page = Object.assign(new EventTarget(), { visibilityState: 'visible' });
    vi.stubGlobal('document', page);
    vi.stubGlobal('window', new EventTarget());
    const listeners = vi.spyOn(page, 'addEventListener');
    start(A);
    utils.logInfo('visibility-A');
    const oldListener = listeners.mock.calls.find(([type]) => type === 'visibilitychange')?.[1];
    if (typeof oldListener !== 'function') throw new Error('A visibility listener not registered');
    start(B);
    utils.logInfo('visibility-B');
    oldListener.call(page, new Event('visibilitychange'));
    expect(minted).toEqual([]);
    page.dispatchEvent(new Event('visibilitychange'));
    await vi.waitFor(() => expect(event('visibility-B')?.owner).toBe(B));
    expect(event('visibility-A')).toBeUndefined();
  });

  it('drops the first A timer after switching through B back to a new A epoch', async () => {
    const intervals = vi.spyOn(globalThis, 'setInterval');
    start(A);
    utils.logInfo('old-epoch-A');
    const oldTick = intervals.mock.calls.at(-1)?.[0];
    if (typeof oldTick !== 'function') throw new Error('First A interval not registered');
    start(B);
    start(A);
    utils.logInfo('new-epoch-A');
    oldTick();
    expect(minted).toEqual([]);
    const currentTick = intervals.mock.calls.at(-1)?.[0];
    if (typeof currentTick !== 'function') throw new Error('New A interval not registered');
    currentTick();
    await vi.waitFor(() => expect(event('new-epoch-A')?.owner).toBe(A));
    expect(event('old-epoch-A')).toBeUndefined();
  });

  it('drops a stale online callback while an offline batch waits across the switch to B', async () => {
    const browser = new EventTarget();
    vi.stubGlobal('document', Object.assign(new EventTarget(), { visibilityState: 'visible' }));
    vi.stubGlobal('window', browser);
    const listeners = vi.spyOn(browser, 'addEventListener');
    online.onLine = false;
    start(A);
    flush('offline-callback-A');
    const oldListener = listeners.mock.calls.find(([type]) => type === 'online')?.[1];
    if (typeof oldListener !== 'function') throw new Error('A online listener not registered');
    start(B);
    flush('offline-callback-B');
    online.onLine = true;
    oldListener.call(browser, new Event('online'));
    expect(minted).toEqual([]);
    browser.dispatchEvent(new Event('online'));
    await vi.waitFor(() => expect(event('offline-callback-B')?.owner).toBe(B));
    expect(event('offline-callback-A')).toBeUndefined();
    expect(listeners).toHaveBeenCalledTimes(2);
  });

  it('drops A while the token JSON body is pending and keeps B authorization', async () => {
    const entered = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    mint = async (subject) => {
      const response = tokenResponse(subject);
      if (subject === A) {
        const read = response.json.bind(response);
        vi.spyOn(response, 'json').mockImplementation(async () => {
          entered.resolve();
          await release.promise;
          return read();
        });
      }
      return response;
    };
    start(A);
    flush('json-await-A');
    await entered.promise;
    start(B);
    flush('json-await-B');
    release.resolve();
    await vi.waitFor(() => expect(event('json-await-B')?.owner).toBe(B));
    expect(event('json-await-A')).toBeUndefined();
    expect(minted).toEqual([A, B]);
  });

  it('ignores stale A cleanup while B token authorization is pending', async () => {
    const staleStop = start(A);
    const pending = Promise.withResolvers<Response>();
    mint = async () => pending.promise;
    start(B);
    flush('cleanup-pending-B');
    await vi.waitFor(() => expect(minted).toEqual([B]));
    staleStop();
    pending.resolve(tokenResponse(B));
    await vi.waitFor(() => expect(event('cleanup-pending-B')?.owner).toBe(B));
    flush('cleanup-cached-B');
    await vi.waitFor(() => expect(event('cleanup-cached-B')?.owner).toBe(B));
    expect(minted).toEqual([B]);
  });

  it('drops an ordinary A upload when B starts during compression', async () => {
    start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    const entered = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    vi.mocked(compression.compress).mockImplementationOnce(async (data) => {
      entered.resolve();
      await release.promise;
      return realCompression.compress(data);
    });
    flush('compressing-A');
    await entered.promise;
    start(B);
    flush('compression-B');
    release.resolve();
    await vi.waitFor(() => expect(event('compression-B')?.owner).toBe(B));
    expect(event('compressing-A')).toBeUndefined();
    expect(minted).toEqual([A, B]);
  });

  it('uses only the captured A token when B starts during the final flush compression', async () => {
    const stop = start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    const entered = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    vi.mocked(compression.compress).mockImplementationOnce(async (data) => {
      entered.resolve();
      await release.promise;
      return realCompression.compress(data);
    });
    utils.logInfo('retiring-A');
    stop();
    await entered.promise;
    start(B);
    flush('retirement-B');
    await vi.waitFor(() => expect(event('retirement-B')?.owner).toBe(B));
    release.resolve();
    await vi.waitFor(() => expect(event('retiring-A')?.owner).toBe(A));
    expect(attempts.filter((entry) => entry.lines.some((line) => line.endsWith('retiring-A')))).toHaveLength(1);
    expect(event('retiring-A')?.lines.some((line) => line.endsWith('retirement-B'))).toBe(false);
    expect(event('retirement-B')?.lines.some((line) => line.endsWith('retiring-A'))).toBe(false);
    expect(minted).toEqual([A, B]);
  });

  it('drops the final A flush if its captured token expires while B starts during compression', async () => {
    const stop = start(A);
    flush('warm-A');
    await vi.waitFor(() => expect(event('warm-A')?.owner).toBe(A));
    const entered = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    const now = Date.now();
    vi.mocked(compression.compress).mockImplementationOnce(async (data) => {
      entered.resolve();
      await release.promise;
      return realCompression.compress(data);
    });
    utils.logInfo('expired-retiring-A');
    stop();
    await entered.promise;
    const pendingCompression = vi.mocked(compression.compress).mock.results.at(-1);
    if (!pendingCompression || pendingCompression.type !== 'return') throw new Error('A compression not pending');
    start(B);
    vi.spyOn(Date, 'now').mockReturnValue(now + 601_000);
    flush('expiry-B');
    await vi.waitFor(() => expect(event('expiry-B')?.owner).toBe(B));
    release.resolve();
    await pendingCompression.value;
    flush('expiry-next-B');
    await vi.waitFor(() => expect(event('expiry-next-B')?.owner).toBe(B));
    expect(event('expired-retiring-A')).toBeUndefined();
    expect(minted).toEqual([A, B]);
  });

  it('drops an A retry when B starts during its backoff timer', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    refusedLines.add('retry-A');
    start(A);
    flush('retry-A');
    await vi.waitFor(() => expect(errors).toHaveBeenCalledWith(expect.stringContaining('Retrying in 1s')));
    start(B);
    flush('retry-B');
    await vi.advanceTimersByTimeAsync(1_001);
    await vi.waitFor(() => expect(event('retry-B')?.owner).toBe(B));
    await vi.advanceTimersByTimeAsync(65_000);
    expect(attempts.filter((entry) => entry.owner === A)).toHaveLength(1);
    expect(event('retry-A')).toBeUndefined();
    expect(minted).toEqual([A, B]);
  });

  for (const status of [204, 500]) {
    it(`keeps B work when A's already-sent upload resolves ${status} after the switch`, async () => {
      const entered = Promise.withResolvers<void>();
      const response = Promise.withResolvers<Response>();
      respond = async (entry) => {
        if (entry.owner === A) {
          entered.resolve();
          return response.promise;
        }
        return new Response(null, { status: 204 });
      };
      start(A);
      flush('response-await-A');
      await entered.promise;
      start(B);
      flush('response-await-B');
      response.resolve(new Response(null, { status }));
      await vi.waitFor(() => expect(event('response-await-B')?.owner).toBe(B));
      expect(attempts.map((entry) => entry.owner)).toEqual([A, B]);
      expect(event('response-await-A')?.owner).toBe(status === 204 ? A : undefined);
      expect(minted).toEqual([A, B]);
    });
  }
});
