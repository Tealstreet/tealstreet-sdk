import type { ExchangeOptions, Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import type { ORDER_EVENT } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';

const makeMarket = (overrides: Partial<Market> = {}): Market => ({
  id: 'BTC-28MAR27',
  symbol: 'BTC-28MAR27',
  base: 'BTC',
  quote: 'USD',
  active: true,
  isExpiring: true,
  precision: { amount: 1, price: 0.1 },
  limits: {
    amount: { min: 1, max: 1_000_000 },
    leverage: { min: 1, max: 10 },
  },
  contractSize: 0.001,
  ...overrides,
});

const makeRawCfmMarket = (product_id: string, contract_size: unknown) =>
  ({
    product_id,
    price_increment: '0.1',
    base_increment: '1',
    base_min_size: '1',
    base_max_size: '1000000',
    base_currency_id: 'BTC',
    quote_currency_id: 'USD',
    status: 'TRADING',
    future_product_details: {
      contract_expiry_type: 'EXPIRING',
      contract_root_unit: 'BTC',
      contract_size,
    },
  }) as any;

const makeOrder = (overrides: Partial<ORDER_EVENT['orders'][0]> = {}): ORDER_EVENT['orders'][0] => ({
  order_id: 'coinbase-order-id',
  client_order_id: 'coinbase-client-order-id',
  cumulative_quantity: '1.2',
  leaves_quantity: '0.3',
  avg_price: '100000',
  limit_price: '100000',
  stop_price: '',
  total_fees: '0',
  status: 'OPEN',
  product_id: 'BTC-28MAR27',
  creation_time: '2026-07-14T00:00:00Z',
  order_side: 'BUY',
  order_type: 'Limit',
  cancel_reason: '',
  reject_reason: '',
  ...overrides,
});

const makeOrderEvent = (orders: ORDER_EVENT['orders'] = [makeOrder()], type: ORDER_EVENT['type'] = 'snapshot'): ORDER_EVENT => ({
  type,
  orders,
});

const makePosition = (product_id: string, number_of_contracts = '1.5') =>
  ({
    product_id,
    side: 'LONG',
    number_of_contracts,
    current_price: '100000',
    avg_entry_price: '99000',
    unrealized_pnl: '10',
  }) as any;

const createExchange = (markets: Market[] = [makeMarket()]) => {
  const store = new DefaultStore();
  store.update({ markets });
  const exchange = new CoinbaseExchange(
    'coinbase' as any,
    {
      key: 'test-key',
      secret: 'test-secret',
      tealstreetTunnel: false,
    } as ExchangeOptions,
    store,
  );

  return { exchange, store };
};

describe('Coinbase CFM contract-size quarantine', () => {
  it('keeps only valid expiring markets, including contractSize=1, when raw CFM metadata is absent or invalid', async () => {
    const { exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const get = vi.fn().mockResolvedValue({
      data: {
        products: [
          makeRawCfmMarket('BTC-VALID-ONE', '1'),
          makeRawCfmMarket('ETH-VALID-NON-ONE', '0.01'),
          makeRawCfmMarket('SOL-ABSENT', undefined),
          makeRawCfmMarket('DOGE-ZERO', '0'),
          makeRawCfmMarket('XRP-NEGATIVE', '-1'),
          makeRawCfmMarket('ADA-NAN', 'NaN'),
        ],
      },
    });
    exchange.xhr = { get } as any;
    vi.spyOn(exchange, 'getSupportedTypes').mockResolvedValue(['cfm']);

    const markets = await exchange.fetchMarkets();

    expect(markets.map((market) => [market.id, market.contractSize])).toEqual([
      ['BTC-VALID-ONE', 1],
      ['ETH-VALID-NON-ONE', 0.01],
    ]);
    expect(emitAndLogError).toHaveBeenCalledTimes(4);
    expect(emitAndLogError.mock.calls.every(([message]) => String(message).includes('CFM market skipped'))).toBe(true);
  });

  it('maps valid CFM order and position quantities to the same base unit for contract sizes 1 and non-1', async () => {
    for (const contractSize of [1, 0.001]) {
      const productId = `BTC-${contractSize}`;
      const { exchange, store } = createExchange([makeMarket({ id: productId, symbol: productId, contractSize })]);
      exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent([makeOrder({ product_id: productId })]));
      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { positions: [makePosition(productId)] } }),
      } as any;

      const positions = await exchange.fetchOpenPositionsCfm();

      expect(store.orders).toHaveLength(1);
      expect(store.orders[0].amount).toBeCloseTo(1.5 * contractSize, 12);
      expect(store.orders[0].filled).toBeCloseTo(1.2 * contractSize, 12);
      expect(store.orders[0].remaining).toBeCloseTo(0.3 * contractSize, 12);
      expect(positions).toHaveLength(1);
      expect(positions[0].contracts).toBeCloseTo(store.orders[0].amount, 12);
    }
  });

  it('keeps valid siblings and continues processing after missing or invalid CFM order metadata', () => {
    const valid = makeMarket({ id: 'BTC-VALID', symbol: 'BTC-VALID', contractSize: 0.01 });
    const { exchange, store } = createExchange([
      valid,
      makeMarket({ id: 'SOL-ABSENT', contractSize: undefined }),
      makeMarket({ id: 'DOGE-ZERO', contractSize: 0 }),
      makeMarket({ id: 'XRP-NEGATIVE', contractSize: -1 }),
      makeMarket({ id: 'ADA-NAN', contractSize: Number.NaN }),
    ]);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([
        makeOrder({ order_id: 'missing-market', product_id: 'UNKNOWN' }),
        makeOrder({ order_id: 'absent-size', product_id: 'SOL-ABSENT' }),
        makeOrder({ order_id: 'zero-size', product_id: 'DOGE-ZERO' }),
        makeOrder({ order_id: 'negative-size', product_id: 'XRP-NEGATIVE' }),
        makeOrder({ order_id: 'nan-size', product_id: 'ADA-NAN' }),
        makeOrder({ order_id: 'valid-sibling', product_id: 'BTC-VALID' }),
      ]),
    );
    exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'later-valid', product_id: 'BTC-VALID' })], 'update'),
    );

    expect(store.orders.map((order) => order.id)).toEqual(['valid-sibling', 'later-valid']);
    expect(store.orders.every((order) => order.amount === 0.015)).toBe(true);
    expect(emitAndLogError).toHaveBeenCalledTimes(5);
  });

  it('treats empty, invalid-only, and mixed private order snapshots as distinct authoritative states', () => {
    const valid = makeMarket({ id: 'BTC-VALID', symbol: 'BTC-VALID', contractSize: 0.01 });

    const empty = createExchange([valid]);
    empty.exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent([makeOrder({ order_id: 'old-empty', product_id: 'BTC-VALID' })], 'update'));
    empty.exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent([]));
    expect(empty.store.orders).toEqual([]);
    expect(empty.store.loaded.orders).toBe(true);

    const invalidOnly = createExchange([valid]);
    const invalidOnlyError = vi.spyOn(invalidOnly.exchange, 'emitAndLogError').mockImplementation(() => undefined);
    invalidOnly.exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'old-invalid-only', product_id: 'BTC-VALID' })], 'update'),
    );
    invalidOnly.exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'quarantined', product_id: 'UNKNOWN' })]),
    );
    expect(invalidOnly.store.orders.map((order) => order.id)).toEqual(['old-invalid-only']);
    expect(invalidOnly.store.loaded.orders).toBe(false);
    expect(invalidOnlyError.mock.calls.some(([message]) => message === 'Coinbase order skipped: missing market for UNKNOWN')).toBe(
      true,
    );

    const mixed = createExchange([valid]);
    const mixedError = vi.spyOn(mixed.exchange, 'emitAndLogError').mockImplementation(() => undefined);
    mixed.exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'old-mixed', product_id: 'BTC-VALID' })], 'update'),
    );
    mixed.exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([
        makeOrder({ order_id: 'quarantined-mixed', product_id: 'UNKNOWN' }),
        makeOrder({ order_id: 'valid-mixed', product_id: 'BTC-VALID' }),
      ]),
    );
    expect(mixed.store.orders.map((order) => order.id)).toEqual(['valid-mixed']);
    expect(mixed.store.loaded.orders).toBe(true);
    expect(mixedError.mock.calls.some(([message]) => message === 'Coinbase order skipped: missing market for UNKNOWN')).toBe(true);

    const invalidBatched = createExchange([valid]);
    vi.spyOn(invalidBatched.exchange, 'emitAndLogError').mockImplementation(() => undefined);
    invalidBatched.exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'old-invalid-batched', product_id: 'BTC-VALID' })], 'update'),
    );
    invalidBatched.exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent(
        Array.from({ length: 50 }, (_, index) =>
          makeOrder({ order_id: `quarantined-batched-${index}`, product_id: 'UNKNOWN' }),
        ),
      ),
    );
    invalidBatched.exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent([]));
    expect(invalidBatched.store.orders.map((order) => order.id)).toEqual(['old-invalid-batched']);
    expect(invalidBatched.store.loaded.orders).toBe(false);
  });

  it('assembles Coinbase private order snapshot batches before replacing store orders', () => {
    const valid = makeMarket({ id: 'BTC-VALID', symbol: 'BTC-VALID', contractSize: 0.01 });
    const { exchange, store } = createExchange([valid]);
    exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'old-order', product_id: 'BTC-VALID' })], 'update'),
    );

    const firstBatch = Array.from({ length: 50 }, (_, index) =>
      makeOrder({ order_id: `snapshot-${index}`, product_id: 'BTC-VALID' }),
    );
    const finalBatch = [
      makeOrder({ order_id: 'snapshot-50', product_id: 'BTC-VALID' }),
      makeOrder({ order_id: 'snapshot-51', product_id: 'BTC-VALID' }),
    ];

    exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent(firstBatch));
    expect(store.orders.map((order) => order.id)).toEqual(['old-order']);
    expect(store.loaded.orders).toBe(false);

    exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent(finalBatch));

    expect(store.orders).toHaveLength(52);
    expect(store.orders[0].id).toBe('snapshot-0');
    expect(store.orders.at(-1)?.id).toBe('snapshot-51');
    expect(store.loaded.orders).toBe(true);
  });

  it('skips only unresolved or invalid CFM positions instead of publishing guessed or mixed units', async () => {
    const valid = makeMarket({ id: 'BTC-VALID', symbol: 'BTC-VALID', contractSize: 0.01 });
    const { exchange } = createExchange([
      valid,
      makeMarket({ id: 'SOL-ABSENT', contractSize: undefined }),
      makeMarket({ id: 'DOGE-ZERO', contractSize: 0 }),
      makeMarket({ id: 'XRP-NEGATIVE', contractSize: -1 }),
      makeMarket({ id: 'ADA-NAN', contractSize: Number.NaN }),
    ]);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          positions: [
            makePosition('UNKNOWN'),
            makePosition('SOL-ABSENT'),
            makePosition('DOGE-ZERO'),
            makePosition('XRP-NEGATIVE'),
            makePosition('ADA-NAN'),
            makePosition('BTC-VALID'),
          ],
        },
      }),
    } as any;

    const positions = await exchange.fetchOpenPositionsCfm();

    expect(positions).toHaveLength(1);
    expect(positions[0]).toMatchObject({ symbol: 'BTC-VALID', contracts: 0.015, notional: 1500 });
    expect(emitAndLogError).toHaveBeenCalledTimes(5);
  });

  it('does not rescale INTX or spot quantities, even when their market advertises a contractSize', () => {
    const { exchange, store } = createExchange([
      makeMarket({ id: 'BTC-PERP-INTX', symbol: 'BTC-PERP-INTX', isExpiring: false, contractSize: 100 }),
      makeMarket({ id: 'BTC-USD', symbol: 'BTC-USD', isExpiring: false, isSpot: true, contractSize: 100 }),
    ]);

    exchange.privateWebsocket.handleUserEventOrders(makeOrderEvent([makeOrder({ product_id: 'BTC-PERP-INTX' })]));
    exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'spot-order-id', product_id: 'BTC-USD' })], 'update'),
    );

    expect(store.orders).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ symbol: 'BTC-PERP-INTX', amount: 1.5, filled: 1.2, remaining: 0.3 }),
        expect.objectContaining({ symbol: 'BTC-USD', amount: 1.5, filled: 1.2, remaining: 0.3 }),
      ]),
    );
  });

  it('maps USDC-equivalent spot order events to loaded USD markets', () => {
    const { exchange, store } = createExchange([
      makeMarket({ id: 'BTC-USD', symbol: 'BTC-USD', isExpiring: false, isSpot: true, contractSize: 1 }),
    ]);

    exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'spot-usdc-order', product_id: 'BTC-USDC' })]),
    );

    expect(store.orders).toEqual([
      expect.objectContaining({
        id: 'spot-usdc-order',
        symbol: 'BTC-USD',
        amount: 1.5,
        filled: 1.2,
        remaining: 0.3,
      }),
    ]);
    expect(store.loaded.orders).toBe(true);
  });

  it('prefers loaded USDC markets when Coinbase reports equivalent USD private orders', () => {
    const { exchange, store } = createExchange([
      makeMarket({ id: 'BTC-USD', symbol: 'BTC-USD', isExpiring: false, isSpot: true, contractSize: 1 }),
      makeMarket({ id: 'BTC-USDC', symbol: 'BTC-USDC', isExpiring: false, isSpot: true, contractSize: 1 }),
    ]);

    exchange.privateWebsocket.handleUserEventOrders(
      makeOrderEvent([makeOrder({ order_id: 'spot-usd-order', product_id: 'BTC-USD' })]),
    );

    expect(store.orders).toEqual([
      expect.objectContaining({
        id: 'spot-usd-order',
        symbol: 'BTC-USDC',
      }),
    ]);
  });
});
