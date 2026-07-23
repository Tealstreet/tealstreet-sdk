import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  BITMEX_EXCHANGE_CLOSURE_MS,
  BITMEX_REDUCE_ONLY_START_MS,
  BitmexExchange,
} from '../exchanges/bitmex/bitmex.exchange';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../types';

const createExchangeLike = (extra: Record<string, unknown> = {}) => {
  const store = {
    subscribe: vi.fn(),
    marketsBySymbol: {
      XBTUSDT: {
        symbol: 'XBTUSDT',
        precision: { price: 0.5, amount: 0.001 },
        info: { isQuanto: false },
        isInverse: false,
      },
      // XBTUSD is an INVERSE contract (not quanto): orderQty is USD-notional
      // contracts, lotSize 100 (real BitMEX value) -> precision.amount = 1 * 100.
      XBTUSD: {
        symbol: 'XBTUSD',
        precision: { price: 0.1, amount: 100 },
        // BitMEX raw instrument carries isInverse on info too; convertFromMicroValue
        // reads market.info.isInverse to skip micro conversion for inverse contracts.
        info: { isQuanto: false, isInverse: true, lotSize: 100 },
        isInverse: true,
      },
      ETHUSD: {
        symbol: 'ETHUSD',
        precision: { price: 0.01, amount: 0.1 },
        contractSize: 0.1,
        info: { isQuanto: true, lotSize: 1 },
        isInverse: false,
      },
    },
    tickersBySymbol: {
      XBTUSD: { symbol: 'XBTUSD', mark: 50000, last: 50000 },
      XBTUSDT: { symbol: 'XBTUSDT', mark: 50000, last: 50000 },
    },
  } as any;

  const exchange = new BitmexExchange(
    'bitmex' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
      extra,
    } as any,
    store,
  );

  exchange.convertInputsToMicro = true;
  exchange.convertToMicroValue = vi.fn((value: number) => value);
  return exchange;
};

