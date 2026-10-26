vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>(), true);
});

import type { CommandProgress, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName } from '../types';

import { createPrivateKey, createPublicKey, verify } from 'node:crypto';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createExchange } from '../index';
import { BaseExchange } from '../exchanges/base';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../types';

const names: ExchangeName[] = ['coinbase', 'coinbasespot'];
const keyName = 'organizations/documented/apiKeys/sdk-fixture';
const seed = Buffer.alloc(32, 13);
const publicKey = createPublicKey(createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
  format: 'der', type: 'pkcs8',
}));
const downloaded = JSON.stringify({ name: keyName, privateKey: seed.toString('base64') });
const product = {
  product_id: 'BTC-USD', product_type: 'SPOT', base_currency_id: 'BTC', quote_currency_id: 'USD',
  status: 'TRADING', base_increment: '0.001', price_increment: '0.01', base_min_size: '0.001',
  base_max_size: '100', price: '110', volume_24h: '20', price_percentage_change_24h: '2',
};
const jwtClaims = (token: string) => {
  const parts = token.split('.');
  expect(parts).toHaveLength(3);
  const header = JSON.parse(Buffer.from(parts[0], 'base64url').toString());
  const claims = JSON.parse(Buffer.from(parts[1], 'base64url').toString());
  expect(header).toMatchObject({ alg: 'EdDSA', kid: keyName, typ: 'JWT' });
  expect(claims).toMatchObject({ iss: 'cdp', aud: ['cdp_service'], sub: keyName });
  expect(claims.exp - claims.nbf).toBe(120);
  expect(verify(null, Buffer.from(parts.slice(0, 2).join('.')), publicKey, Buffer.from(parts[2], 'base64url'))).toBe(true);
  return claims;
};
const transport = (profile: 'spot' | 'cfm' = 'spot') => {
  const requests: Array<{ method: string; url: string; headers: Record<string, string>; body?: string }> = [];
  const subscriptions: Array<{ url: string; message: Record<string, any> }> = [];
  const response = (body: unknown, status = 200) => ({ status, body: new TextEncoder().encode(JSON.stringify(body)) });
  const native: Transport = {
    now: Date.now,
    connect(url, handlers) {
      setTimeout(handlers.onOpen, 0);
      return { send(text) { subscriptions.push({ url, message: JSON.parse(text) }); }, close() {} };
    },
    async request(request) {
      requests.push(request);
      const parsed = new URL(request.url);
      const path = parsed.pathname;
      if (request.method === 'POST' && path.endsWith('/orders'))
        return response({ success: true, success_response: { order_id: '9007199254740995' } });
      if (request.method === 'POST' && path.endsWith('/batch_cancel')) {
        const { order_ids } = JSON.parse(request.body!);
        return response({ results: order_ids.map((order_id: string) => ({ order_id, success: true })) });
      }
      if (parsed.hostname === 'api.international.coinbase.com') return response([{
        symbol: 'BTC-PERP', type: 'PERP', trading_state: 'TRADING', base_asset_name: 'BTC', quote_asset_name: 'USDC',
        base_increment: '0.001', quote_increment: '0.01', min_quantity: '0.001', qty_24hr: '20', notional_24hr: '2200',
        quote: { best_bid_price: '109', best_ask_price: '111', trade_price: '110', mark_price: '110', index_price: '110' },
      }]);
      if (path.endsWith('/cfm/balance_summary')) return profile === 'cfm'
        ? response({ balance_summary: { total_usd_balance: { value: '1000' }, available_margin: { value: '800' }, unrealized_pnl: { value: '0.2' } } })
        : response({ message: 'No CFM account' }, 403);
      if (path.endsWith('/cfm/positions')) return response({ positions: [{ product_id: 'BTC-CDE', side: 'LONG',
        number_of_contracts: '2', avg_entry_price: '100', current_price: '110', unrealized_pnl: '0.2' }] });
      if (path.endsWith('/portfolios')) return response({ portfolios: [{ uuid: 'selected-spot-portfolio', type: 'DEFAULT' }] });
      if (path.endsWith('/portfolios/selected-spot-portfolio')) return response({ breakdown: {
        spot_positions: [{ asset: 'BTC', total_balance_crypto: 1.5, available_to_trade_crypto: 1, total_balance_fiat: 165 }],
      } });
      if (path.endsWith('/products')) {
        const row = profile === 'cfm' && parsed.searchParams.get('product_type') === 'FUTURE'
          ? { ...product, product_id: 'BTC-CDE', product_type: 'FUTURE', base_increment: '1', base_min_size: '1',
              future_product_details: { contract_expiry_type: 'EXPIRING', contract_root_unit: 'BTC', contract_size: '0.01' } }
          : product;
        return response({ products: [row], num_products: 1, pagination: { has_next: false } });
      }
      if (path.endsWith('/orders/historical/batch')) return response({ orders: [], has_next: false, cursor: '' });
      throw new Error(`Unexpected Coinbase SDK request ${request.method} ${request.url}`);
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { native, requests, subscriptions };
};

describe('retained Coinbase Rust family oracle', () => {
  let exchange: RustEngineExchange | undefined;
  let mirrored: BaseExchange | undefined;
  afterEach(() => { exchange?.dispose(); exchange = undefined; mirrored?.dispose(); mirrored = undefined; });

  it('delivers queued native IDs after a deadline and retains them in the existing log path if a listener throws', async () => {
    const tape = transport();
    const request = tape.native.request;
    const writes: Array<{ request: Parameters<Transport['request']>[0]; release: (body: Awaited<ReturnType<Transport['request']>>) => void }> = [];
    const progress: CommandProgress[] = [];
    const logs: string[] = [];
    tape.native.request = (input) => {
      if (input.method === 'POST' && new URL(input.url).pathname.endsWith('/orders')) {
        expect(progress.some((event) => event.receipts.some((row) => row.identity === JSON.parse(input.body!).client_order_id))).toBe(true);
        return new Promise((release) => writes.push({ request: input, release }));
      }
      return request(input);
    };
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.native);
    exchange = new RustEngineExchange('coinbasespot', {
      key: downloaded, secret: '', tealstreetTunnel: false, extra: { rustCommandTimeoutMs: 1000 },
    }, new DefaultStore());
    exchange.on('log', (message: string) => logs.push(message));
    exchange.on('command-progress', (event: CommandProgress) => {
      progress.push(event);
      if (event.late && event.receipts.length) throw new Error('Consumer receipt listener failed');
    });
    await exchange.start();
    const order = { symbol: 'BTC-USD', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.1, price: 99 };
    const first = exchange.placeOrder(order).catch((error: unknown) => error);
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    const second = exchange.placeOrder(order).catch((error: unknown) => error);
    const [firstFailure, secondFailure] = await Promise.all([first, second]) as Array<Error & {
      receiptKey: string; orderIdentities: Array<{ identity: string }>;
    }>;
    expect(firstFailure).toMatchObject({ code: 'COMMAND_TIMEOUT', outcome: 'unknown' });
    expect(secondFailure).toMatchObject({ code: 'COMMAND_TIMEOUT', outcome: 'unknown', orderIdentities: [] });
    expect(writes).toHaveLength(1);
    const firstId = JSON.parse(writes[0].request.body!).client_order_id;
    expect(firstFailure.orderIdentities).toEqual([{ symbol: 'BTC-USD', kind: 'clientOrderId', identity: firstId }]);
    writes[0].release({ status: 200, body: new TextEncoder().encode(JSON.stringify({
      success: true, success_response: { order_id: 'receipt-order-first' },
    })) });
    await vi.waitFor(() => expect(writes).toHaveLength(2));
    const secondId = JSON.parse(writes[1].request.body!).client_order_id;
    expect(secondId).not.toBe(firstId);
    expect(secondId).toMatch(/^tealstreet-/);
    expect(progress).toContainEqual(expect.objectContaining({
      receiptKey: secondFailure.receiptKey, late: true, complete: false,
      receipts: [{ symbol: 'BTC-USD', kind: 'clientOrderId', identity: secondId }],
    }));
    expect(logs.some((line) => line.includes(secondFailure.receiptKey) && line.includes(secondId))).toBe(true);
    expect(secondFailure.orderIdentities).toEqual([]);
    expect(exchange.isEngineDead).toBe(false);
    expect(writes).toHaveLength(2);
  }, 20000);

  it.each([
    ['coinbasespot', true],
    ['coinbase', false],
    ['grvt', false],
    ['bybitspot', true],
  ] as const)('%s preserves its public spot identity', (name, isSpot) => {
    exchange = new RustEngineExchange(name, { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    expect(exchange.isPublicDataOnly).toBe(true);
    expect(exchange.isSpot).toBe(isSpot);
  });

  it.each(names)('%s selects private startup from downloaded JSON, returns native portfolio and preserves native commands', async (name) => {
    const tape = transport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.native);
    const store = new DefaultStore();
    exchange = await createExchange(name, { key: downloaded, secret: '', tealstreetTunnel: false }, store) as RustEngineExchange;
    expect(exchange).toBeInstanceOf(RustEngineExchange);
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(await exchange.getAccount()).toEqual({ userId: 'selected-spot-portfolio' });
    expect(store.markets.map((m) => m.symbol)).toEqual(['BTC-USD']);
    // Non-finite optional fields must fail before JSON could turn them into null
    // and silently widen a read or strip a protective bracket from a write.
    const beforeInvalid = tape.requests.length;
    await expect(exchange.fetchHistoricalFills({ symbol: 'BTC-USD', from: Infinity })).rejects.toThrow(/bounds must be finite/);
    await expect(exchange.fetchHistoricalFills({ symbol: 'BTC-USD', to: NaN })).rejects.toThrow(/bounds must be finite/);
    await expect(exchange.fetchHistoricalOrders({ symbol: 'BTC-USD', from: Infinity })).rejects.toThrow(/numbers must be finite/);
    await expect(exchange.fetchOHLCV({ symbol: 'BTC-USD', interval: '1m', from: NaN })).rejects.toThrow(/numbers must be finite/);
    await expect(exchange.placeOrder({ symbol: 'BTC-USD', type: OrderType.Market, side: OrderSide.Buy,
      amount: 1, stopLoss: NaN })).rejects.toThrow(/must be finite/);
    expect(tape.requests).toHaveLength(beforeInvalid);
    await expect(exchange.placeOrder({ symbol: 'BTC-USD', type: OrderType.Limit, side: OrderSide.Buy,
      amount: 0.125, price: 99.25, timeInForce: OrderTimeInForce.PostOnly, clientOrderId: 'SDK-Exact-42' })).resolves.toEqual(['9007199254740995']);
    const writes = tape.requests.filter((r) => r.method !== 'GET');
    expect(writes).toHaveLength(1);
    expect(JSON.parse(writes[0].body!)).toEqual({ client_order_id: 'SDK-Exact-42', product_id: 'BTC-USD',
      side: 'BUY', leverage: '10', order_configuration: { limit_limit_gtc: { base_size: '0.125', limit_price: '99.25', post_only: true } } });
    await expect(exchange.placeOrder({ symbol: 'BTC-USD', type: OrderType.Market, side: OrderSide.Buy,
      amount: 1, clientOrderId: 'must-not-drop', stopLoss: 90 })).rejects.toThrow(/clientOrderId cannot be honored/);
    await expect(exchange.placeOrder({ symbol: 'BTC-USD', type: OrderType.Market, side: OrderSide.Buy,
      amount: 1, clientOrderId: 'x'.repeat(129) })).rejects.toThrow(/clientOrderId/);
    expect(tape.requests.filter((r) => r.method !== 'GET')).toHaveLength(1);
    await expect.poll(() => store.orders.find((o) => o.id === '9007199254740995')?.clientOrderId).toBe('SDK-Exact-42');
    const order = store.orders.find((o) => o.id === '9007199254740995')!;
    expect(order.clientOrderId).toBe('SDK-Exact-42');
    await exchange.cancelOrders([order]);
    expect(JSON.parse(tape.requests.filter((r) => r.method !== 'GET')[1].body!)).toEqual({ order_ids: ['9007199254740995'] });
    for (const request of tape.requests) {
      if (new URL(request.url).pathname.includes('/market/')) continue;
      const authorization = request.headers.Authorization;
      expect(authorization).toMatch(/^Bearer /);
      expect(jwtClaims(authorization.slice(7)).uri).toBe(`${request.method} api.coinbase.com${new URL(request.url).pathname}`);
    }
    const privateMessages = tape.subscriptions.filter((s) => s.url.includes('-ws-user.'));
    expect(privateMessages.map((s) => s.message.channel).sort()).toEqual(['heartbeats', 'user']);
    expect(new Set(privateMessages.map((s) => s.message.jwt)).size).toBe(2);
    for (const { message } of privateMessages) expect(jwtClaims(message.jwt).uri).toBeUndefined();
    if (name === 'coinbasespot') expect(tape.requests.every((r) => !/\/(cfm|intx)\//.test(r.url))).toBe(true);
  }, 20000);

  it('preserves CFM native placement units across the real WASM frame boundary', async () => {
    const tape = transport('cfm');
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.native);
    const store = new DefaultStore();
    const mainStore = new DefaultStore();
    mirrored = new BaseExchange('coinbase', { key: '', secret: '' }, mainStore);
    exchange = await createExchange('coinbase', { key: downloaded, secret: '', tealstreetTunnel: false }, store) as RustEngineExchange;
    exchange.onFrame = (frame) => mirrored!.applyWorkerFrame(frame.slice(0));
    await exchange.start();
    expect(store.marketsBySymbol['BTC-CDE'].contractSize).toBe(0.01);
    expect(store.positions.find((p) => p.symbol === 'BTC-CDE')).toMatchObject({
      contracts: 0.02, extra: { contractsNative: 2 },
    });
    expect(mainStore.positions.find((p) => p.symbol === 'BTC-CDE')).toMatchObject({
      contracts: 0.02, extra: { contractsNative: 2 },
    });
    await expect(exchange.placeOrder({ symbol: 'BTC-CDE', type: OrderType.Limit, side: OrderSide.Buy,
      amount: 3, price: 100, clientOrderId: 'CFM-native-three' })).resolves.toEqual(['9007199254740995']);
    const placed = tape.requests.find((r) => r.method === 'POST' && new URL(r.url).pathname.endsWith('/orders'))!;
    expect(JSON.parse(placed.body!).order_configuration.limit_limit_gtc.base_size).toBe('3');
    await expect.poll(() => store.orders.find((o) => o.id === '9007199254740995')).toMatchObject({
      symbol: 'BTC-CDE', amount: 0.03, filled: 0, remaining: 0.03,
      extra: { remainingNative: 3, filledInPlacementUnits: 0 },
    });
    expect(mainStore.orders.find((o) => o.id === '9007199254740995')).toMatchObject({
      symbol: 'BTC-CDE', amount: 0.03, filled: 0, remaining: 0.03,
      extra: { remainingNative: 3, filledInPlacementUnits: 0 },
    });
  }, 20000);

  it.each(names)('%s honors explicit public-only despite downloaded credentials', async (name) => {
    const tape = transport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.native);
    exchange = await createExchange(name, { key: downloaded, secret: '', publicDataOnly: true, tealstreetTunnel: false }) as RustEngineExchange;
    expect(exchange.isPublicDataOnly).toBe(true);
    await exchange.start();
    expect(tape.requests.length).toBeGreaterThan(0);
    expect(tape.requests.every((r) => name === 'coinbasespot'
      ? new URL(r.url).pathname.startsWith('/api/v3/brokerage/market/')
      : new URL(r.url).hostname === 'api.international.coinbase.com')).toBe(true);
    expect(tape.requests.every((r) => r.headers.Authorization === undefined)).toBe(true);
    expect(tape.subscriptions.every((s) => !s.url.includes('-ws-user.') && s.message.jwt === undefined)).toBe(true);
    await expect(exchange.getAccount()).rejects.toThrow(/private portfolio ID is unavailable/);
  }, 10000);

  it('rejects incomplete supplied credentials natively before any network access', async () => {
    const tape = transport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.native);
    exchange = await createExchange('coinbase', { key: keyName, secret: '', tealstreetTunnel: false }) as RustEngineExchange;
    expect(exchange.isPublicDataOnly).toBe(false);
    await expect(exchange.start()).rejects.toThrow(/Coinbase secret must be/);
    expect(tape.requests).toEqual([]);
    expect(tape.subscriptions).toEqual([]);
  });
});
