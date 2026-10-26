vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BackpackExchange } from '../../exchanges/backpack/backpack.exchange';
import { BinanceExchange } from '../../exchanges/binance/binance.exchange';
import { BinancePMExchange } from '../../exchanges/binance/binancepm.exchange';
import { BinanceSpotExchange } from '../../exchanges/binance/binancespot.exchange';
import { BingxExchange } from '../../exchanges/bingx/bingx.exchange';
import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { BitrueExchange } from '../../exchanges/bitrue/bitrue.exchange';
import { BitunixExchange } from '../../exchanges/bitunix/bitunix.exchange';
import { BlofinExchange } from '../../exchanges/blofin/blofin.exchange';
import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { BybitInverseExchange } from '../../exchanges/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../../exchanges/bybit/bybitspot.exchange';
import { CryptocomExchange } from '../../exchanges/cryptocom/cryptocom.exchange';
import { GrvtExchange } from '../../exchanges/grvt/grvt.exchange';
import { KucoinExchange } from '../../exchanges/kucoin/kucoin.exchange';
import { KucoinSpotExchange } from '../../exchanges/kucoin/kucoinspot.exchange';
import { OKXExchange } from '../../exchanges/okx/okx.exchange';
import { ParadexExchange } from '../../exchanges/paradex/paradex.exchange';
import { PhemexExchange } from '../../exchanges/phemex/phemex.exchange';
import { RustEngineExchange as OrderlyExchange } from '../../exchanges/rust/rust.exchange';
import { WeexExchange } from '../../exchanges/weex/weex.exchange';
import { WOOXExchange } from '../../exchanges/woo/woo.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType, PositionMode } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  info: { lotSize: '0.001', minSize: '0.001', ih: '0x030501', bd: 3, qd: 6 },
};

const OPTIONS = { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false };

const create = <T>(Ctor: new (name: any, options: any, store: DefaultStore) => T, name: string, options = OPTIONS) => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new Ctor(name, options, store);
  vi.spyOn(exchange as any, 'log').mockImplementation(() => undefined);
  // Venue internals (private formatters, transports) are driven directly, so the handle is untyped.
  return exchange as any;
};

const limit = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
  ...overrides,
});

const recorder = (response: unknown) => vi.fn(async (..._args: any[]) => ({ data: response }));

// Each case places one regular order carrying `id` and returns the native payload that went out.
type Case = [venue: string, id: string, field: string, run: (id: string) => Promise<Record<string, any>>];

