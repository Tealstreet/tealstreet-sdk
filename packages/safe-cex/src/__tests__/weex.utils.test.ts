import { describe, expect, it } from 'vitest';

import { WeexExchange } from './reference/weex/weex.exchange';
import {
  normalizeMarket,
  normalizePosition,
  normalizePublicTrade,
  normalizeTicker,
  normalizeTimestampToMs,
} from './reference/weex/weex.utils';
import { DefaultStore } from '../store/store.base';
import {
  MarginType,
  OrderSide,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../types';

const rawMarket = {
  symbol: 'BTCUSDT',
  baseAsset: 'BTC',
  quoteAsset: 'USDT',
  marginAsset: 'USDT',
  pricePrecision: 1,
  quantityPrecision: 4,
  baseAssetPrecision: 3,
  quotePrecision: 8,
  contractVal: 0.0001,
  forwardContractFlag: true,
  minLeverage: 1,
  maxLeverage: 400,
  makerFeeRate: 0.0002,
  takerFeeRate: 0.0008,
  minOrderSize: 0.0001,
  maxOrderSize: 1200,
  maxPositionSize: 1000000,
  marketOpenLimitSize: 100,
};

describe('WEEX normalizers', () => {
  it('normalizes exchangeInfo symbols into app markets', () => {
    const market = normalizeMarket(rawMarket);

    expect(market).toMatchObject({
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.0001, price: 0.1 },
      limits: {
        amount: { min: 0.0001, max: 1200 },
        leverage: { min: 1, max: 400 },
      },
      contractSize: 0.0001,
      makerFee: 0.0002,
      takerFee: 0.0008,
    });
  });

  it('normalizes WEEX fractional ticker percentages into percent units', () => {
    const market = normalizeMarket(rawMarket);
    const ticker = normalizeTicker(
      {
        symbol: 'BTCUSDT',
        lastPrice: '78114.1',
        openPrice: '77040.0',
        priceChangePercent: '0.013942',
        markPrice: '78114.1',
        indexPrice: '78152.613',
        volume: '43947.1012',
        quoteVolume: '3433014029.98339',
      },
      market,
      {
        bidPrice: '78114.1',
        askPrice: '78114.2',
      },
    );

    expect(ticker.percentage).toBeCloseTo(1.3942);
    expect(ticker.bid).toBe(78114.1);
    expect(ticker.ask).toBe(78114.2);
  });

  it('uses isBuyerMaker when normalizing public trade side', () => {
    expect(
      normalizePublicTrade(
        {
          id: 'trade-1',
          time: 1777700217242,
          price: '78114.1',
          qty: '0.0031',
          quoteQty: '242.15371',
          isBuyerMaker: true,
        },
        'BTCUSDT',
      )?.side,
    ).toBe(OrderSide.Sell);

    expect(
      normalizePublicTrade(
        {
          id: 'trade-2',
          time: 1777700217243,
          price: '78114.1',
          qty: '0.0031',
          isBuyerMaker: false,
        },
        'BTCUSDT',
      )?.side,
    ).toBe(OrderSide.Buy);
  });

  it('preserves symbol settings as position fallbacks when rows omit them', () => {
    const market = normalizeMarket(rawMarket);
    const position = normalizePosition(
      {
        symbol: 'BTCUSDT',
        side: 'SHORT',
        positions: '0.02',
        openValue: '1500',
        leverage: '25',
        unrealizePnl: '-3.5',
      },
      market,
      {
        positionMode: PositionMode.Hedged,
        marginType: MarginType.Isolated,
      },
    );

    expect(position).toMatchObject({
      symbol: 'BTCUSDT',
      side: PositionSide.Short,
      contracts: 0.02,
      notional: 1500,
      entryPrice: 75000,
      leverage: 25,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.Hedged,
    });
  });

  it('normalizes second, millisecond, microsecond, and nanosecond timestamps to milliseconds', () => {
    expect(normalizeTimestampToMs(1777700217)).toBe(1777700217000);
    expect(normalizeTimestampToMs(1777700217000)).toBe(1777700217000);
    expect(normalizeTimestampToMs(1777700217000000)).toBe(1777700217000);
    expect(normalizeTimestampToMs(1777700217000000000)).toBe(1777700217000);
  });
});

describe('WEEX order payload formatting', () => {
  const createExchange = () => {
    const store = new DefaultStore();
    const exchange = new WeexExchange('weex', { key: '', secret: '', passphrase: '', publicDataOnly: true }, store);
    store.update({ markets: [normalizeMarket(rawMarket)] });
    return exchange;
  };

  it('formats regular limit orders with native WEEX fields', () => {
    const exchange = createExchange();
    const payload = (exchange as any).formatOrderPayload({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.123456,
      price: 78123.45,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(payload).toMatchObject({
      symbol: 'BTCUSDT',
      side: 'BUY',
      positionSide: 'LONG',
      type: 'LIMIT',
      timeInForce: 'GTC',
      quantity: '0.1235',
      price: '78123.5',
    });
    expect(payload.newClientOrderId).toMatch(/^ts/);
  });

  it('formats standalone TP/SL plan orders against the TP/SL endpoint schema', () => {
    const exchange = createExchange();
    const payload = (exchange as any).formatOrderPayload({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.01,
      price: 77000.04,
      reduceOnly: true,
      slTriggerType: OrderStopTriggerType.MarkPrice,
    });

    expect(payload).toMatchObject({
      symbol: 'BTCUSDT',
      planType: 'STOP_LOSS',
      triggerPrice: '77000',
      executePrice: '0',
      quantity: '0.01',
      positionSide: 'LONG',
      triggerPriceType: 'MARK_PRICE',
    });
    expect(payload.clientAlgoId).toMatch(/^ts/);
  });
});
