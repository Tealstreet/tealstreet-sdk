import { describe, expect, it, vi } from 'vitest';

import { composeClientOrderId } from '../composeClientOrderId';
import { BinanceExchange } from '../exchanges/binance/binance.exchange';
import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import { BlofinExchange } from '../exchanges/blofin/blofin.exchange';
import { BybitExchange } from '../exchanges/bybit/bybit.exchange';
import { BybitInverseExchange } from '../exchanges/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../exchanges/bybit/bybitspot.exchange';
import { OKXExchange } from '../exchanges/okx/okx.exchange';
import { OKXEUExchange } from '../exchanges/okx/okxeu.exchange';
import type { Market } from '../types';

import { classifyBinancePlacement, interpretBinanceLookup } from './binance';
import { classifyBitgetPlacement, interpretBitgetLookup } from './bitget';
import { classifyBlofinPlacement, interpretBlofinLookup } from './blofin';
import { classifyBybitPlacement, interpretBybitLookup } from './bybit';
import { trackedRegularOrderCapabilityFor } from './capability';
import { classifyOkxPlacement, interpretOkxLookup } from './okx';
import { classifyPlaceOrderIds } from './placementIds';

const linearUsdt = (overrides: Partial<Market> = {}): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
    ...overrides,
  }) as Market;

const createStore = (market: Market) =>
  ({
    subscribe: vi.fn(),
    markets: [market],
    marketsBySymbol: { [market.symbol]: market },
    tickersBySymbol: {},
    orders: [],
    positions: [],
  }) as never;

const axiosError = (data: unknown) => ({ response: { data } });

describe('classifyPlaceOrderIds', () => {
  it('accepts exactly one nonempty venue order ID', () => {
    expect(classifyPlaceOrderIds(['oid-1'])).toEqual({ kind: 'accepted', venueOrderId: 'oid-1' });
  });

  it('is ambiguous for zero, multiple, or non-list results', () => {
    expect(classifyPlaceOrderIds([])).toMatchObject({ kind: 'ambiguous' });
    expect(classifyPlaceOrderIds(['a', 'b'])).toMatchObject({ kind: 'ambiguous' });
    expect(classifyPlaceOrderIds('oid-1')).toMatchObject({ kind: 'ambiguous' });
  });
});

describe('Bybit placement and lookup', () => {
  it('maps surviving 110072 to ambiguous and keeps code-lost throws ambiguous', () => {
    expect(
      classifyBybitPlacement({
        kind: 'thrown',
        error: axiosError({ retCode: 110072, retMsg: 'OrderLinkedID is duplicate' }),
      }),
    ).toEqual({
      kind: 'ambiguous',
      reason: 'OrderLinkedID is duplicate',
    });
    expect(classifyBybitPlacement({ kind: 'thrown', error: new Error('OrderLinkedID is duplicate') })).toEqual({
      kind: 'ambiguous',
      reason: 'bybit-thrown-code-lost',
    });
  });

  it('treats empty list as indeterminate, never confirmed_absent', () => {
    const raw = { retCode: 0, result: { category: 'linear', list: [] } };
    expect(interpretBybitLookup(raw, 'wire-1', 'BTCUSDT')).toMatchObject({
      kind: 'indeterminate',
      reason: 'bybit-empty-list-not-absence',
    });
  });

  it('finds a matching linear row', () => {
    const raw = {
      retCode: 0,
      result: {
        category: 'linear',
        list: [
          {
            orderLinkId: 'wire-1',
            orderId: 'oid-1',
            symbol: 'BTCUSDT',
            orderStatus: 'Filled',
            qty: '0.01',
            cumExecQty: '0.01',
            avgPrice: '70000',
          },
        ],
      },
    };
    expect(interpretBybitLookup(raw, 'wire-1', 'BTCUSDT')).toMatchObject({
      kind: 'found',
      venueOrderId: 'oid-1',
      status: 'filled',
      totalAmount: '0.01',
      cumulativeFilledAmount: '0.01',
    });
  });
});

