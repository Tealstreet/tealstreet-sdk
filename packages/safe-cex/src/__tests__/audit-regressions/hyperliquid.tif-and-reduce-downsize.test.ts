import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { HyperliquidExchange } from '../../exchanges/hyperliquid/hyperliquid.exchange';
import { parseHyperliquidOrderTimeInForce } from '../../exchanges/hyperliquid/hyperliquid.order-inference';
import { DefaultStore } from '../../store/store.base';
import { OrderTimeInForce } from '../../types';

const createExchange = () =>
  new HyperliquidExchange('hyperliquid' as any, { key: 'test', secret: 'test' } as any, new DefaultStore());

// e2e run 31317468325 sent {"s":"336","r":true} against a 168-contract long and
// Hyperliquid rested it at 336 — it does not cap reduce-only size server-side.
describe('hyperliquid reduce-only downsizing', () => {
  it('opts into client-side downsizing', () => {
    expect(exchangeMetadata.hyperliquid.autoDownSizeReduceOrders).toBe(true);
  });

  // Spot has no positions to reduce, so the flag would be meaningless there.
  it('leaves the spot venue on the default', () => {
    expect(exchangeMetadata.hyperliquidspot.autoDownSizeReduceOrders).toBe(false);
  });
});

// The WS `orderUpdates` frame carries no `tif` field for any market, so this
// parser only ever resolves on the REST path.
describe('parseHyperliquidOrderTimeInForce', () => {
  it('maps the wire values REST sends', () => {
    expect(parseHyperliquidOrderTimeInForce('Gtc')).toBe(OrderTimeInForce.GoodTillCancel);
    expect(parseHyperliquidOrderTimeInForce('Alo')).toBe(OrderTimeInForce.PostOnly);
  });

  it('returns undefined for an omitted tif, which is what the WS path always hits', () => {
    expect(parseHyperliquidOrderTimeInForce(undefined)).toBeUndefined();
    expect(parseHyperliquidOrderTimeInForce(null)).toBeUndefined();
  });
});

// Core perps get no optimistic store row (that is HIP-3 only) and tick() only
// re-fetches orders when HIP-3 dexes exist, so this cache is the sole source of
// time-in-force for them once the WS frame lands.
describe('hyperliquid placement-time timeInForce cache', () => {
  it('returns undefined for an order it never saw', () => {
    expect(createExchange().getPlacedLimitOrderTimeInForce('999')).toBeUndefined();
  });

  it('recalls what placement recorded', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.PostOnly);
    expect(exchange.getPlacedLimitOrderTimeInForce('1')).toBe(OrderTimeInForce.PostOnly);
  });

  it('drops the entry when the order reaches a terminal status', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    exchange.forgetLimitOrderTimeInForce('1');
    expect(exchange.getPlacedLimitOrderTimeInForce('1')).toBeUndefined();
  });

  it('evicts oldest-first past the bound instead of growing without limit', () => {
    const exchange = createExchange();
    for (let i = 0; i < 600; i++) {
      exchange.rememberLimitOrderTimeInForce(`${i}`, OrderTimeInForce.GoodTillCancel);
    }
    expect(exchange.getPlacedLimitOrderTimeInForce('0')).toBeUndefined();
    expect(exchange.getPlacedLimitOrderTimeInForce('599')).toBe(OrderTimeInForce.GoodTillCancel);
  });

  it('re-recording an id refreshes its position rather than duplicating it', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('keep', OrderTimeInForce.PostOnly);
    for (let i = 0; i < 499; i++) {
      exchange.rememberLimitOrderTimeInForce(`${i}`, OrderTimeInForce.GoodTillCancel);
    }
    exchange.rememberLimitOrderTimeInForce('keep', OrderTimeInForce.PostOnly);
    exchange.rememberLimitOrderTimeInForce('overflow', OrderTimeInForce.GoodTillCancel);
    expect(exchange.getPlacedLimitOrderTimeInForce('keep')).toBe(OrderTimeInForce.PostOnly);
  });

  it('clears on dispose', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    exchange.dispose();
    expect(exchange.getPlacedLimitOrderTimeInForce('1')).toBeUndefined();
  });
});

// Mirrors the resolution order in hyperliquid.ws-private.ts `handleOrderUpdates`.
const resolveTimeInForce = (wsTif: unknown, orderId: string, exchange: HyperliquidExchange, stored?: StoredOrder) =>
  parseHyperliquidOrderTimeInForce(wsTif) ?? stored?.timeInForce ?? exchange.getPlacedLimitOrderTimeInForce(orderId);

type StoredOrder = { timeInForce?: OrderTimeInForce };

describe('hyperliquid websocket timeInForce preservation', () => {
  it('keeps the stored value when the frame omits tif', () => {
    const exchange = createExchange();
    expect(resolveTimeInForce(undefined, '1', exchange, { timeInForce: OrderTimeInForce.GoodTillCancel })).toBe(
      OrderTimeInForce.GoodTillCancel,
    );
  });

  it('falls back to the placement record when nothing is stored', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.PostOnly);
    expect(resolveTimeInForce(undefined, '1', exchange, undefined)).toBe(OrderTimeInForce.PostOnly);
  });

  it('prefers the frame over both fallbacks when tif is present', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    expect(resolveTimeInForce('Alo', '1', exchange, { timeInForce: OrderTimeInForce.GoodTillCancel })).toBe(
      OrderTimeInForce.PostOnly,
    );
  });

  it('no longer resolves to undefined for a plain core-perp GTC order', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    expect(resolveTimeInForce(undefined, '1', exchange, undefined)).not.toBeUndefined();
  });
});
