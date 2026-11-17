import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { RUST_SOCKET_STALE_MS, RustSocketHandle } from '../../exchanges/rust/rustSockets';
import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from '../cutover/nativeFixture';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

type Handlers = Parameters<Transport['connect']>[1];
interface FakeSocket {
  url: string;
  handlers: Handlers;
  closed: boolean;
}

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.useRealTimers();
});

/** A venue that reopens closed sockets, optionally with two public sockets
 * to exercise physical-socket isolation within the same channel. */
const setup = async (
  singleSocket?: 'keyed' | 'public',
  venueOverride?: 'blofin' | 'okx' | 'bybit' | 'cryptocom',
  now = () => Date.now(),
  publicSibling = false,
) => {
  const venue = venueOverride ?? (singleSocket ? 'hyperliquid' : 'bybit');
  const sockets: FakeSocket[] = [];
  const engineCloses: Array<[number, number]> = [];
  let nextSocket = 1;
  const channels = new Map<number, 'public' | 'private'>();
  const connect = (channel: 'public' | 'private') => {
    const socket = nextSocket++;
    channels.set(socket, channel);
    return {
      op: 'wsConnect',
      socket,
      url: `wss://venue.test/${channel}/${socket}`,
      channel,
      ...(singleSocket ? { publicTopics: true } : {}),
    };
  };
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities(venue),
    start: () =>
      JSON.stringify(
        singleSocket
          ? [connect(singleSocket === 'keyed' ? 'private' : 'public')]
          : [connect('public'), connect('private'), ...(publicSibling ? [connect('public')] : [])],
      ),
    command: (json) => {
      const command = JSON.parse(json);
      return command.command ? JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]) : '[]';
    },
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
    wsOpen: () => '[]',
    wsMessage: (socket, text) => {
      if (text === 'deactivate') return JSON.stringify([{ op: 'deactivate' }]);
      if (text === 'rejected-and-deactivated')
        return JSON.stringify([
          { op: 'event', level: 'error', code: 'auth.rejected', message: 'key rejected' },
          { op: 'deactivate' },
        ]);
      if (text === 'rejected')
        return JSON.stringify([{ op: 'event', level: 'error', code: 'auth.rejected', message: 'key rejected' }]);
      if (text === 'drop') return JSON.stringify([{ op: 'wsClose', socket }]);
      return '[]';
    },
    wsRouteExhausted: () => '',
    wsClose: (socket, code) => {
      engineCloses.push([socket, code]);
      return JSON.stringify([connect(channels.get(socket)!)]);
    },
    httpStart: () => '[]',
    httpChunk: () => '[]',
    httpEnd: () => '[]',
    httpFailed: () => '[]',
    timer: () => '[]',
  };
  setWasmEngineLoader(async () => ({
    WasmEngine: function () {
      return engine;
    } as unknown as WasmEngineModule['WasmEngine'],
  }));
  setRustEngineTransport(() => ({
    now,
    connect: (url, handlers) => {
      const socket: FakeSocket = { url, handlers, closed: false };
      sockets.push(socket);
      return {
        send: () => {},
        close: () => {
          socket.closed = true;
        },
      };
    },
    request: () => new Promise(() => {}),
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const store = new FrameStore();
  installNativeStartupFrames(engine, store);
  store.apply(
    nativeFixture(
      { loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } },
      store.markets.map((row) => row.symbol),
    ),
  );
  const exchange = new RustEngineExchange(
    venue,
    singleSocket === 'public'
      ? { key: '', secret: '', publicDataOnly: true }
      : { key: 'k', secret: 's', passphrase: 'fixture' },
    store,
  );
  exchanges.push(exchange);
  const started = exchange.start();
  if (vi.isFakeTimers()) await vi.advanceTimersByTimeAsync(500);
  await started;
  const live = (channel: string) => sockets.filter((s) => s.url.includes(`/${channel}/`) && !s.closed).at(-1)!;
  const deliver = (channel: string, text = '{}') => {
    const socket = live(channel);
    socket.handlers.onOpen();
    socket.handlers.onMessage(text);
  };
  return { exchange, store, sockets, engineCloses, live, deliver };
};

describe('Rust engine socket health (cutover B5)', () => {
  it.each(['keyed', 'public'] as const)(
    'publishes public health for Hyperliquid %s on its one socket',
    async (mode) => {
      const { exchange, store, sockets, deliver, live } = await setup(mode);
      expect(sockets).toHaveLength(1);
      expect(store.latency).toBeNull();
      expect(store.socketHealth?.publicInitialized).toBe(false);
      const primary = mode === 'keyed' ? 'private' : 'public';
      deliver(primary);
      expect(store.socketHealth).toMatchObject({ publicInitialized: true, publicHealthy: true });
      expect(exchange.publicWebsocket?.isInitialized).toBe(true);
      if (mode === 'keyed') expect(store.socketHealth?.privateInitialized).toBe(true);
      live(primary).handlers.onClose(1006);
      exchange.syncSocketHealthToStore();
      expect(store.socketHealth?.publicInitialized).toBe(false);
      if (mode === 'keyed') expect(store.socketHealth?.privateInitialized).toBe(false);
    },
  );

  it('frames write store.socketHealth from engine sockets', async () => {
    const { exchange, store, deliver } = await setup();
    expect(store.socketHealth).toMatchObject({ privateInitialized: false, publicInitialized: false });

    deliver('public');
    deliver('private');
    expect(store.socketHealth).toMatchObject({
      publicInitialized: true,
      publicHealthy: true,
      privateInitialized: true,
      privateHealthy: true,
      privateReconnectPaused: false,
    });
    expect(store.socketHealth?.privateLastMessage).toBeGreaterThan(0);
    // CLI ConnectionMonitor and the reconnect banner read these.
    expect(exchange.privateWebsocket?.lastReceived).toBe(store.socketHealth?.privateLastMessage);
    expect(exchange.publicWebsocket?.lastReceived).toBeGreaterThan(0);
    // web createLimitOrder reads this off a venue-specific private socket.
    expect((exchange.privateWebsocket as unknown as RustSocketHandle).isReadonlyFallbackActive()).toBe(false);
  });

  it('publishes the transport close attribution in the existing account socket health', async () => {
    const { exchange, store, live, deliver } = await setup();
    deliver('private');
    live('private').handlers.onClose(1006);
    exchange.syncSocketHealthToStore();
    expect(store.socketHealth?.reconnectDiagnostics?.private).toMatchObject({
      initiator: 'unknown',
      phase: 'established',
      route: 'direct',
      port: '443',
      lifetime: '<5s',
    });
    const snapshot = store.socketHealth?.reconnectDiagnostics;
    exchange.syncSocketHealthToStore();
    expect(store.socketHealth?.reconnectDiagnostics).toBe(snapshot);
  });

  it('keeps the page-clock private message timestamp while its reconnect is uninitialized', async () => {
    const { exchange, store, live, deliver } = await setup();
    deliver('private');
    const last = store.socketHealth!.privateLastMessage;
    expect(last).toBeGreaterThan(0);
    live('private').handlers.onClose(1006);
    exchange.syncSocketHealthToStore();
    expect(store.socketHealth!.privateInitialized).toBe(false);
    expect(store.socketHealth!.privateLastMessage).toBe(last);
    expect(exchange.privateWebsocket!.lastReceived).toBe(last);
  });

  it('a silent private stream turns unhealthy on the periodic sync', async () => {
    const { exchange, store, deliver } = await setup();
    deliver('private');
    vi.useFakeTimers({ now: Date.now() + RUST_SOCKET_STALE_MS + 1 });
    exchange.syncSocketHealthToStore();
    // cli-core's stale-stream gate blocks on exactly this pair.
    expect(store.socketHealth).toMatchObject({ privateInitialized: true, privateHealthy: false });
  });

  it.each(['blofin', 'okx', 'bybit'] as const)(
    'the first runnable health check after sleep retires %s stale sockets despite a paused engine clock',
    async (venue) => {
      vi.useFakeTimers({ now: 1_800_000_000_000 });
      let engineNow = Date.now();
      const { exchange, sockets, engineCloses, deliver, store } = await setup(undefined, venue, () => engineNow);
      // Startup advances normally; this control freezes the clock from the sleep onward.
      engineNow = Date.now();
      deliver('public');
      deliver('private');
      // No socket close callback or native supervisor deadline runs while the laptop sleeps.
      vi.setSystemTime(Date.now() + 101_012);
      await vi.advanceTimersByTimeAsync(5_000);
      expect(engineCloses.sort()).toEqual([
        [1, 1006],
        [2, 1006],
      ]);
      expect(sockets.slice(0, 2).every((socket) => socket.closed)).toBe(true);
      expect(sockets).toHaveLength(4);
      // Aggregate health defaults true without initialized sockets; readiness remains false.
      expect(store.socketHealth?.privateInitialized).toBe(false);
      expect(exchange.privateWebsocket?.isHealthy).toBe(false);
      deliver('private');
      expect(store.socketHealth).toMatchObject({ privateHealthy: true, privateInitialized: true });
    },
  );

  it('wake health recovery leaves a freshly delivering private channel alone', async () => {
    vi.useFakeTimers({ now: 1_800_000_000_000 });
    let engineNow = Date.now();
    const { engineCloses, deliver, live } = await setup(undefined, undefined, () => engineNow);
    engineNow = Date.now();
    deliver('public');
    deliver('private');
    vi.setSystemTime(Date.now() + 101_012);
    deliver('private');
    const privateSocket = live('private');
    await vi.advanceTimersByTimeAsync(5_000);
    expect(engineCloses).toEqual([[1, 1006]]);
    expect(privateSocket.closed).toBe(false);
  });

  it.each([50_000, 101_012])(
    'retains the %i ms sleep obligation while the engine clock resumes normally',
    async (sleptMs) => {
      vi.useFakeTimers({ now: 1_800_000_000_000 });
      let sleepOffset = 0;
      const { exchange, engineCloses, deliver, live, store } = await setup(
        undefined,
        'blofin',
        () => Date.now() - sleepOffset,
      );
      deliver('private');
      const original = live('private');
      sleepOffset = sleptMs;
      vi.setSystemTime(Date.now() + sleptMs);
      await vi.advanceTimersByTimeAsync(5_000);
      if (sleptMs === 50_000) {
        expect(original.closed).toBe(false);
        expect(engineCloses).toEqual([]);
        expect(store.socketHealth).toMatchObject({ privateInitialized: true, privateHealthy: true });
      } else {
        expect(original.closed).toBe(true);
        expect(engineCloses).toEqual([[2, 1006]]);
      }
      await vi.advanceTimersByTimeAsync(10_000);
      expect(original.closed).toBe(true);
      expect(engineCloses).toEqual([[2, 1006]]);
      expect(exchange.privateWebsocket?.isInitialized).toBe(false);
    },
  );

  it.each(['fresh traffic', 'replacement', 'disposal'] as const)(
    'retires a pending short-sleep obligation on %s',
    async (recovery) => {
      vi.useFakeTimers({ now: 1_800_000_000_000 });
      let sleepOffset = 0;
      const { exchange, engineCloses, deliver, live } = await setup(
        undefined,
        'cryptocom',
        () => Date.now() - sleepOffset,
      );
      deliver('private');
      sleepOffset = 50_000;
      vi.setSystemTime(Date.now() + sleepOffset);
      await vi.advanceTimersByTimeAsync(5_000);
      expect(engineCloses).toEqual([]);
      if (recovery === 'replacement') exchange.privateWebsocket!.reconnect();
      if (recovery === 'disposal') exchange.dispose();
      else deliver('private');
      const recovered = recovery === 'disposal' ? undefined : live('private');
      await vi.advanceTimersByTimeAsync(65_000);
      expect(engineCloses).toEqual(recovery === 'replacement' ? [[2, 1006]] : []);
      expect(recovered?.closed ?? true).toBe(recovery === 'disposal');
    },
  );

  it.each([2_000, 50_000])('does not attach a %i ms sleep obligation to already resumed traffic', async (sleptMs) => {
    vi.useFakeTimers({ now: 1_800_000_000_000 });
    let sleepOffset = 0;
    const { engineCloses, deliver, live } = await setup(undefined, 'cryptocom', () => Date.now() - sleepOffset);
    deliver('private');
    sleepOffset = sleptMs;
    vi.setSystemTime(Date.now() + sleepOffset);
    deliver('private');
    const fresh = live('private');
    await vi.advanceTimersByTimeAsync(65_000);
    expect(engineCloses).toEqual([]);
    expect(fresh.closed).toBe(false);
  });

  it.each(['uninitialized', 'fresh'] as const)(
    'wake recovery isolates a stale physical socket from its %s same-channel sibling',
    async (state) => {
      vi.useFakeTimers({ now: 1_800_000_000_000 });
      let sleepOffset = 0;
      const { sockets, engineCloses } = await setup(undefined, 'bybit', () => Date.now() - sleepOffset, true);
      const [original, , sibling] = sockets;
      original.handlers.onOpen();
      original.handlers.onMessage('{}');
      sleepOffset = 101_012;
      vi.setSystemTime(Date.now() + sleepOffset);
      sibling.handlers.onOpen();
      if (state === 'fresh') sibling.handlers.onMessage('{}');
      await vi.advanceTimersByTimeAsync(5_000);
      expect(original.closed).toBe(true);
      expect(sibling.closed).toBe(false);
      expect(engineCloses).toEqual([[1, 1006]]);
    },
  );

  it('wake health recovery preserves an uninitialized replacement and stops on disposal', async () => {
    vi.useFakeTimers({ now: 1_800_000_000_000 });
    let engineNow = Date.now();
    const { exchange, engineCloses, deliver, live } = await setup(undefined, undefined, () => engineNow);
    engineNow = Date.now();
    deliver('private');
    vi.setSystemTime(Date.now() + 101_012);
    live('private').handlers.onClose(1006);
    const replacement = live('private');
    replacement.handlers.onOpen();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(engineCloses).toEqual([[2, 1006]]);
    expect(replacement.closed).toBe(false);
    exchange.dispose();
    await vi.advanceTimersByTimeAsync(120_000);
    expect(engineCloses).toEqual([[2, 1006]]);
  });

  it.each(['ordinary callbacks', 'withheld callbacks'] as const)(
    'preserves Crypto.com native 90s silence allowance with advancing clocks and %s',
    async (mode) => {
      vi.useFakeTimers({ now: 1_800_000_000_000 });
      const { engineCloses, deliver, live } = await setup(undefined, 'cryptocom');
      deliver('private');
      const privateSocket = live('private');
      // The 60s display threshold must not replace the native 90s retirement policy.
      if (mode === 'withheld callbacks') vi.setSystemTime(Date.now() + 65_000);
      await vi.advanceTimersByTimeAsync(mode === 'ordinary callbacks' ? 65_000 : 5_000);
      expect(engineCloses).toEqual([]);
      expect(privateSocket.closed).toBe(false);
    },
  );

  it('reconnect closes every open engine socket and the venue reopens it', async () => {
    const { exchange, store, sockets, engineCloses, deliver } = await setup();
    deliver('public');
    deliver('private');
    expect(exchange.getAllSockets()).toHaveLength(3);

    exchange.reconnect();
    expect(sockets.slice(0, 2).every((socket) => socket.closed)).toBe(true);
    expect(engineCloses.sort()).toEqual([
      [1, 1006],
      [2, 1006],
    ]);
    expect(sockets).toHaveLength(4);
    expect(store.socketHealth).toMatchObject({
      privateInitialized: false,
      privateCloseCode: 1006,
      privateCloseCountTotal: 1,
      privateCloseStreak: 1,
      publicCloseCountTotal: 1,
    });

    deliver('private');
    expect(store.socketHealth).toMatchObject({
      privateInitialized: true,
      privateCloseStreak: 0,
      privateCloseCountTotal: 1,
    });
  });

  it('recoverAfterOffline acts and reports recovery until the engine is gone', async () => {
    const { exchange, engineCloses, deliver } = await setup();
    deliver('private');
    expect(exchange.recoverAfterOffline()).toBe(true);
    // The public socket is still connecting, so it is left alone.
    expect(engineCloses).toEqual([[2, 1006]]);
    exchange.dispose();
    // The web reloads the account when recovery is impossible in place.
    expect(exchange.recoverAfterOffline()).toBe(false);
  });

  it('engine-initiated socket closes count like venue closes', async () => {
    const { store, deliver } = await setup();
    deliver('private', 'drop');
    expect(store.socketHealth).toMatchObject({ privateCloseCode: 1000, privateCloseCountTotal: 1 });
  });

  it('credential rejections raise authFailureStreak until the private stream delivers again', async () => {
    const { exchange, deliver } = await setup();
    deliver('private', 'rejected');
    // The first frame proved nothing yet: the same frame rejected the key.
    expect(exchange.authFailureStreak).toBe(1);
    // Separate frames are separate tasks.
    await Promise.resolve();
    deliver('public', 'deactivate');
    expect(exchange.authFailureStreak).toBe(2);

    exchange.reconnect();
    deliver('private');
    expect(exchange.authFailureStreak).toBe(0);
  });

  it('one rejection reported as an auth.rejected event and a deactivation counts once', async () => {
    const { exchange, deliver } = await setup();
    deliver('public', 'rejected-and-deactivated');
    expect(exchange.authFailureStreak).toBe(1);
    await Promise.resolve();
    deliver('public', 'rejected');
    expect(exchange.authFailureStreak).toBe(2);
  });
});
