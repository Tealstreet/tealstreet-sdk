import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { RUST_SOCKET_STALE_MS, RustSocketHandle } from '../../exchanges/rust/rustSockets';
import { DefaultStore } from '../../store/store.base';
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

/**
 * A venue with one public and one private socket that reopens a socket the
 * moment the engine reports it closed, like every engine venue's reconnect.
 */
const setup = async (singleSocket?: 'keyed' | 'public', workerMirror = false) => {
  const venue = singleSocket ? 'hyperliquid' : 'bybit';
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
          : [connect('public'), connect('private')],
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
    now: () => Date.now(),
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
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange(
    venue,
    {
      ...(singleSocket === 'public' ? { key: '', secret: '', publicDataOnly: true } : { key: 'k', secret: 's' }),
      workerMirror,
    },
    store,
  );
  exchanges.push(exchange);
  await exchange.start();
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

  it('reports observed channel traffic to handler diagnostics without inventing topic callbacks', async () => {
    const { exchange, store, deliver, live } = await setup();
    expect(exchange.privateWebsocket!.messageHandlersStats).toEqual({});
    deliver('private');
    const timestamp = store.socketHealth!.privateLastMessage;
    expect(exchange.privateWebsocket!.messageHandlersStats).toEqual({
      'engine:private': { lastMessageTimestamp: timestamp },
    });
    live('private').handlers.onClose(1006);
    exchange.syncSocketHealthToStore();
    expect(exchange.privateWebsocket!.messageHandlersStats).toEqual({
      'engine:private': { lastMessageTimestamp: timestamp },
    });
  });

  it('uses worker socket observations for a page mirror, including stale traffic during reconnect', () => {
    const store = new DefaultStore();
    const owner = { socketHost: undefined, isDisposed: false };
    const socket = new RustSocketHandle('public', owner, () => store.socketHealth);
    expect(socket.messageHandlersStats).toEqual({});
    store.updateSocketHealth({ publicInitialized: false, publicLastMessage: 123_456 });
    expect(socket.messageHandlersStats).toEqual({ 'engine:public': { lastMessageTimestamp: 123_456 } });
  });

  it('wires page-mirror exchange handles to worker-published channel timestamps', async () => {
    const { exchange, store } = await setup(undefined, true);
    expect(exchange.socketHost).toBeUndefined();
    store.updateSocketHealth({ publicLastMessage: 123_456, privateLastMessage: 234_567 });
    expect(exchange.publicWebsocket!.messageHandlersStats).toEqual({
      'engine:public': { lastMessageTimestamp: 123_456 },
    });
    expect(exchange.privateWebsocket!.messageHandlersStats).toEqual({
      'engine:private': { lastMessageTimestamp: 234_567 },
    });
  });

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
