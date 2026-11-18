// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { setImmediate } from 'node:timers';

import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { prepareRecordedHttp } from '../__tests__/support/managedHttp';

const INFO = 'https://api.hyperliquid.xyz/info';
const transports: Transport[] = [];
const packet = (request = 1, body = '{"type":"perpDexs"}') => ({
  request,
  method: 'POST',
  url: INFO,
  headers: { 'Content-Type': 'application/json' },
  body,
});
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('indexedDB', new IDBFactory());
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
});
afterEach(() => {
  transports.splice(0).forEach((transport) => transport.dispose?.());
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
const flush = async () => {
  for (let i = 0; i < 16; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
const setup = async () => {
  const { createEngineTransport } = await import('./engine');
  const answer = new TextEncoder().encode('[null,{"name":"xyz"}]');
  const raw: Transport = {
    now: () => Date.now(),
    setTimeout: (callback, ms) => setTimeout(callback, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    prepareHttp: prepareRecordedHttp,
    request: vi.fn(async () => ({ status: 200, body: answer.slice() })),
    connect: vi.fn(),
  };
  const transport = createEngineTransport('hyperliquid', { key: '', secret: '' }, raw);
  transports.push(transport);
  const read = async (request = packet()) =>
    (await transport.prepareHttp!({ ...request, intent: 'public' })).bind(request).dispatch();
  return { transport, raw, answer, read };
};

it('replays byte-exact catalogues without another fetch or quota debit, even during Retry-After', async () => {
  const { raw, answer, read, transport } = await setup();
  expect((await read()).body).toEqual(answer);
  const { HyperliquidInfoBudget } = await import('./hyperliquidInfoBudget');
  await new HyperliquidInfoBudget().response('https://api.hyperliquid.xyz', 429, 'Retry-After: 30');
  expect((await read(packet(2))).body).toEqual(answer);
  expect(raw.request).toHaveBeenCalledTimes(1);
  expect(transport.startupDiagnostics().startup_info_in_flight).toBe('0');
  const privatePacket = { ...packet(3, '{"type":"clearinghouseState","user":"account"}'), timeoutMs: 50 };
  const waiting = Promise.allSettled([read(privatePacket)]);
  await flush();
  await vi.advanceTimersByTimeAsync(51);
  expect((await waiting)[0].status).toBe('rejected');
  expect(raw.request).toHaveBeenCalledTimes(1);
});

it.each(['leader', 'cached'])('binds a %s catalogue to its exact anonymous request', async (phase) => {
  const { read, transport, raw } = await setup();
  if (phase === 'cached') await read();
  const request = packet(2);
  const prepared = await transport.prepareHttp!({ ...request, intent: 'public' });
  expect(() => prepared.bind({ ...request, body: '{"type":"metaAndAssetCtxs","dex":"other"}' })).toThrow();
  expect(raw.request).toHaveBeenCalledTimes(phase === 'cached' ? 1 : 0);
});

it('cached dispatch remains single-use, abortable and bounded by the original deadline', async () => {
  const { read, transport } = await setup();
  await read();
  const request = { ...packet(2), timeoutMs: 50 };
  const prepared = await transport.prepareHttp!({ ...request, intent: 'public' });
  const bound = prepared.bind(request);
  await vi.advanceTimersByTimeAsync(51);
  expect(() => bound.dispatch()).toThrow();
  const next = packet(3);
  const once = (await transport.prepareHttp!({ ...next, intent: 'public' })).bind(next);
  await once.dispatch();
  expect(() => once.dispatch()).toThrow();
  const stop = new AbortController();
  const interrupted = { ...packet(4), signal: stop.signal };
  const abortable = (await transport.prepareHttp!({ ...interrupted, intent: 'public' })).bind(interrupted);
  stop.abort();
  expect(() => abortable.dispatch()).toThrow();
});

it('invalidating an origin forces a new physical catalogue read', async () => {
  const { read, transport, raw } = await setup();
  await read();
  transport.invalidateCatalogue();
  await flush();
  await read(packet(2));
  expect(raw.request).toHaveBeenCalledTimes(2);
});

it('returned network bytes cannot be mutated into the shared cache', async () => {
  const { read, answer } = await setup();
  const first = await read();
  expect(first.body).toEqual(answer);
  (first.body as Uint8Array).set(new TextEncoder().encode('[null,{"name":"abc"}]'));
  await flush();
  const next = await read(packet(2));
  expect(next.body).toEqual(answer);
});
it('cached preparation does not serve bytes after the payload TTL', async () => {
  const { read, transport, raw } = await setup();
  await read();
  await flush();
  const req = packet(2);
  const prepared = await transport.prepareHttp!({ ...req, intent: 'public' });
  const bound = prepared.bind(req);
  await vi.advanceTimersByTimeAsync(5001);
  expect(bound.ready()).toBe(false);
  expect(() => bound.dispatch()).toThrow();
  expect(raw.request).toHaveBeenCalledTimes(1);
});

it('ordinary TTL lookup refreshes and unchanged returned bytes replay exactly', async () => {
  const { read, answer, raw } = await setup();
  expect((await read()).body).toEqual(answer);
  await flush();
  await vi.advanceTimersByTimeAsync(4999);
  expect((await read(packet(2))).body).toEqual(answer);
  expect(raw.request).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(2);
  expect((await read(packet(3))).body).toEqual(answer);
  expect(raw.request).toHaveBeenCalledTimes(2);
});
it('real transport follower bypasses a stalled leader within the lease bound', async () => {
  const { read, answer, raw } = await setup();
  let release!: (r: { status: number; body: Uint8Array }) => void;
  (raw.request as ReturnType<typeof vi.fn>).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  const first = read();
  const firstOutcome = Promise.allSettled([first]);
  for (let i = 0; i < 8 && (raw.request as ReturnType<typeof vi.fn>).mock.calls.length === 0; i++) await flush();
  expect(raw.request).toHaveBeenCalledTimes(1);
  let done = false;
  const second = read(packet(2)).then((r) => {
    done = true;
    return r;
  });
  for (let i = 0; i < 31; i++) {
    await flush();
    await vi.advanceTimersByTimeAsync(100);
  }
  expect(done).toBe(true);
  expect((await second).body).toEqual(answer);
  expect(raw.request).toHaveBeenCalledTimes(2);
  release({ status: 200, body: new TextEncoder().encode('[null,{"name":"old"}]') });
  expect((await firstOutcome)[0].status).toBe('fulfilled');
  await flush();
  expect((await read(packet(3))).body).toEqual(answer);
  expect(raw.request).toHaveBeenCalledTimes(2);
});

it('refuses a cached handoff after the wall clock rolls behind its publication', async () => {
  const { read, transport, raw } = await setup();
  await read();
  await flush();
  const request = packet(2);
  const bound = (await transport.prepareHttp!({ ...request, intent: 'public' })).bind(request);
  vi.setSystemTime(Date.now() - 1000);
  expect(bound.ready()).toBe(false);
  expect(() => bound.dispatch()).toThrow();
  expect(raw.request).toHaveBeenCalledTimes(1);
});