const CASES: Case[] = [
  [
    'binance',
    'caller.id:1/x',
    'newClientOrderId',
    async (id) => create(BinanceExchange, 'binance').formatCreateOrder(limit({ clientOrderId: id })).regular[0],
  ],
  [
    'binancepm',
    'caller-pm_1',
    'newClientOrderId',
    async (id) => {
      const exchange = create(BinancePMExchange, 'binancepm');
      const post = recorder({});
      exchange.unlimitedXHR = { post };
      await expect(exchange.placeOrderWithClientOrderId(limit({ clientOrderId: id }))).resolves.toEqual({
        orderIds: [id],
        clientOrderId: id,
      });
      return post.mock.calls[0][1];
    },
  ],
  [
    'binancespot',
    'caller-spot_1',
    'newClientOrderId',
    async (id) => {
      const exchange = create(BinanceSpotExchange, 'binancespot');
      const post = recorder({});
      exchange.unlimitedXHR = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      return post.mock.calls[0][1];
    },
  ],
  [
    'okx',
    'callerOkx1',
    'clOrdId',
    async (id) => {
      const exchange = create(OKXExchange, 'okx');
      exchange._accountPositionMode = PositionMode.OneWay;
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay as any);
      const post = recorder({ data: [{ ordId: 'okx-1' }] });
      exchange.xhr = { post };
      await expect(exchange.placeOrderWithClientOrderId(limit({ clientOrderId: id }))).resolves.toEqual({
        orderIds: ['okx-1'],
        clientOrderId: id,
      });
      return post.mock.calls[0][1][0];
    },
  ],
  [
    'bybit',
    'caller-bybit_1',
    'orderLinkId',
    async (id) => {
      const exchange = create(BybitExchange, 'bybit');
      vi.spyOn(exchange, 'getOrderPositionIdx').mockResolvedValue(0);
      vi.spyOn(exchange, 'recordRecentlyPlacedOrderFromOpts').mockImplementation(() => undefined);
      const post = recorder({ retMsg: 'OK', result: { orderId: 'bybit-1' } });
      exchange.xhr = { post };
      await expect(exchange.placeOrderWithClientOrderId(limit({ clientOrderId: id }))).resolves.toEqual({
        orderIds: ['bybit-1'],
        clientOrderId: id,
      });
      return post.mock.calls[0][1];
    },
  ],
  [
    'bybit batch',
    'caller-bybit_2',
    'orderLinkId',
    async (id) => {
      const exchange = create(BybitExchange, 'bybit');
      vi.spyOn(exchange, 'getOrderPositionIdx').mockResolvedValue(0);
      vi.spyOn(exchange, 'recordRecentlyPlacedOrderFromOpts').mockImplementation(() => undefined);
      const post = recorder({ retMsg: 'OK', result: { list: [{ orderId: 'b1' }, { orderId: 'b2' }] } });
      exchange.xhr = { post };
      await exchange.placeOrders([limit(), limit({ clientOrderId: id })]);
      return post.mock.calls[0][1].request[1];
    },
  ],
  [
    'bybitinverse',
    'caller-inv_1',
    'orderLinkId',
    async (id) => {
      const exchange = create(BybitInverseExchange, 'bybitinverse');
      vi.spyOn(exchange, 'getOrderPositionIdx').mockResolvedValue(0);
      const post = recorder({ retMsg: 'OK', result: { orderId: 'inv-1' } });
      exchange.xhr = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      return post.mock.calls[0][1];
    },
  ],
  [
    'bybitspot',
    'caller-spot_2',
    'orderLinkId',
    async (id) => {
      const exchange = create(BybitSpotExchange, 'bybitspot');
      const post = recorder({ retMsg: 'OK', result: { orderId: 'spot-1' } });
      exchange.xhr = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      return post.mock.calls[0][1];
    },
  ],
  [
    'bitget',
    'caller.bitget:1',
    'clientOid',
    async (id) => {
      const exchange = create(BitgetExchange, 'bitget');
      vi.spyOn(exchange, 'ensureAccountTypeDetected').mockResolvedValue(undefined);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      return (await exchange.formatCreateOrder(limit({ clientOrderId: id })))[0];
    },
  ],
  [
    'kucoin',
    'caller-kucoin_1',
    'clientOid',
    async (id) => (await create(KucoinExchange, 'kucoin').formatCreateOrder(limit({ clientOrderId: id })))[0],
  ],
  [
    'kucoinspot',
    'caller-kspot_1',
    'clientOid',
    async (id) => {
      const exchange = create(KucoinSpotExchange, 'kucoinspot');
      const post = recorder({ code: '200000', data: { orderId: 'ks-1' } });
      exchange.unlimitedXHR = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      return post.mock.calls[0][1];
    },
  ],
  [
    'phemex',
    'caller phemex 1',
    'clOrdID',
    async (id) => {
      const exchange = create(PhemexExchange, 'phemex');
      vi.spyOn(exchange, 'getOrFetchPositionsForSymbol').mockResolvedValue([]);
      vi.spyOn(exchange, 'fetchRawPositionModeForSymbol').mockResolvedValue(PositionMode.OneWay);
      const get = recorder({ data: { positions: [] } });
      exchange.xhr = { get };
      const payload = (await exchange.formatCreateOrder(limit({ clientOrderId: id })))[0];
      expect(get).not.toHaveBeenCalled();
      return payload;
    },
  ],
  [
    'blofin',
    'CallerBlofin1',
    'clientOrderId',
    async (id) => {
      const exchange = create(BlofinExchange, 'blofin');
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue('cross');
      vi.spyOn(exchange, 'getPositionSide').mockResolvedValue('net');
      return (await exchange.formatNormalOrder(limit({ clientOrderId: id })))[0];
    },
  ],
  [
    'bingx',
    'caller-bingx_1',
    'clientOrderId',
    async (id) => {
      const exchange = create(BingxExchange, 'bingx');
      exchange._accountPositionMode = PositionMode.OneWay;
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      return (await exchange.formatCreateOrder(limit({ clientOrderId: id })))[0];
    },
  ],
  [
    'weex',
    'caller.weex:1',
    'newClientOrderId',
    async (id) => create(WeexExchange, 'weex').formatOrderPayload(limit({ clientOrderId: id })),
  ],
  [
    'bitunix',
    'caller-bitunix-1',
    'clientId',
    async (id) => create(BitunixExchange, 'bitunix').formatCreateOrder(limit({ clientOrderId: id }))[0],
  ],
  [
    'bitrue',
    'caller-bitrue-1',
    'clientOrderId',
    async (id) => create(BitrueExchange, 'bitrue').formatCreateOrder(limit({ clientOrderId: id }))[0],
  ],
  [
    'backpack',
    '4294967295',
    'clientId',
    async (id) => {
      const exchange = create(BackpackExchange, 'backpack');
      const post = recorder({ id: 'bp-1', status: 'New' });
      exchange.xhr = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      const payload = post.mock.calls[0][1];
      // Backpack's uint32 goes out as a number whose decimal form is the caller's string.
      return { ...payload, clientId: String(payload.clientId) };
    },
  ],
  [
    'coinbase',
    'caller-coinbase-1',
    'client_order_id',
    async (id) => create(CoinbaseExchange, 'coinbase').formatCreateOrder(limit({ clientOrderId: id })),
  ],
  [
    'cryptocom',
    'caller-cdc-1',
    'client_oid',
    async (id) => {
      const exchange = create(CryptocomExchange, 'cryptocom');
      const post = recorder({ order_id: 'cdc-1' });
      exchange.xhr = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      return post.mock.calls[0][1];
    },
  ],
  [
    'woo',
    '9007199254740991',
    'client_order_id',
    async (id) => {
      const exchange = create(WOOXExchange, 'woo');
      vi.spyOn(exchange, 'getPositionSideForOrder').mockResolvedValue(undefined);
      const post = recorder({ order_id: 77 });
      exchange.xhr = { post };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      const payload = post.mock.calls[0][1];
      return { ...payload, client_order_id: String(payload.client_order_id) };
    },
  ],
  [
    'orderly',
    'caller-orderly-1',
    'clientOrderId',
    async (id) => {
      const exchange = create(OrderlyExchange, 'salsadex');
      adoptEngineCapabilities(exchange);
      const command = vi.fn(async (_command: { orders: PlaceOrderOpts[] }) => ['99']);
      exchange.host = { command };
      await exchange.placeOrder(limit({ clientOrderId: id }));
      return command.mock.calls[0][0].orders[0];
    },
  ],
  [
    'paradex',
    'caller-paradex-1',
    'client_id',
    async (id) => {
      const exchange = create(ParadexExchange, 'paradex', {
        publicDataOnly: true,
        tuning: { consoleLogging: false },
      } as any);
      exchange.requestSigner = { signOrder: vi.fn(async () => ({ signature: '["r","s"]', signature_timestamp: 1 })) };
      const post = vi.fn(async (_endpoint: string, payload: any) => ({
        data: { id: 'paradex-1', client_id: payload.client_id },
      }));
      exchange.xhr = { post };
      await expect(exchange.placeOrderWithClientOrderId(limit({ clientOrderId: id }))).resolves.toEqual({
        orderIds: ['paradex-1'],
        clientOrderId: id,
      });
      return post.mock.calls[0][1];
    },
  ],
  [
    'grvt',
    '9223372036854775807',
    'client_order_id',
    async (id) => {
      const exchange = create(GrvtExchange, 'grvt', { key: 'key', secret: `0x${'11'.repeat(32)}` } as any);
      exchange.wallet = { address: '0x0000000000000000000000000000000000000001' };
      vi.spyOn(exchange, 'getBuilderInfo').mockResolvedValue({
        builder: '0x0000000000000000000000000000000000000002',
        builderFee: '0.03',
        builderFeeInt: 300,
      });
      const createOrder = vi.fn(async () => ({ result: { order_id: '0x1' } }));
      exchange.grvtClient = { tdgClient: { createOrder }, authenticatedEndpoint: vi.fn(async () => ({})) };
      // GRVT's canonical order ID IS the client_order_id, so the placement result is the caller's ID.
      await expect(exchange.placeOrderWithClientOrderId(limit({ clientOrderId: id }))).resolves.toEqual({
        orderIds: [id],
        clientOrderId: id,
      });
      return (createOrder.mock.calls[0] as any[])[0].order.metadata;
    },
  ],
];

