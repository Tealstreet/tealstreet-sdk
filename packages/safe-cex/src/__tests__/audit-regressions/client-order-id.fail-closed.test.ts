import type { Market, PlaceOrderOpts } from '../../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from '../reference/apexomni/apexomni.exchange';
import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { BaseExchange } from '../../exchanges/base';
import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';
import { BinanceOptionsExchange } from '../reference/binance/binanceoptions.exchange';
import { BinanceSpotExchange } from '../reference/binance/binancespot.exchange';
import { BingxExchange } from '../reference/bingx/bingx.exchange';
import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { BlofinExchange } from '../reference/blofin/blofin.exchange';
import { BreakoutpropExchange } from '../reference/breakoutprop/breakoutprop.exchange';
import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../reference/bybit/bybitspot.exchange';
import { HyrotraderExchange } from '../reference/bybit/hyrotrader.exchange';
import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { CryptocomExchange } from '../reference/cryptocom/cryptocom.exchange';
import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { TxFlowExchange } from '../reference/txflow/txflow.exchange';
import { HyperliquidSpotExchange } from '../reference/hyperliquid/hyperliquidspot.exchange';
import { KrakenExchange } from '../reference/kraken/kraken.exchange';
import { KrakenSpotExchange } from '../reference/kraken/krakenspot.exchange';
import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
import { KucoinSpotExchange } from '../reference/kucoin/kucoinspot.exchange';
import { LighterExchange } from '../reference/lighter/lighter.exchange';
import { MexcExchange } from '../reference/mexc/mexc.exchange';
import { OKXExchange } from '../reference/okx/okx.exchange';
import { RustEngineExchange as OrderlyExchange } from '../../exchanges/rust/rust.exchange';
import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { PolymarketExchange } from '../reference/polymarket/polymarket.exchange';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { WeexExchange } from '../reference/weex/weex.exchange';
import { WOOXExchange } from '../reference/woo/woo.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

// Contract: a caller clientOrderId is sent verbatim on one native order, or the
// call rejects — emitting first — before ANY transport is touched.

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  info: {},
};

type ExchangeCtor = new (name: any, options: any, store: DefaultStore) => BaseExchange;

const HONORING: Array<[string, ExchangeCtor]> = [
  ['txflow', TxFlowExchange],
  ['txflow', RustEngineExchange],
  ['apexomni', ApexOmniExchange],
  ['backpack', BackpackExchange],
  ['binance', BinanceExchange],
  ['binanceoptions', BinanceOptionsExchange],
  ['binancepm', BinancePMExchange],
  ['binancespot', BinanceSpotExchange],
  ['bingx', BingxExchange],
  ['bitget', BitgetExchange],
  ['bitget', RustEngineExchange],
  ['bitgetusdc', RustEngineExchange],
  ['bitrue', BitrueExchange],
  ['bitunix', BitunixExchange],
  ['blofin', BlofinExchange],
  ['bybit', BybitExchange],
  ['bybitinverse', BybitInverseExchange],
  ['bybitspot', BybitSpotExchange],
  ['coinbase', CoinbaseExchange],
  ['cryptocom', CryptocomExchange],
  ['cryptocom', RustEngineExchange],
  ['grvt', GrvtExchange],
  ['kucoin', KucoinExchange],
  ['kucoinspot', KucoinSpotExchange],
  ['okx', OKXExchange],
  ['salsadex', OrderlyExchange],
  ['paradex', ParadexExchange],
  ['phemex', PhemexExchange],
  ['weex', WeexExchange],
  ['weex', RustEngineExchange],
  ['woo', WOOXExchange],
];

// Honor a caller ID on EVERY shape, not only plain Limit/Market: a stop is one
// native order, and on a bracket the ID goes on the entry while each TP/SL leg
// gets its own minted ID. So the shape-refusal cases above do not apply.
const HONORING_EVERY_SHAPE: Array<[string, ExchangeCtor]> = [
  ['hyperliquid', HyperliquidExchange],
  ['hyperliquid', RustEngineExchange],
  ['hyperliquidspot', HyperliquidSpotExchange],
];

const HYPERLIQUID_CLOID = '0x0123456789abcdef0123456789abcdef';

const NOT_HONORING: Array<[string, ExchangeCtor]> = [
  ['asterdex', AsterDEXExchange],
  ['breakoutprop', BreakoutpropExchange],
  // hyperliquid and hyperliquidspot moved to HONORING_EVERY_SHAPE when every
  // placement started carrying a cloid (feat(hyperliquid): cloid on every order).
  ['hyrotrader', HyrotraderExchange],
  ['kraken', KrakenExchange],
  ['krakenspot', KrakenSpotExchange],
  ['lighter', LighterExchange],
  ['lighter', RustEngineExchange],
  ['mexc', MexcExchange],
  ['polymarket', PolymarketExchange],
];

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
  ...overrides,
});

