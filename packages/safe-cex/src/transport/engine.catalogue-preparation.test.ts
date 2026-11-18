import type { DisposableAxiosAdapter } from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { AxiosAdapter } from 'axios';

import { setImmediate } from 'node:timers';

import { IDBDatabase, IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { configureHOW } from '@tealstreet/http-over-websocket/src/config';
import { resetTunnelLane, tunnelEgressUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';

const socket = vi.hoisted(() => ({ open: true, synced: true, auth: 'Good', frames: [] as unknown[] }));
vi.mock('@tealstreet/http-over-websocket/src/HOWWebSocket', () => ({
  HOWWebSocket: class {
    ws = {
      OPEN: 1,
      get readyState() {
        return socket.open ? 1 : 0;
      },
      loopState: { phase: 'connected', since: 0 },
    };
    epoch = 0;
    isInitialized = true;
    isTokenSent = false;
    tokenSentAt = 0;
    get isOpen() {
      return socket.open;
    }
    get isTimeSynced() {
      return socket.synced;
    }
    get authState() {
      return socket.auth;
    }
    listeners = new Map<string, (data: unknown) => void>();
    once(event: string, listener: (data: unknown) => void) {
      this.listeners.set(event, listener);
    }
    off(event: string) {
      this.listeners.delete(event);
    }
    sendPrepared(frame: string) {
      const message = JSON.parse(frame);
      socket.frames.push(message);
      queueMicrotask(() => this.listeners.get(`sub:${message.nonce}`)?.({ status: 200, data_raw: '{}', headers: {} }));
    }
    sendToken(frame: unknown) {
      this.isTokenSent = true;
      this.tokenSentAt = Date.now();
      socket.frames.push(frame);
    }
  },
}));
vi.mock('../accessToken', () => ({
  getProxyToken: async () => 'proxy-fixture',
  isTokenExpired: () => false,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => async () => 'proxy-fixture',
  refreshProxyToken: vi.fn(async () => undefined),
}));
vi.mock('../index', () => ({ createExchange: vi.fn() }));

const CATALOGUE_URL = 'https://fapi.binance.com/fapi/v1/exchangeInfo';
const packet = (url = CATALOGUE_URL, timeoutMs = 5_000) => ({
  method: 'GET',
  url,
  headers: {},
  intent: 'public' as const,
  timeoutMs,
});
const flush = async () => {
  for (let i = 0; i < 16; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
const raw = (): Transport => ({
  now: Date.now,
  setTimeout,
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  request: vi.fn(async () => ({ status: 200, body: new Uint8Array() })),
  connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
});
type PreparingAdapter = AxiosAdapter & Pick<DisposableAxiosAdapter, 'prepare'>;
const hasPreparation = (adapter: AxiosAdapter | undefined): adapter is PreparingAdapter =>
  adapter !== undefined && 'prepare' in adapter && typeof adapter.prepare === 'function';

const engines: Array<{ dispose?(): void }> = [];
const setup = async (preparationError?: Error) => {
  const { createEngineTransport } = await import('./engine');
  const { tealstreetTransport } = await import('./tealstreet/tealstreetTransport');
  const reports = vi.fn<(message: string, ...provenance: unknown[]) => void>();
  const native = raw();
  const original = tealstreetTransport.restAdapter;
  if (!original) throw new Error('Fixture requires the real HOW adapter factory');
  let prepares = 0;
  vi.spyOn(tealstreetTransport, 'restAdapter').mockImplementation((options) => {
    const adapter = original(options);
    if (hasPreparation(adapter)) {
      const prepare = adapter.prepare;
      vi.spyOn(adapter, 'prepare').mockImplementation((...args) => {
        prepares++;
        if (preparationError) return Promise.reject(preparationError);
        return prepare.call(adapter, ...args);
      });
    }
    return adapter;
  });
  const engine = createEngineTransport(
    'binance',
    { key: 'fixture', secret: 'fixture', transport: tealstreetTransport, tealstreetTunnel: true },
    native,
    undefined,
    reports,
  );
  engines.push(engine);
  return { engine, reports, native, prepares: () => prepares };
};
beforeEach(async () => {
  vi.resetModules();
  vi.stubGlobal('indexedDB', new IDBFactory());
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
  Object.assign(socket, { open: true, synced: true, auth: 'Good', frames: [] });
  resetTunnelLane(true);
  configureHOW({ getProxyToken: async () => 'proxy-fixture', isTokenExpired: () => false });
  // Load the real module graph outside the request's assertion timeout.
  await import('./engine');
});
afterEach(() => {
  for (const engine of engines.splice(0)) engine.dispose?.();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  resetTunnelLane();
});

it.each(['storage', 'owner'] as const)(
  'reports coordinated %s failure as local, before HOW preparation, without leaking the cause',
  async (failure) => {
    const { sharedBinanceCatalogue } = await import('./binanceCatalogue');
    const seed = await sharedBinanceCatalogue.prepare(packet(), new AbortController().signal, Date.now() + 20_000, {
      network: 'mainnet',
      route: 'direct',
    });
    seed?.cancel();
    await flush();
    if (failure === 'storage')
      vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(() => {
        throw new DOMException('private-storage-canary', 'InvalidStateError');
      });
    else
      vi.spyOn(crypto, 'randomUUID').mockImplementation(() => {
        throw new Error('private-owner-canary');
      });
    const { engine, reports, native, prepares } = await setup();
    // The real engine selected a tunnel, but a catalogue refusal must precede its readiness work.
    const observed = Promise.allSettled([engine.prepareHttp!(packet())]);
    await flush();
    const [result] = await observed;
    expect(reports.mock.calls).toEqual([
      [
        '[binance] Local catalogue coordination failed; the public catalogue request was not sent',
        'catalogue_coordination',
      ],
    ]);
    expect(result).toMatchObject({
      status: 'rejected',
      reason: { code: 'HTTP_PREPARATION_NETWORK', preparationSource: 'catalogue_coordination' },
    });
    expect(prepares()).toBe(0);
    expect(socket.frames).toEqual([]);
    expect(native.request).not.toHaveBeenCalled();
    await expect(engine.prepareHttp!(packet())).rejects.toMatchObject({ code: 'HTTP_PREPARATION_NETWORK' });
    expect(reports).toHaveBeenCalledTimes(1);
  },
);

it('keeps healthy HOW preparation pending through connection, clock and authentication readiness', async () => {
  socket.open = false;
  socket.synced = false;
  socket.auth = 'Unknown';
  const { engine, reports, native, prepares } = await setup();
  const request = { ...packet('https://fapi.binance.com/fapi/v1/ticker/price'), request: 1 };
  let done = false;
  const pending = Promise.resolve(engine.prepareHttp!(request)).then((value) => {
    done = true;
    return value;
  });
  await flush();
  await vi.advanceTimersByTimeAsync(100);
  expect(done).toBe(false);
  socket.open = true;
  await vi.advanceTimersByTimeAsync(100);
  expect(done).toBe(false);
  socket.synced = true;
  await vi.advanceTimersByTimeAsync(100);
  expect(done).toBe(false);
  expect(socket.frames).toEqual([expect.objectContaining({ type: 'rpc.auth' })]);
  socket.auth = 'Good';
  await vi.advanceTimersByTimeAsync(100);
  const prepared = await pending;
  expect(done).toBe(true);
  expect(reports).not.toHaveBeenCalled();
  expect(socket.frames).toHaveLength(1);
  expect(prepares()).toBe(1);
  const bound = prepared.bind(request);
  expect(bound.ready()).toBe(true);
  expect(await bound.dispatch()).toMatchObject({ status: 200 });
  expect(socket.frames).toEqual([
    expect.objectContaining({ type: 'rpc.auth' }),
    expect.objectContaining({ type: 'rpc.http' }),
  ]);
  expect(native.request).not.toHaveBeenCalled();
});

it('still reports a genuine tunnel readiness deadline as a relay failure and sends no request', async () => {
  socket.open = false;
  const { engine, reports, native } = await setup();
  const pending = Promise.allSettled([
    engine.prepareHttp!(packet('https://fapi.binance.com/fapi/v1/ticker/price', 500)),
  ]);
  await flush();
  await vi.advanceTimersByTimeAsync(500);
  expect((await pending)[0]).toMatchObject({ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } });
  expect(reports.mock.calls).toEqual([[expect.stringContaining('The connection to the Tealstreet relay failed')]]);
  expect(socket.frames).toEqual([]);
  expect(native.request).not.toHaveBeenCalled();
});

it('does not trust a foreign error claiming catalogue provenance', async () => {
  const foreign = Object.assign(new Error('foreign failure'), { preparationSource: 'catalogue_coordination' });
  const { engine, reports, prepares } = await setup(foreign);
  await expect(engine.prepareHttp!(packet('https://fapi.binance.com/fapi/v1/ticker/price'))).rejects.toBe(foreign);
  expect(prepares()).toBe(1);
  expect(reports.mock.calls).toEqual([[expect.stringContaining('The connection to the Tealstreet relay failed')]]);
});

it('does not report a preparation cancelled by its owner', async () => {
  socket.open = false;
  const { engine, reports } = await setup();
  const owner = new AbortController();
  const pending = Promise.allSettled([
    engine.prepareHttp!({ ...packet('https://fapi.binance.com/fapi/v1/ticker/price'), signal: owner.signal }),
  ]);
  await flush();
  owner.abort(new Error('owner retired'));
  await vi.advanceTimersByTimeAsync(25);
  expect((await pending)[0]).toMatchObject({ status: 'rejected' });
  expect(reports).not.toHaveBeenCalled();
  expect(socket.frames).toEqual([]);
});

it('attributes an unchanged deadline spent waiting on a peer catalogue lease to local coordination', async () => {
  const { sharedBinanceCatalogue } = await import('./binanceCatalogue');
  const route = { network: 'mainnet' as const, route: `how:jp:${new URL(tunnelEgressUrl('jp')).href}` };
  const observe = vi.spyOn(sharedBinanceCatalogue, 'prepare');
  const peer = await sharedBinanceCatalogue.prepare(packet(), new AbortController().signal, Date.now() + 5_000, route);
  const { engine, reports, prepares, native } = await setup();
  const started = Date.now();
  const pending = Promise.allSettled([engine.prepareHttp!(packet(CATALOGUE_URL, 500))]);
  // Let IndexedDB callbacks settle between fake timer steps, as in the catalogue lease tests.
  for (let elapsed = 0; elapsed < 500; elapsed += 100) {
    await flush();
    await vi.advanceTimersByTimeAsync(100);
  }
  await flush();
  const [result] = await pending;
  expect(observe.mock.calls[1][3]).toEqual(route);
  expect(prepares()).toBe(0);
  expect(Date.now() - started).toBe(500);
  expect(result).toMatchObject({ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } });
  expect(reports.mock.calls).toEqual([
    [
      '[binance] Local catalogue coordination failed; the public catalogue request was not sent',
      'catalogue_coordination',
    ],
  ]);
  expect(socket.frames).toEqual([]);
  expect(native.request).not.toHaveBeenCalled();
  peer?.cancel();
  await flush();
});