describe('caller clientOrderId is sent verbatim on the one native order', () => {
  it.each(CASES)('%s sends %s as %s', async (_venue, id, field, run) => {
    expect((await run(id))[field]).toBe(id);
  });
});

describe('Binance Spot clientOrderId charset (Spot rejects . : / that USD-M accepts)', () => {
  it.each(['caller.id', 'caller:id', 'caller/id', 'x'.repeat(37), ''])('rejects %j before any write', async (id) => {
    const exchange = create(BinanceSpotExchange, 'binancespot');
    const post = vi.fn();
    exchange.unlimitedXHR = { post };
    const emitted = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    await expect(exchange.placeOrder(limit({ clientOrderId: id }))).rejects.toThrow(
      'Binance Spot clientOrderId must match /^[A-Za-z0-9_-]{1,36}$/',
    );
    expect(emitted).toHaveBeenCalledWith('Binance Spot clientOrderId must match /^[A-Za-z0-9_-]{1,36}$/');
    expect(post).not.toHaveBeenCalled();
  });

  it('accepts the full 36-character Spot range', async () => {
    const exchange = create(BinanceSpotExchange, 'binancespot');
    const post = recorder({});
    exchange.unlimitedXHR = { post };
    const id = `A-z_${'9'.repeat(32)}`;

    await exchange.placeOrder(limit({ clientOrderId: id }));
    expect(post.mock.calls[0][1].newClientOrderId).toBe(id);
  });
});

