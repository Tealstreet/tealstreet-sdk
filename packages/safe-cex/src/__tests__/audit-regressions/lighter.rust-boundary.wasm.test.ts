import { vi } from 'vitest';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, OrderFillEvent, RemoteSignerRequest } from '../../types';

import { afterEach, describe, expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { extractLighterAccountTierFromPayload } from '../../exchanges/lighter/lighter.account-tier';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

const NOW = 1700000000000;
const KEY = `01${'00'.repeat(39)}`;
const cleanup: Array<() => void> = [];
afterEach(() => cleanup.splice(0).forEach((dispose) => dispose()));

const recorded = (
  accountLimits: unknown = { code: 200, user_tier_name: 'premium' },
  integrator = 158385,
) => {
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  const sockets: Array<{ subscriptions: string[]; message(text: string): void }> = [];
  let marginMode = 0;
  let marginFraction = 1000;
  const response = (data: unknown) => ({ status: 200, body: new TextEncoder().encode(JSON.stringify(data)) });
  const transport: Transport = {
    now: () => NOW,
    prepareHttp: prepareRecordedHttp,
    connect: (_url, handlers) => {
      const subscriptions: string[] = [];
      sockets.push({ subscriptions, message: handlers.onMessage });
      queueMicrotask(() => handlers.onOpen());
      return { send: (text) => subscriptions.push(text), close() {} };
    },
    request: async (request) => {
      requests.push(request);
      const { pathname } = new URL(request.url);
      if (pathname.endsWith('/orderBookDetails'))
        return response({
          code: 200,
          order_book_details: [
            {
              market_id: 1,
              symbol: 'ETH',
              status: 'active',
              market_type: 'perp',
              supported_size_decimals: 3,
              supported_price_decimals: 2,
              min_base_amount: '0.001',
              min_quote_amount: '1',
              min_initial_margin_fraction: 500,
              maintenance_margin_fraction: 250,
            },
          ],
        });
      if (pathname.endsWith('/exchangeStats'))
        return response({
          code: 200,
          order_book_stats: [
            {
              market_id: 1,
              last_trade_price: '100',
              mark_price: '100',
              index_price: '100',
            },
          ],
        });
      if (pathname.endsWith('/account'))
        return response({
          code: 200,
          accounts: [
            {
              index: 718895,
              collateral: '1000',
              total_asset_value: '1000',
              available_balance: '900',
              approved_integrators: [
                {
                  account_index: integrator,
                  approval_expiry: NOW + 86400000,
                  max_perps_maker_fee: 0,
                  max_perps_taker_fee: 250,
                },
              ],
              positions: [
                {
                  market_id: 1,
                  position: '2',
                  sign: 1,
                  avg_entry_price: '100',
                  unrealized_pnl: '0',
                  margin_mode: marginMode,
                  initial_margin_fraction: marginFraction,
                  liquidation_price: '50',
                },
              ],
            },
          ],
        });
      if (pathname.endsWith('/accountLimits')) return response(accountLimits);
      if (pathname.endsWith('/accountActiveOrders')) return response({ code: 200, orders: [] });
      if (pathname.endsWith('/nextNonce')) return response({ code: 200, nonce: 10 });
      if (pathname.endsWith('/sendTx')) {
        const params = new URLSearchParams(request.body);
        const tx = JSON.parse(params.get('tx_info')!);
        if (params.get('tx_type') === '20') {
          marginMode = tx.MarginMode;
          marginFraction = tx.InitialMarginFraction;
        }
        return response({ code: 200, tx_hash: 'documented-transaction-hash' });
      }
      throw new Error(`Unexpected Lighter boundary request ${request.method} ${request.url}`);
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  return { transport, requests, sockets };
};
const options = (): ExchangeOptions => ({
  key: '',
  secret: KEY,
  apiKeyIndex: '0',
  applicationId: '718895',
  testnet: true,
});
const start = async (opts = options(), name: ExchangeName = 'lighter', accountLimits?: unknown) => {
  const integrator = Number(opts.extra?.lighterIntegratorAccountIndex ?? (name === 'lighter-robinhood' ? 47296 : 158385));
  const tape = recorded(accountLimits, integrator);
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape.transport);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(name, opts, store);
  cleanup.push(() => exchange.dispose());
  await exchange.start();
  return { ...tape, exchange, store };
};
const until = async (check: () => boolean) => {
  for (let i = 0; i < 100; i++) {
    if (check()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error('Lighter boundary condition did not settle');
};

describe.each(['lighter', 'lighter-robinhood'] as const)('%s real WASM boundary', (name) => {
  const variantStart = (opts = options(), accountLimits?: unknown) => start(opts, name, accountLimits);
  it('rejects caller IDs on regular, bracket and later batch orders before any signing write', async () => {
    const { exchange, requests, store } = await variantStart();
    expect(store.markets.map((m) => m.symbol)).toEqual(['ETH']);
    const regular = { symbol: 'ETH', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.2, price: 90 };
    const before = requests.length;
    await expect(exchange.placeOrder({ ...regular, clientOrderId: 'caller' })).rejects.toThrow(/client.*id/i);
    await expect(exchange.placeOrder({ ...regular, stopLoss: 80, clientOrderId: 'caller' })).rejects.toThrow(
      /client.*id/i,
    );
    await expect(exchange.placeOrders([regular, { ...regular, clientOrderId: 'caller' }])).rejects.toThrow(
      /client.*id/i,
    );
    expect(exchange.mintClientOrderId(regular)).toBeUndefined();
    expect(requests.slice(before).some((r) => r.url.includes('/nextNonce') || r.method !== 'GET')).toBe(false);
  });

  it.each([{ secret: '' }, { apiKeyIndex: undefined }, { publicDataOnly: true }])(
    'starts public data without private account access for %o',
    async (override) => {
      const { requests, store, sockets } = await variantStart({ ...options(), ...override });
      expect(
        store.loaded.markets &&
          store.loaded.tickers &&
          store.loaded.positions &&
          store.loaded.orders &&
          store.loaded.balance,
      ).toBe(true);
      expect(requests.every((r) => !new URL(r.url).pathname.includes('/account'))).toBe(true);
      expect(sockets).toHaveLength(1);
    },
  );

  it('applies exactly one execution to worker and main frames including the initial five seconds', async () => {
    const { exchange, sockets } = await variantStart();
    const main = new BaseExchange(name, { key: '', secret: '' }, new DefaultStore());
    cleanup.push(() => main.dispose());
    main.applyWorkerFrame(exchange.snapshotFrame()!);
    exchange.onFrame = (frame) => main.applyWorkerFrame(frame.slice(0));
    const workerFills: OrderFillEvent[] = [];
    const mainFills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => workerFills.push(fill));
    main.on('fill', (fill: OrderFillEvent) => mainFills.push(fill));
    const socket = sockets.find((s) => s.subscriptions.some((text) => text.includes('account_all/718895')))!;
    expect(socket).toBeDefined();
    const frame = JSON.stringify({
      type: 'update/account_all',
      channel: 'account_all:718895',
      orders: {
        1: [
          {
            order_index: '42',
            market_id: 1,
            status: 'filled',
            initial_base_amount: '0.2',
            filled_base_amount: '0.2',
            remaining_base_amount: '0',
            price: '90',
            updated_at: NOW,
          },
        ],
      },
      trades: { 1: [{ trade_id: '99', order_id: '42', side: 'buy', price: '90', size: '0.05', timestamp: NOW }] },
    });
    socket.message(frame);
    socket.message(frame);
    await until(() => workerFills.length > 0 && mainFills.length > 0);
    expect(workerFills).toEqual([{ id: '42', orderId: '42', symbol: 'ETH', side: 'buy', price: 90, amount: 0.05 }]);
    expect(mainFills).toEqual(workerFills);
    socket.message(frame);
    await new Promise((resolve) => setTimeout(resolve, 150));
    expect(workerFills).toHaveLength(1);
    expect(mainFills).toHaveLength(1);
  });

  it('preserves signer indices, conditional slippage and account settings through the real wasm host', async () => {
    const { exchange, requests, store } = await variantStart({ ...options(), extra: { lighterStopSlippagePct: 3 } });
    const ids = await exchange.placeOrder({
      symbol: 'ETH',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.2,
      price: 80,
      triggerPrice: 80,
      reduceOnly: true,
    });
    expect(ids).toHaveLength(1);
    const writes = () =>
      requests
        .filter((r) => r.method === 'POST')
        .map((r) => ({
          type: new URLSearchParams(r.body).get('tx_type'),
          tx: JSON.parse(new URLSearchParams(r.body).get('tx_info')!),
        }));
    const stop = writes()[0];
    expect(stop.type).toBe('14');
    expect(stop.tx).toMatchObject({
      AccountIndex: 718895,
      ApiKeyIndex: 0,
      MarketIndex: 1,
      Type: 2,
      IsAsk: 1,
      BaseAmount: 200,
      Price: 7760,
      TriggerPrice: 8000,
      TimeInForce: 0,
      ReduceOnly: 1,
    });
    if (name === 'lighter-robinhood') {
      expect(stop.tx.L2TxAttributes).toEqual({ 1: 47296, 2: 250, 3: 0 });
    } else {
      expect(stop.tx).not.toHaveProperty('L2TxAttributes');
    }
    expect(stop.tx.ClientOrderIndex.toString()).toBe(ids[0]);
    expect(atob(stop.tx.Sig)).toHaveLength(80);
    expect(stop.tx.ExpiredAt).toBeGreaterThan(NOW);
    await exchange.setLeverage('ETH', 20);
    await exchange.changeMarginType(true, 'ETH');
    await until(
      () =>
        store.fetchedLeverage.ETH?.buyLeverage === 20 && store.fetchedLeverage.ETH.marginType === MarginType.Isolated,
    );
    const settings = writes().filter((w) => w.type === '20');
    expect(settings.map((w) => [w.tx.InitialMarginFraction, w.tx.MarginMode])).toEqual([
      [500, 0],
      [500, 1],
    ]);
    expect(new Set(writes().map((w) => w.tx.Sig)).size).toBe(3);
  });

  it.each([
    { code: 200, user_tier_name: 'standard' },
    { code: 200, user_tier_name: ['standard'] },
    { code: 200, data: { userTierName: [' STD '] } },
    { code: 200, data: { account: { accountType: ['standard'] } } },
  ])('blocks attributed placements for TS-recognized Standard payload %j', async (accountLimits) => {
    expect(extractLighterAccountTierFromPayload(accountLimits)).toBe('standard');
    const { exchange, requests } = await variantStart(
      { ...options(), extra: { lighterIntegratorAccountIndex: '158385' } },
      accountLimits,
    );
    const before = requests.length;
    await expect(
      exchange.placeOrder({ symbol: 'ETH', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.2, price: 90 }),
    ).rejects.toThrow('Plus or Premium');
    expect(requests.slice(before).some((r) => new URL(r.url).pathname.endsWith('/accountLimits'))).toBe(true);
    expect(requests.slice(before).some((r) => r.method !== 'GET' || /nextNonce|account\?/.test(r.url))).toBe(false);
  });

  it('a remote-signed account signs Lighter statements through its signer and sends nothing it refuses', async () => {
    // The keyed browser answers auth statements and refuses the transaction.
    const keyed = new RustEngineExchange(name, options(), new DefaultStore());
    cleanup.push(() => keyed.dispose());
    const asked: RemoteSignerRequest[] = [];
    const { exchange, requests, store } = await variantStart({
      ...options(),
      key: 'lighter-public-key',
      secret: '',
      privateOnly: true,
      remoteSigner: {
        sign: async (request) => {
          asked.push(request);
          if (JSON.parse(request.wsAuth!.toSign).kind === 'auth') return keyed.signWsAuth(request);
          throw new Error('remote signer refused');
        },
        isAvailable: () => true,
        onAvailabilityChange: () => () => {},
      },
    });
    expect(store.loaded.orders && store.loaded.positions).toBe(true);
    await expect(
      exchange.placeOrder({ symbol: 'ETH', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.2, price: 90 }),
    ).rejects.toThrow();
    expect(requests.some((r) => new URL(r.url).pathname.endsWith('/sendTx'))).toBe(false);
    const kinds = asked.map((request) => {
      expect(request).toMatchObject({ type: 'ws-auth', exchangeName: name, wsAuth: { outputFormat: 'lighter-tx' } });
      return JSON.parse(request.wsAuth!.toSign).kind;
    });
    expect(kinds).toContain('auth');
    expect(kinds).toContain('txs');
    expect(keyed.hasStarted).toBe(false);
  });
});
