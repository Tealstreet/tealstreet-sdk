import { describe, expect, it } from 'vitest';

import {
  inferHyperliquidOrderTypeAndPrice,
  parseHyperliquidOrderTimeInForce,
} from '../exchanges/hyperliquid/hyperliquid.order-inference';
import { OrderTimeInForce, OrderType } from '../types';

describe('hyperliquid.order-inference', () => {
  it('infers limit orders from explicit orderType', () => {
    const inferred = inferHyperliquidOrderTypeAndPrice({
      orderType: 'Limit',
      limitPx: '123.45',
    });

    expect(inferred).toEqual({
      type: OrderType.Limit,
      price: 123.45,
      source: 'orderType=Limit',
    });
  });

  it('infers take-profit orders from trigger+tpsl', () => {
    const inferred = inferHyperliquidOrderTypeAndPrice({
      triggerPx: '2500',
      tpsl: 'tp',
    });

    expect(inferred).toEqual({
      type: OrderType.TakeProfit,
      price: 2500,
      source: 'triggerPx+tpsl=tp',
    });
  });

  it('infers stop-loss orders from trigger-only payloads', () => {
    const inferred = inferHyperliquidOrderTypeAndPrice({
      triggerPx: '1800',
    });

    expect(inferred).toEqual({
      type: OrderType.StopLoss,
      price: 1800,
      source: 'triggerPx-only',
    });
  });

  it('falls back to limit for limitPx without orderType', () => {
    const inferred = inferHyperliquidOrderTypeAndPrice({
      limitPx: '99.5',
    });

    expect(inferred).toEqual({
      type: OrderType.Limit,
      price: 99.5,
      source: 'no-orderType-fallback',
    });
  });

  it('falls back to market when no useful fields exist', () => {
    const inferred = inferHyperliquidOrderTypeAndPrice({});

    expect(inferred).toEqual({
      type: OrderType.Market,
      price: 0,
      source: 'fallback-market',
    });
  });

  it('parses Hyperliquid tif codes', () => {
    expect(parseHyperliquidOrderTimeInForce('Alo')).toBe(OrderTimeInForce.PostOnly);
    expect(parseHyperliquidOrderTimeInForce('Gtc')).toBe(OrderTimeInForce.GoodTillCancel);
    expect(parseHyperliquidOrderTimeInForce('Ioc')).toBeUndefined();
    expect(parseHyperliquidOrderTimeInForce(undefined)).toBeUndefined();
  });
});