const isTransport = (value: unknown): value is Record<string, unknown> =>
  !!value &&
  typeof value === 'object' &&
  ['get', 'post', 'put', 'delete'].some((m) => typeof (value as any)[m] === 'function');

// Replace every axios-like client on the instance with recorders, so any write is observable.
const setup = (name: string, Ctor: ExchangeCtor) => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new Ctor(
    name,
    { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false },
    store,
  );
  // A Rust venue answers from its engine's record, as it would once started.
  if (exchange instanceof RustEngineExchange) adoptEngineCapabilities(exchange);
  const transportCalls = vi.fn();
  for (const key of Object.keys(exchange)) {
    if (!isTransport((exchange as any)[key])) continue;
    const recorder = (..._args: unknown[]) => {
      transportCalls(key);
      return Promise.reject(new Error('transport must not be reached'));
    };
    (exchange as any)[key] = { get: recorder, post: recorder, put: recorder, delete: recorder, request: recorder };
  }
  const emitted: string[] = [];
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation((err) => {
    emitted.push(String(err));
  });
  return { exchange, transportCalls, emitted };
};

describe('clientOrderId fails closed before any write', () => {
  it.each(['bitget', 'bitgetusdc'])('%s Rust wrapper honors caller IDs and finds their live orders', async (name) => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new RustEngineExchange(
      name as any,
      { key: 'key', secret: 'secret', passphrase: 'passphrase' },
      store,
    );
    adoptEngineCapabilities(exchange);
    const opts = order({ clientOrderId: 'caller.id:/A_-' });
    const place = vi.spyOn(exchange, 'placeOrders').mockResolvedValue(['order-1']);
    await expect(
      exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: opts.clientOrderId! }),
    ).resolves.toEqual({
      orderIds: ['order-1'],
      clientOrderId: opts.clientOrderId,
    });
    expect(place).toHaveBeenCalledWith([opts]);
    expect(exchange.mintClientOrderId(order())).toMatch(/^[.A-Z:/a-z0-9_-]{1,32}$/);
    const live = {
      ...opts,
      id: 'order-1',
      price: 50_000,
      status: OrderStatus.Open,
      filled: 0,
      remaining: 1,
      reduceOnly: false,
      lastUpdate: 1,
    };
    exchange.store.update({ orders: [live] });
    await expect(exchange.findOrderByClientOrderId(opts.symbol, opts.clientOrderId!)).resolves.toEqual({
      found: true,
      order: live,
    });
    exchange.dispose();
  });

  beforeEach(() => {
    // HyroTrader's REST origin is fixed at build time; any origin lets it construct.
    vi.spyOn(HyrotraderExchange.prototype, 'getBybitRestBaseUrl').mockReturnValue('https://simulator.test');
  });

  it.each(HONORING)('%s rejects a caller ID on a TP/SL bracket shape before any transport', async (name, Ctor) => {
    const { exchange, transportCalls, emitted } = setup(name, Ctor);

    await expect(exchange.placeOrder(order({ clientOrderId: '12345', stopLoss: 49_000 }))).rejects.toThrow(
      /clientOrderId cannot be honored for this order shape/,
    );
    expect(emitted).toEqual([expect.stringMatching(/clientOrderId cannot be honored/)]);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING)('%s rejects a caller ID on a standalone stop before any transport', async (name, Ctor) => {
    const { exchange, transportCalls, emitted } = setup(name, Ctor);

    await expect(
      exchange.placeOrder(order({ type: OrderType.StopLoss, side: OrderSide.Sell, clientOrderId: '12345' })),
    ).rejects.toThrow(/clientOrderId cannot be honored/);
    expect(emitted).toHaveLength(1);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING)('%s rejects a caller ID that breaks the venue charset before any transport', async (name, Ctor) => {
    const { exchange, transportCalls, emitted } = setup(name, Ctor);

    await expect(exchange.placeOrder(order({ clientOrderId: `${'x'.repeat(80)} y` }))).rejects.toThrow(
      /clientOrderId must /,
    );
    expect(emitted).toHaveLength(1);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING)('%s rejects a caller ID on an order above the max size for one order', async (name, Ctor) => {
    const { exchange, transportCalls } = setup(name, Ctor);
    const split = /exceeds the market's max size for one order; a caller clientOrderId cannot span several orders/;

    await expect(exchange.placeOrder(order({ amount: 25, clientOrderId: '12345' }))).rejects.toThrow(split);
    await expect(exchange.placeOrders([order(), order({ amount: 25, clientOrderId: '12345' })])).rejects.toThrow(split);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING)('%s rejects a whole batch when one caller ID cannot be honored', async (name, Ctor) => {
    const { exchange, transportCalls } = setup(name, Ctor);

    await expect(
      exchange.placeOrders([
        order(),
        order({ clientOrderId: '12345' }),
        order({ type: OrderType.TakeProfit, side: OrderSide.Sell, clientOrderId: '67890' }),
      ]),
    ).rejects.toThrow(/clientOrderId cannot be honored/);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING)('%s rejects a batch that repeats one caller ID', async (name, Ctor) => {
    const { exchange, transportCalls } = setup(name, Ctor);

    await expect(
      exchange.placeOrders([order({ clientOrderId: '12345' }), order({ clientOrderId: '12345' })]),
    ).rejects.toThrow(/duplicated in one batch/);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(NOT_HONORING)('%s rejects any caller ID, single or batch, before any transport', async (name, Ctor) => {
    const { exchange, transportCalls, emitted } = setup(name, Ctor);

    await expect(exchange.placeOrder(order({ clientOrderId: '12345' }))).rejects.toThrow(
      /cannot honor a caller clientOrderId/,
    );
    await expect(exchange.placeOrders([order(), order({ clientOrderId: '12345' })])).rejects.toThrow(
      /cannot honor a caller clientOrderId/,
    );
    await expect(exchange.placeOrderWithClientOrderId(order({ clientOrderId: '12345' }) as any)).rejects.toThrow(
      /cannot honor a caller clientOrderId/,
    );
    expect(emitted).toHaveLength(3);
    expect(transportCalls).not.toHaveBeenCalled();
  });
});

describe('clientOrderId on venues that honor every shape', () => {
  it.each(HONORING_EVERY_SHAPE)(
    '%s rejects a caller ID that breaks the venue charset before any transport',
    async (name, Ctor) => {
      const { exchange, transportCalls, emitted } = setup(name, Ctor);

      await expect(exchange.placeOrder(order({ clientOrderId: '12345' }))).rejects.toThrow(/clientOrderId must /);
      await expect(exchange.placeOrder(order({ clientOrderId: '0x0123456789ABCDEF0123456789ABCDEF' }))).rejects.toThrow(
        /clientOrderId must /,
      );
      expect(emitted).toHaveLength(2);
      expect(transportCalls).not.toHaveBeenCalled();
    },
  );

  it.each(HONORING_EVERY_SHAPE)(
    '%s rejects a caller ID on an order above the max size for one order',
    async (name, Ctor) => {
      const { exchange, transportCalls } = setup(name, Ctor);

      await expect(exchange.placeOrder(order({ amount: 25, clientOrderId: HYPERLIQUID_CLOID }))).rejects.toThrow(
        /exceeds the market's max size for one order/,
      );
      expect(transportCalls).not.toHaveBeenCalled();
    },
  );

  it.each(HONORING_EVERY_SHAPE)('%s rejects a whole batch with one bad or repeated caller ID', async (name, Ctor) => {
    const { exchange, transportCalls } = setup(name, Ctor);

    await expect(exchange.placeOrders([order(), order({ clientOrderId: 'nope' })])).rejects.toThrow(
      /clientOrderId must /,
    );
    await expect(
      exchange.placeOrders([order({ clientOrderId: HYPERLIQUID_CLOID }), order({ clientOrderId: HYPERLIQUID_CLOID })]),
    ).rejects.toThrow(/duplicated in one batch/);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING_EVERY_SHAPE)('%s refuses to place an order when the caller ID went missing', async (name, Ctor) => {
    const { exchange, transportCalls } = setup(name, Ctor);

    await expect(exchange.placeOrderWithClientOrderId(order() as any)).rejects.toThrow(/requires a clientOrderId/);
    expect(transportCalls).not.toHaveBeenCalled();
  });

  it.each(HONORING_EVERY_SHAPE)(
    '%s mints a cloid for every shape it places, and none it would refuse',
    (name, Ctor) => {
      const { exchange, emitted } = setup(name, Ctor);
      const cloid = /^0x[0-9a-f]{32}$/;

      expect(exchange.mintClientOrderId(order())).toMatch(cloid);
      expect(exchange.mintClientOrderId(order({ stopLoss: 49_000, takeProfit: 51_000 }))).toMatch(cloid);
      expect(exchange.mintClientOrderId(order({ type: OrderType.StopLoss, triggerPrice: 49_000 }))).toMatch(cloid);
      expect(exchange.mintClientOrderId(order({ type: OrderType.TrailingStopLoss }))).toBeUndefined();
      expect(exchange.mintClientOrderId(order({ amount: 11 }))).toBeUndefined();
      expect(exchange.mintClientOrderId(order({ symbol: 'UNKNOWN' }))).toBeUndefined();
      expect(emitted).toEqual([]);
    },
  );
});

describe('placeOrderWithClientOrderId', () => {
  it.each(HONORING)('%s refuses to place an order when the caller ID went missing', async (name, Ctor) => {
    const { exchange, transportCalls, emitted } = setup(name, Ctor);

    await expect(exchange.placeOrderWithClientOrderId(order() as any)).rejects.toThrow(/requires a clientOrderId/);
    expect(emitted).toHaveLength(1);
    expect(transportCalls).not.toHaveBeenCalled();
  });
});

describe('BaseExchange client order ID helpers', () => {
  it('rejectPreflight emits before it throws', () => {
    const exchange = new BaseExchange('binance', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    const events: string[] = [];
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {
      events.push('emit');
    });

    expect(() => {
      try {
        (exchange as any).rejectPreflight('cannot honor');
      } finally {
        events.push('threw');
      }
    }).toThrow('cannot honor');
    expect(events).toEqual(['emit', 'threw']);
  });
});

// A minted ID is one the venue will honor, in the venue's own format: a lost
// response can be looked up by it, and broker-prefixed venues keep their prefix.
describe('mintClientOrderId', () => {
  beforeEach(() => {
    vi.spyOn(HyrotraderExchange.prototype, 'getBybitRestBaseUrl').mockReturnValue('https://simulator.test');
  });

  it.each(HONORING)('%s mints an ID its own contract accepts, without emitting', (name, Ctor) => {
    const { exchange, emitted } = setup(name, Ctor);
    const clientOrderId = exchange.mintClientOrderId(order());

    expect(clientOrderId).toEqual(expect.any(String));
    expect((exchange as any).resolveCallerClientOrderId(order({ clientOrderId }))).toBe(clientOrderId);
    expect(emitted).toEqual([]);
  });

  it.each([
    ['binance', BinanceExchange, 'x-s3vFFRQp'],
    ['binancepm', BinancePMExchange, 'x-s3vFFRQp'],
    ['binancespot', BinanceSpotExchange, 'x-s3vFFRQp'],
    ['okx', OKXExchange, 'e847386590ce4dBC'],
    ['blofin', BlofinExchange, 'b3cdedc0a20880ba'],
    ['phemex', PhemexExchange, 'Tealstreet_'],
  ] as Array<[string, ExchangeCtor, string]>)('%s keeps its broker prefix', (name, Ctor, prefix) => {
    expect(setup(name, Ctor).exchange.mintClientOrderId(order())).toMatch(new RegExp(`^${prefix}`));
  });

  it.each(NOT_HONORING)('%s mints nothing', (name, Ctor) => {
    const { exchange, emitted } = setup(name, Ctor);
    expect(exchange.mintClientOrderId(order())).toBeUndefined();
    expect(emitted).toEqual([]);
  });

  it.each(HONORING)('%s mints nothing for a shape or size it would refuse', (name, Ctor) => {
    const { exchange, emitted } = setup(name, Ctor);
    expect(exchange.mintClientOrderId(order({ stopLoss: 49_000 }))).toBeUndefined();
    expect(exchange.mintClientOrderId(order({ type: OrderType.StopLoss, triggerPrice: 49_000 }))).toBeUndefined();
    expect(exchange.mintClientOrderId(order({ amount: 11 }))).toBeUndefined();
    expect(exchange.mintClientOrderId(order({ symbol: 'UNKNOWN' }))).toBeUndefined();
    expect(emitted).toEqual([]);
  });
});

describe('findOrderByClientOrderId on a venue that refuses caller IDs', () => {
  beforeEach(() => {
    vi.spyOn(HyrotraderExchange.prototype, 'getBybitRestBaseUrl').mockReturnValue('https://simulator.test');
  });

  // It never sent one, and some venues write the store while fetching orders.
  it.each(NOT_HONORING)('%s misses without touching a transport', async (name, Ctor) => {
    const { exchange, transportCalls } = setup(name, Ctor);
    await expect(exchange.findOrderByClientOrderId(MARKET.symbol, 'cid-1')).resolves.toEqual({
      found: false,
      definitive: false,
    });
    expect(transportCalls).not.toHaveBeenCalled();
  });
});