describe('fail-closed absence and duplicate placement', () => {
  it('leaves every currently unbounded venue absence indeterminate', () => {
    expect(interpretOkxLookup({ code: '51603', data: [] }, 'wire', 'BTC-USDT-SWAP')).toMatchObject({
      kind: 'indeterminate',
      reason: 'okx-absence-unconfirmable-no-finite-visibility-bound',
    });
    expect(interpretBlofinLookup({ code: '0', data: null }, 'wire', 'BTC-USDT')).toMatchObject({
      kind: 'indeterminate',
      reason: 'blofin-absence-unconfirmable-no-finite-visibility-bound',
    });
    expect(interpretBinanceLookup({ code: -2013, msg: 'Order does not exist.' }, 'wire', 'BTCUSDT')).toMatchObject({
      kind: 'indeterminate',
      reason: 'binance-absence-unconfirmable-no-finite-visibility-bound',
    });
    expect(interpretBitgetLookup({ code: '40109', data: null }, 'wire', 'BTCUSDT')).toMatchObject({
      kind: 'indeterminate',
      reason: 'bitget-40109-visibility-unproven',
    });
  });

  it('maps every surviving duplicate code to ambiguous so exact lookup decides truth', () => {
    expect(
      classifyOkxPlacement({
        kind: 'thrown',
        error: axiosError({ data: [{ sCode: '50071', sMsg: 'Duplicate clOrdId' }] }),
      }),
    ).toMatchObject({ kind: 'ambiguous' });
    expect(
      classifyBitgetPlacement({
        kind: 'thrown',
        error: axiosError({ code: '45034', msg: 'duplicate clientOid' }),
      }),
    ).toMatchObject({ kind: 'ambiguous' });
    expect(
      classifyBlofinPlacement({
        kind: 'thrown',
        error: axiosError({ code: '102002', data: [{ code: '102002' }] }),
      }),
    ).toMatchObject({ kind: 'ambiguous' });
    expect(
      classifyBinancePlacement({
        kind: 'thrown',
        error: axiosError({ code: -4116, msg: 'Client order id is not unique' }),
      }),
    ).toMatchObject({ kind: 'ambiguous' });
  });

  it('rejects malformed and contradictory quantity observations', () => {
    expect(
      interpretBinanceLookup(
        {
          clientOrderId: 'wire',
          orderId: 1,
          symbol: 'BTCUSDT',
          status: 'FILLED',
          origQty: '1',
          executedQty: '0.5',
        },
        'wire',
        'BTCUSDT',
      ),
    ).toMatchObject({ kind: 'indeterminate', reason: 'binance-row-quantity-invalid' });
    expect(
      interpretBitgetLookup(
        {
          code: '00000',
          data: {
            clientOid: 'wire',
            orderId: '1',
            symbol: 'BTCUSDT',
            state: 'partially_filled',
            size: '1',
            baseVolume: '2',
          },
        },
        'wire',
        'BTCUSDT',
      ),
    ).toMatchObject({ kind: 'indeterminate', reason: 'bitget-row-quantity-invalid' });
  });
});