describe('BitmexExchange order payload formatting', () => {
  beforeEach(() => {
    vi.spyOn(Date, 'now').mockReturnValue(Date.UTC(2026, 6, 24, 4, 0, 0));
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('omits empty execInst and defaults plain limit orders to GTC', () => {
    const exchange = createExchangeLike();

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50000,
    });

    expect(payload.execInst).toBeUndefined();
    expect(payload.timeInForce).toBe(OrderTimeInForce.GoodTillCancel);
  });

  it('maps PostOnly to ParticipateDoNotInitiate with explicit GTC', () => {
    const exchange = createExchangeLike();

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50000,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(payload.execInst).toBe('ParticipateDoNotInitiate');
    expect(payload.timeInForce).toBe(OrderTimeInForce.GoodTillCancel);
  });

  it('passes through IOC for limit orders without forcing execInst', () => {
    const exchange = createExchangeLike();

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50000,
      timeInForce: OrderTimeInForce.ImmediateOrCancel,
    });

    expect(payload.execInst).toBeUndefined();
    expect(payload.timeInForce).toBe(OrderTimeInForce.ImmediateOrCancel);
  });

  it('sends configured maxSlippagePct on market payloads only', () => {
    const exchange = createExchangeLike({ bitmexMaxSlippagePct: '0.8' });

    const [marketPayload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1,
    });
    const [limitPayload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50000,
    });

    expect(marketPayload.maxSlippagePct).toBe('0.8');
    expect(limitPayload.maxSlippagePct).toBeUndefined();
  });

  it('prefers per-order maxSlippagePct over tracked/configured values', () => {
    const exchange = createExchangeLike({ bitmexMaxSlippagePct: '0.8' });
    (exchange as any).trackBitmexMaxSlippagePct('0.7');

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1,
      maxSlippagePct: 1.25,
    });

    expect(payload.maxSlippagePct).toBe('1.25');
  });

  it('applies tracked maxSlippagePct to attached reduce-only TP/SL legs', () => {
    const exchange = createExchangeLike();
    (exchange as any).trackBitmexMaxSlippagePct('0.9');

    const payloads = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50000,
      stopLoss: 49000,
      takeProfit: 51000,
    });

    expect(payloads[0].maxSlippagePct).toBeUndefined();
    expect(payloads[1]).toMatchObject({ ordType: 'StopMarket', maxSlippagePct: '0.9' });
    expect(payloads[2]).toMatchObject({ ordType: 'MarketIfTouched', maxSlippagePct: '0.9' });
  });

  it('converts ETHUSD quanto base amount to native BitMEX orderQty', () => {
    const exchange = createExchangeLike();

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'ETHUSD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.1,
      price: 2000,
    });

    expect(payload.orderQty).toBe('1');
  });

  it('derives ETHUSD quanto market precision from the base value of one contract', async () => {
    const exchange = createExchangeLike();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: [
          { symbol: '.BXBT', markPrice: 100000 },
          {
            symbol: 'ETHUSD',
            underlying: 'ETH',
            quoteCurrency: 'USD',
            settlCurrency: 'XBt',
            isQuanto: true,
            isInverse: false,
            multiplier: 100,
            markPrice: 2000,
            lotSize: 1,
            tickSize: 0.01,
            initMargin: 0.01,
            maxOrderQty: 10000000,
            bidPrice: 1999,
            askPrice: 2001,
            lastPrice: 2000,
            fairPrice: 2000,
            prevClosePrice: 1900,
            openValue: 1000,
            fundingRate: 0.0001,
            homeNotional24h: 100,
            foreignNotional24h: 200000,
          },
        ],
      }),
    } as any;

    const { markets } = await exchange.fetchMarketsAndTickers();
    const ethusd = markets.find((market) => market.symbol === 'ETHUSD');

    expect(ethusd?.contractSize).toBe(0.1);
    expect(ethusd?.precision.amount).toBe(0.1);
  });

  it('uses converted ETHUSD quanto orderQty for attached TP/SL legs', () => {
    const exchange = createExchangeLike();

    const payloads = (exchange as any).formatCreateOrder({
      symbol: 'ETHUSD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.3,
      price: 2000,
      stopLoss: 1900,
      takeProfit: 2100,
    });

    expect(payloads.map((payload: Record<string, unknown>) => payload.orderQty)).toEqual(['3', '3', '3']);
  });

  it('amends ETHUSD quanto orders using native BitMEX contract quantity', async () => {
    const exchange = createExchangeLike();
    exchange.unlimitedXHR = {
      put: vi.fn().mockResolvedValue({ data: {} }),
    } as any;

    await exchange.updateOrder({
      order: {
        id: 'order-1',
        symbol: 'ETHUSD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        status: OrderStatus.Open,
        price: 2000,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: 0,
      },
      update: { amount: 0.3 },
    });

    expect(exchange.unlimitedXHR.put).toHaveBeenCalledWith('/order', {
      orderID: 'order-1',
      orderQty: 3,
    });
  });

  it('maps ETHUSD quanto BitMEX order quantities back to base ETH', () => {
    const exchange = createExchangeLike();

    const order = (exchange as any).mapOrder({
      orderID: 'order-1',
      symbol: 'ETHUSD',
      side: 'Buy',
      orderQty: 10,
      leavesQty: 4,
      price: 2000,
      ordType: 'Limit',
      ordStatus: 'New',
      timeInForce: 'GoodTillCancel',
      execInst: '',
      text: 'Tealstreet',
    });

    expect(order.amount).toBe(1);
    expect(order.filled).toBe(0.6);
    expect(order.remaining).toBe(0.4);
    expect(order.extra.remainingNative).toBe(4);
  });

  // Regression guard for TSBUG-97: XBTUSD is an inverse contract, NOT quanto.
  // The app sends USD-notional contracts and BitMEX's orderQty is also
  // USD-notional, so the units already match and no base->native conversion
  // is needed (unlike ETHUSD quanto). A realistic order must produce a
  // non-zero orderQty rather than rounding to 0 / omitting orderQty.
  it('sends XBTUSD inverse orderQty as USD-notional contracts (non-zero)', () => {
    const exchange = createExchangeLike();

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1000,
      price: 50000,
    });

    // USD notional passed straight through, snapped to the 100-contract lot.
    expect(payload.orderQty).toBe('1000');
  });

  it('amends XBTUSD inverse orders using USD-notional contract quantity', async () => {
    const exchange = createExchangeLike();
    exchange.unlimitedXHR = {
      put: vi.fn().mockResolvedValue({ data: {} }),
    } as any;

    await exchange.updateOrder({
      order: {
        id: 'order-xbt',
        symbol: 'XBTUSD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        status: OrderStatus.Open,
        price: 50000,
        amount: 1000,
        filled: 0,
        remaining: 1000,
        reduceOnly: false,
        lastUpdate: 0,
      },
      update: { amount: 2000 },
    });

    expect(exchange.unlimitedXHR.put).toHaveBeenCalledWith('/order', {
      orderID: 'order-xbt',
      orderQty: 2000,
    });
  });

  it('sends XBTUSDT linear orderQty through the micro-value path', () => {
    const exchange = createExchangeLike();

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50000,
    });

    // convertToMicroValue is mocked as identity; the linear branch adjusts the
    // base amount to precision (0.001) before/after micro conversion.
    expect(payload.orderQty).toBe('1');
  });

  it('blocks non-reduce-only BitMEX orders during the wind-down reduce-only window', () => {
    vi.mocked(Date.now).mockReturnValue(BITMEX_REDUCE_ONLY_START_MS);
    const exchange = createExchangeLike();

    expect(() =>
      (exchange as any).formatCreateOrder({
        symbol: 'XBTUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 50000,
      }),
    ).toThrow('BitMEX is in reduce-only wind-down mode');

    const [payload] = (exchange as any).formatCreateOrder({
      symbol: 'XBTUSDT',
      type: OrderType.Market,
      side: OrderSide.Sell,
      amount: 1,
      reduceOnly: true,
    });

    expect(payload.execInst).toBe('ReduceOnly');
  });

  it('blocks BitMEX order amendments after exchange closure', async () => {
    vi.mocked(Date.now).mockReturnValue(BITMEX_EXCHANGE_CLOSURE_MS);
    const exchange = createExchangeLike();
    exchange.unlimitedXHR = {
      put: vi.fn().mockResolvedValue({ data: {} }),
    } as any;

    await expect(
      exchange.updateOrder({
        order: {
          id: 'order-closed',
          symbol: 'XBTUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          status: OrderStatus.Open,
          price: 50000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          lastUpdate: 0,
        },
        update: { price: 49900 },
      }),
    ).rejects.toThrow('BitMEX exchange services ended');
    expect(exchange.unlimitedXHR.put).not.toHaveBeenCalled();
  });

  it('maps ETHUSD quanto position quantities back to base ETH', () => {
    const exchange = createExchangeLike();

    const position = (exchange as any).mapPosition({
      symbol: 'ETHUSD',
      currentQty: 10,
      avgCostPrice: 2000,
      markPrice: 2000,
      currency: 'XBt',
      leverage: 5,
      crossMargin: true,
    });

    expect(position.contracts).toBe(1);
    expect(position.notional).toBe(2000);
    expect(position.extra.contractsNative).toBe(10);
  });

  it('maps XBTUSD inverse position contracts to base via reference price', () => {
    const exchange = createExchangeLike();

    const position = (exchange as any).mapPosition({
      symbol: 'XBTUSD',
      currentQty: 10000,
      avgCostPrice: 50000,
      markPrice: 50000,
      currency: 'XBt',
      leverage: 5,
      crossMargin: true,
    });

    // contracts (USD notional) / referencePrice (50000) -> base BTC
    expect(position.contracts).toBe(0.2);
    // notional stays the raw USD-contract value for inverse
    expect(position.notional).toBe(10000);
    expect(position.extra.contractsNative).toBe(10000);
  });
});
