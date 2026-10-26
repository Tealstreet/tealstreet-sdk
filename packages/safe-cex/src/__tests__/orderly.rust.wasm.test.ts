vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>(), true);
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName } from '../types';

import bs58 from 'bs58';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createExchange } from '../index';
import { RustEngineExchange, setRustEngineTransport } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderType } from '../types';
import { prepareRecordedHttp } from './support/managedHttp';

const aliases: ExchangeName[] = ['tealstreet-dex', 'kodiakfi', 'woofi', 'mode', 'salsadex', 'what'];
const tags = ['TEALSTREET', 'TEALKODIAK', 'TEALSTREET', 'TEALMODE', 'TEALSALSA', 'TEALWHAT'];
const envelope = (data: unknown) => new TextEncoder().encode(JSON.stringify({ success: true, data }));
const transport = (validation = 200) => {
  const writes: Array<{ method: string; url: string; headers: Record<string, string>; body?: string }> = [];
  const sockets: string[] = [];
  const native: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect(url, handlers) {
      sockets.push(url);
      setTimeout(handlers.onOpen, 0);
      return {
        send(text) {
          const message = JSON.parse(text);
          if (message.event === 'auth') setTimeout(() => handlers.onMessage('{"event":"auth","success":true}'), 0);
          if (message.event === 'ping') setTimeout(() => handlers.onMessage('{"event":"pong"}'), 0);
        },
        close() {},
      };
    },
    async request(request) {
      const path = new URL(request.url).pathname;
      if (path === '/v1/client/info' && validation !== 200) return {
        status: validation, body: new TextEncoder().encode('{"success":false,"message":"rejected"}'),
      };
      if (request.method !== 'GET') {
        writes.push(request);
        return { status: 200, body: envelope(path === '/v1/order' ? { order_id: '9007199254740995' } : {}) };
      }
      let data: unknown;
      switch (path) {
        case '/v1/public/info':
          data = { rows: [{ symbol: 'PERP_BTC_USDC', base_tick: 0.001, quote_tick: 0.1, base_min: 0.001, base_max: 20, base_imr: 0.01 }] };
          break;
        case '/v1/public/futures':
          data = { rows: [{ symbol: 'PERP_BTC_USDC', '24h_open': 50000, '24h_close': 51000, mark_price: 51000 }] };
          break;
        case '/v1/client/info': data = { max_leverage: 20 }; break;
        case '/v1/client/margin_modes': data = { rows: [{ symbol: 'PERP_BTC_USDC', default_margin_mode: 'CROSS' }] }; break;
        case '/v1/positions':
          data = { rows: [
            { symbol: 'PERP_BTC_USDC', position_qty: 1, average_open_price: 50000, mark_price: 51000, margin_mode: 'CROSS', leverage: 10 },
            { symbol: 'PERP_BTC_USDC', position_qty: -0.5, average_open_price: 52000, mark_price: 51000, margin_mode: 'ISOLATED', leverage: 5 },
          ], total_collateral_value: 10000, free_collateral: 7000 };
          break;
        case '/v1/orders': case '/v1/algo/orders': data = { rows: [], meta: { total: 0 } }; break;
        default: throw new Error(`Unexpected Orderly read ${request.url}`);
      }
      return { status: 200, body: envelope(data) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { native, writes, sockets };
};

describe('retained Orderly Rust family oracle', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => { exchange?.dispose(); vi.restoreAllMocks(); });

  it.each(aliases)('createExchange loads %s through the default WASM loader', async (name) => {
    const tape = transport();
    setRustEngineTransport(() => tape.native);
    const store = new DefaultStore();
    exchange = await createExchange(name, { key: '', secret: '', publicDataOnly: true }, store) as RustEngineExchange;
    expect(exchange).toBeInstanceOf(RustEngineExchange);
    await exchange.start();
    expect(store.markets.map((m) => m.symbol)).toEqual(['BTCUSDC']);
    expect(tape.sockets[0]).toContain('/ws/stream/OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY');
    expect(tape.writes).toEqual([]);
  });

  it.each(aliases)('%s preserves dual margin identity and exact caller IDs with broker attribution', async (name) => {
    const tape = transport();
    setRustEngineTransport(() => tape.native);
    const store = new DefaultStore();
    exchange = await createExchange(name, {
      key: `ed25519:${bs58.encode(new Uint8Array(32))}`,
      secret: `ed25519:${bs58.encode(new Uint8Array(32).fill(7))}`,
      applicationId: 'documented-test-account', testnet: true,
    }, store) as RustEngineExchange;
    await exchange.start();
    expect(store.positions.map((p) => p.id).sort()).toEqual(['BTCUSDC', 'BTCUSDC:isolated']);
    expect(await exchange.getAccount()).toEqual({ userId: 'documented-test-account', affiliateId: 'documented-test-account' });
    await expect(exchange.placeOrder({ symbol: 'BTCUSDC', type: OrderType.Limit, side: OrderSide.Buy,
      amount: 1, price: 50000, clientOrderId: 'caller-Exact-42' })).resolves.toEqual(['9007199254740995']);
    const payload = JSON.parse(tape.writes[0].body!);
    expect(payload.client_order_id).toBe('caller-Exact-42');
    expect(payload.order_tag).toBe(tags[aliases.indexOf(name)]);
    expect(payload.broker_id).toBe('e708a644-9ce5-46c6-b50c-74b15f62d8ca');
    expect(tape.writes[0].url).toContain('testnet-api-evm.orderly.org');
    await expect(exchange.placeOrder({ symbol: 'BTCUSDC', type: OrderType.Market, side: OrderSide.Buy,
      amount: 1, clientOrderId: 'caller', stopLoss: 49000 })).rejects.toThrow(/clientOrderId cannot be honored/);
    expect(tape.writes).toHaveLength(1);
  });
  it('reports each native rejection once and preserves partial acknowledgements and receipts', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'trace').mockImplementation(() => {});
    const tape = transport();
    const request = tape.native.request;
    let submissions = 0;
    tape.native.request = (input) => {
      if (input.method === 'POST' && new URL(input.url).pathname === '/v1/order' && ++submissions === 2) {
        tape.writes.push(input);
        return Promise.resolve({ status: 200, body: new TextEncoder().encode(JSON.stringify({
          success: false, message: 'Second fixture order refused',
        })) });
      }
      return request(input);
    };
    setRustEngineTransport(() => tape.native);
    exchange = await createExchange('tealstreet-dex', {
      key: `ed25519:${bs58.encode(new Uint8Array(32))}`,
      secret: `ed25519:${bs58.encode(new Uint8Array(32).fill(7))}`,
      applicationId: 'documented-test-account',
    }) as RustEngineExchange;
    const errors: string[] = [];
    const logs: string[] = [];
    exchange.on('error', (message: string) => errors.push(message));
    exchange.on('log', (message: string) => logs.push(message));
    await exchange.start();
    const order = { symbol: 'BTCUSDC', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 50000 };
    // Finite zero reaches native validation, which emits a diagnostic plus DoneErr.
    for (let count = 1; count <= 2; count++) {
      await expect(exchange.placeOrder({ ...order, amount: 0 })).rejects.toThrow(/market minimum/);
      expect(errors).toHaveLength(count);
    }
    expect(errors[0]).toBe(errors[1]);
    expect(tape.writes).toHaveLength(0);
    const failure = await exchange.placeOrders([
      { ...order, clientOrderId: 'receipt-first' },
      { ...order, clientOrderId: 'receipt-second' },
    ]).catch((error: unknown) => error);
    expect(failure).toMatchObject({
      message: expect.stringContaining('Second fixture order refused'),
      partialOrderIds: ['9007199254740995'], partialOrderCount: 1, totalOrderCount: 2,
      actualClientOrderIds: ['receipt-first', 'receipt-second'],
    });
    expect(failure).not.toHaveProperty('outcome');
    expect(errors).toHaveLength(3);
    expect(errors[2]).toContain('Second fixture order refused');
    expect(logs.some((line) => line.includes('Second fixture order refused'))).toBe(true);
    expect(tape.writes).toHaveLength(2);
    expect(exchange.isEngineDead).toBe(false);
  }, 20000);

  it('rejects terminal startup and releases its subscription', async () => {
    const tape = transport(401);
    setRustEngineTransport(() => tape.native);
    exchange = await createExchange('tealstreet-dex', {
      key: `ed25519:${bs58.encode(new Uint8Array(32))}`,
      secret: `ed25519:${bs58.encode(new Uint8Array(32).fill(7))}`,
      applicationId: 'documented-test-account',
    }) as RustEngineExchange;
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const deactivated = vi.fn();
    exchange.on('deactivate', deactivated);
    await expect(exchange.start()).rejects.toThrow(/credentials were rejected/);
    expect(deactivated).toHaveBeenCalledTimes(1);
    expect((exchange as any).startupWaiters.size).toBe(0);
  });

  it('rejects malformed credentials before network access', async () => {
    const tape = transport();
    setRustEngineTransport(() => tape.native);
    exchange = await createExchange('tealstreet-dex', { key: 'invalid', secret: 'invalid', applicationId: 'account' }) as RustEngineExchange;
    await expect(exchange.start()).rejects.toThrow(/base58-encoded 32-byte/);
    expect(tape.sockets).toEqual([]);
    expect(tape.writes).toEqual([]);
  });

  it('disposal settles a startup waiting for its first response', async () => {
    const tape = transport();
    tape.native.request = () => new Promise(() => {});
    setRustEngineTransport(() => tape.native);
    exchange = await createExchange('tealstreet-dex', { key: '', secret: '', publicDataOnly: true }) as RustEngineExchange;
    const pending = expect(exchange.start()).rejects.toThrow(/disposed during startup/);
    await new Promise((resolve) => setTimeout(resolve, 10));
    exchange.dispose();
    await pending;
    expect((exchange as any).startupWaiters.size).toBe(0);
  });

});