describe('trackedRegularOrderCapabilityFor admission', () => {
  const creds = { key: 'key', secret: 'secret' };
  const okxCreds = { ...creds, passphrase: 'pass' };

  it('admits all five eligible concrete adapter families', () => {
    const binanceMarket = linearUsdt();
    const bybitMarket = linearUsdt();
    const okxMarket = linearUsdt({ id: 'BTC-USDT-SWAP' });
    const bitgetMarket = linearUsdt({ id: 'BTCUSDT' });
    const blofinMarket = linearUsdt({ id: 'BTC-USDT' });

    const binance = new BinanceExchange('binance', creds, createStore(binanceMarket));
    const bybit = new BybitExchange('bybit', creds, createStore(bybitMarket));
    const okx = new OKXExchange('okx', okxCreds, createStore(okxMarket));
    const bitget = new BitgetExchange('bitget', okxCreds, createStore(bitgetMarket));
    vi.spyOn(bitget, 'isClassicV2Account', 'get').mockReturnValue(true);
    const blofin = new BlofinExchange('blofin', okxCreds, createStore(blofinMarket));

    expect(trackedRegularOrderCapabilityFor(binance, binanceMarket)?.composeClientOrderId('caller1')).toBe(
      composeClientOrderId('binance', 'caller1'),
    );
    expect(trackedRegularOrderCapabilityFor(bybit, bybitMarket)?.composeClientOrderId('caller1')).toBe(
      composeClientOrderId('bybit', 'caller1'),
    );
    expect(trackedRegularOrderCapabilityFor(okx, okxMarket)?.composeClientOrderId('caller1')).toBe(
      composeClientOrderId('okx', 'caller1'),
    );
    expect(trackedRegularOrderCapabilityFor(bitget, bitgetMarket)?.composeClientOrderId('caller1')).toBe(
      composeClientOrderId('bitget', 'caller1'),
    );
    expect(trackedRegularOrderCapabilityFor(blofin, blofinMarket)?.composeClientOrderId('caller1')).toBe(
      composeClientOrderId('blofin', 'caller1'),
    );
  });

  it('returns no Bitget capability until Classic mode is proven', () => {
    const market = linearUsdt();
    const bitget = new BitgetExchange('bitget', okxCreds, createStore(market));
    expect(trackedRegularOrderCapabilityFor(bitget, market)).toBeUndefined();
  });

  it('rejects spot, inverse, expiring, non-USDT, read-only, public-only, and sibling adapters', () => {
    const rejectedMarkets = [
      linearUsdt({ isSpot: true }),
      linearUsdt({ isInverse: true, quote: 'USD' }),
      linearUsdt({ isExpiring: true }),
      linearUsdt({ quote: 'USDC' }),
      linearUsdt({ isReadOnly: true }),
    ];
    for (const market of rejectedMarkets) {
      const bybit = new BybitExchange('bybit', creds, createStore(market));
      expect(trackedRegularOrderCapabilityFor(bybit, market)).toBeUndefined();
    }

    const publicMarket = linearUsdt();
    const publicOnly = new BybitExchange('bybit', { ...creds, publicDataOnly: true }, createStore(publicMarket));
    expect(trackedRegularOrderCapabilityFor(publicOnly, publicMarket)).toBeUndefined();

    const inverseMarket = linearUsdt({ symbol: 'BTCUSD', quote: 'USD', isInverse: true });
    const inverse = new BybitInverseExchange('bybitinverse', creds, createStore(inverseMarket));
    expect(trackedRegularOrderCapabilityFor(inverse, inverseMarket)).toBeUndefined();

    const spotMarket = linearUsdt({ isSpot: true });
    const spot = new BybitSpotExchange('bybitspot', creds, createStore(spotMarket));
    expect(trackedRegularOrderCapabilityFor(spot, spotMarket)).toBeUndefined();

    const okxeuMarket = linearUsdt({ id: 'BTC-USDT-SWAP' });
    const okxeu = new OKXEUExchange('okxeu', okxCreds, createStore(okxeuMarket));
    expect(trackedRegularOrderCapabilityFor(okxeu, okxeuMarket)).toBeUndefined();
  });

  it('normalizes Binance store IDs to the wire symbol for exact lookup', async () => {
    const market = linearUsdt({ id: 'BTC/USDT:USDT' });
    const exchange = new BinanceExchange('binance', creds, createStore(market));
    exchange.getOrderByClientOrderId = vi.fn().mockResolvedValue({
      clientOrderId: 'wire-1',
      orderId: 123,
      symbol: 'BTCUSDT',
      status: 'FILLED',
      origQty: '0.001',
      executedQty: '0.001',
      avgPrice: '100000',
    });
    const capability = trackedRegularOrderCapabilityFor(exchange, market);

    await expect(capability?.lookupExact({ wireClientOrderId: 'wire-1' })).resolves.toMatchObject({
      kind: 'found',
      venueOrderId: '123',
      wireClientOrderId: 'wire-1',
      status: 'filled',
    });
    expect(exchange.getOrderByClientOrderId).toHaveBeenCalledWith('BTCUSDT', 'wire-1');
  });

  it('lookupExact interprets a single initial observation and does not promote absence', async () => {
    const market = linearUsdt();
    const exchange = new BybitExchange('bybit', creds, createStore(market));
    exchange.getOrderByClientOrderId = vi.fn().mockResolvedValue({
      retCode: 0,
      result: { category: 'linear', list: [] },
    });
    const capability = trackedRegularOrderCapabilityFor(exchange, market);
    expect(capability).toBeDefined();
    const result = await capability!.lookupExact({ wireClientOrderId: 'wire-1' });
    expect(result.kind).toBe('indeterminate');
    expect(exchange.getOrderByClientOrderId).toHaveBeenCalledTimes(1);
  });
});