describe('generated IDs are untouched when no caller ID is supplied', () => {
  it('keeps the Binance broker-prefixed IDs, one per split payload', () => {
    const store = new DefaultStore();
    store.update({ markets: [{ ...MARKET, limits: { ...MARKET.limits, amount: { min: 0.001, max: 0.5 } } }] });
    const exchange = new BinanceExchange('binance', OPTIONS, store) as any;

    const ids = exchange.formatCreateOrder(limit({ amount: 1.2 })).regular.map((p: any) => p.newClientOrderId);
    expect(ids.length).toBeGreaterThan(1);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.every((id: string) => id.startsWith('x-s3vFFRQp'))).toBe(true);
  });
});

describe('updateOrder never carries a caller clientOrderId onto a different native order', () => {
  const openOrder = {
    id: 'venue-1',
    status: OrderStatus.Open,
    symbol: MARKET.symbol,
    type: OrderType.Limit,
    side: OrderSide.Buy,
    price: 50_000,
    amount: 1,
    filled: 0,
    remaining: 1,
    reduceOnly: false,
    lastUpdate: 0,
    info: { clientOrderId: 'caller-tracked-1' },
  };

  it('Binance cancel+replace gives the replacement a generated ID, never the cancelled order caller ID', async () => {
    const exchange = create(BinanceExchange, 'binance');
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue(undefined);
    const placeOrder = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['venue-2']);

    await expect(exchange.updateOrder({ order: openOrder, update: { price: 49_000 } })).resolves.toEqual(['venue-2']);

    expect(cancelOrders).toHaveBeenCalledOnce();
    expect(placeOrder.mock.calls[0][0]).not.toHaveProperty('clientOrderId');
  });

  it('OKX native amend keeps the venue order (and so its clOrdId) without minting a new client ID', async () => {
    const exchange = create(OKXExchange, 'okx');
    const post = recorder({ data: [{ ordId: 'venue-1', sCode: '0' }] });
    exchange.xhr = { post };

    await exchange.updateOrder({ order: openOrder, update: { price: 49_000 } });

    const amend = post.mock.calls.map((call) => call[1]).find((body) => body?.ordId === 'venue-1');
    expect(amend).toBeDefined();
    expect(amend).not.toHaveProperty('clOrdId');
    expect(amend).not.toHaveProperty('newClOrdId');
  });
});
