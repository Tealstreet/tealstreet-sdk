import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import type { CommandContext, CommandResult } from '../Command';

import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { RUST_SOCKET_STALE_MS } from '@tealstreet/safe-cex/src/exchanges/rust/rustSockets';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import { engineCapabilities } from '@tealstreet/safe-cex/src/__tests__/fixtures/engineCapabilities';

import { ConnectionMonitor } from '../../utils/ConnectionMonitor';
import { BaseCommand } from '../BaseCommand';
import { ConnectionCommand } from './ConnectionCommand';

/**
 * Cutover blocker B5: CLI connection commands against a Rust engine account.
 * The engine and transport are fakes; the exchange, its host and its sockets
 * are the real ones.
 */
type Handlers = Parameters<Transport['connect']>[1];

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.useRealTimers();
});

const connected = async () => {
  const sockets: Array<{ url: string; handlers: Handlers; closed: boolean }> = [];
  const engineCloses: number[] = [];
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('bybit'),
    start: () =>
      JSON.stringify([
        { op: 'wsConnect', socket: 1, url: 'wss://venue.test/public', channel: 'public' },
        { op: 'wsConnect', socket: 2, url: 'wss://venue.test/private', channel: 'private' },
      ]),
    command: () => '[]',
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
    wsOpen: () => '[]',
    wsMessage: () => '[]',
    wsClose: (socket) => {
      engineCloses.push(socket);
      return '[]';
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
      const socket = { url, handlers, closed: false };
      sockets.push(socket);
      return { send: () => {}, close: () => void (socket.closed = true) };
    },
    request: () => new Promise(() => {}),
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, store);
  exchanges.push(exchange);
  // Start without waiting for native readiness; the sockets are what matter here.
  void exchange.start().catch(() => {});
  await vi.waitFor(() => expect(sockets).toHaveLength(2));
  for (const socket of sockets) {
    socket.handlers.onOpen();
    socket.handlers.onMessage('{}');
  }
  return { exchange, sockets, engineCloses };
};

const context = (exchange: RustEngineExchange, ...args: string[]) => {
  const printed: string[] = [];
  const print = (message: string) => printed.push(message);
  const ctx = {
    raw: args.join(' '),
    args: { _: args },
    symbol: 'BTCUSDT',
    accountName: 'acct',
    exchange,
    adapter: { print, printInfo: print, printError: print, printSuccess: print, printWarning: print },
  } as unknown as CommandContext;
  return { ctx, printed };
};

class GateProbe extends BaseCommand {
  readonly name = 'probe';
  readonly description = '';
  async execute(ctx: CommandContext): Promise<CommandResult> {
    const blocked = this.healthGate(ctx);
    return blocked ? this.error(blocked) : this.success();
  }
}

describe('CLI connection commands on a Rust engine account', () => {
  it('connection reconnect closes the live engine sockets', async () => {
    const { exchange, sockets, engineCloses } = await connected();
    const { ctx, printed } = context(exchange, 'connection', 'reconnect');
    const result = await new ConnectionCommand().execute(ctx);
    expect(result.success).toBe(true);
    expect(printed).toContain('Triggered reconnection for 3 WebSocket(s)');
    expect(sockets.every((socket) => socket.closed)).toBe(true);
    expect(engineCloses.sort()).toEqual([1, 2]);
  });

  it('ConnectionMonitor reads live stream timestamps', async () => {
    const { exchange } = await connected();
    const status = new ConnectionMonitor(exchange as never).getStatus();
    expect(status.private.status).toBe('healthy');
    expect(status.public.status).toBe('healthy');
  });

  it('the stale-stream gate blocks new exposure once the private stream goes quiet', async () => {
    const { exchange } = await connected();
    expect((await new GateProbe().execute(context(exchange).ctx)).success).toBe(true);
    vi.useFakeTimers({ now: Date.now() + RUST_SOCKET_STALE_MS + 1 });
    exchange.syncSocketHealthToStore();
    expect((await new GateProbe().execute(context(exchange).ctx)).success).toBe(false);
  });
});
