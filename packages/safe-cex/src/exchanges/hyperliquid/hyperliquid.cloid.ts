import type { ClientOrderIdRule } from '../clientOrderId';
import type { OrderWire } from './hyperliquid.types';

import { OrderSide, OrderTimeInForce, OrderType } from '../../types';
import { patternClientOrderIdRule } from '../clientOrderId';
import { parseHyperliquidOrderTimeInForce } from './hyperliquid.order-inference';

/**
 * Hyperliquid client order IDs (cloids): `0x` + 32 hex chars (128 bits), wire
 * field `c`, allowed on every order including triggers and TP/SL groupings.
 * Lowercase only: the venue echoes cloids back lowercase on `orderUpdates` and
 * `frontendOpenOrders`, so an uppercase caller ID would never match its record.
 */
export const HYPERLIQUID_CLOID_PATTERN = /^0x[0-9a-f]{32}$/;

const CLOID_BYTES = 16;

/** A fresh random cloid. Works wherever `crypto.getRandomValues` exists (browser, worker, Node, Bun). */
export const mintHyperliquidCloid = (): string => {
  const bytes = new Uint8Array(CLOID_BYTES);
  const cryptoApi = (globalThis as { crypto?: { getRandomValues?: (array: Uint8Array) => Uint8Array } }).crypto;
  if (cryptoApi?.getRandomValues) {
    cryptoApi.getRandomValues(bytes);
  } else {
    for (let i = 0; i < CLOID_BYTES; i++) bytes[i] = Math.floor(Math.random() * 256);
  }
  let hex = '0x';
  for (let i = 0; i < CLOID_BYTES; i++) hex += bytes[i].toString(16).padStart(2, '0');
  return hex;
};

/**
 * Every shape we place maps the caller's order to ONE native order carrying
 * the ID: a standalone or positional stop is one wire, and on a bracket the ID
 * goes on the entry wire while each TP/SL leg gets its own minted cloid.
 */
export const hyperliquidClientOrderIdRule: ClientOrderIdRule = patternClientOrderIdRule(
  'Hyperliquid',
  HYPERLIQUID_CLOID_PATTERN,
  {
    isHonorableShape: (opts) => opts.type !== OrderType.TrailingStopLoss,
    mint: mintHyperliquidCloid,
  },
);

/** The cloid a venue row/frame carries, or undefined (`null`, absent or malformed). */
export const readHyperliquidCloid = (value: unknown): string | undefined =>
  typeof value === 'string' && HYPERLIQUID_CLOID_PATTERN.test(value) ? value : undefined;

/**
 * What we know about an order at the moment we sign it, keyed by its cloid.
 * `price` is what the order row shows: the trigger price for TP/SL, the limit
 * price otherwise (a market order is a Gtc limit at the slippage cap).
 */
export type HyperliquidPlacementRecord = {
  readonly type: OrderType;
  readonly price: number;
  readonly side: OrderSide;
  readonly amount: number;
  readonly reduceOnly: boolean;
  readonly timeInForce?: OrderTimeInForce;
  readonly symbol: string;
  readonly placedAt: number;
};

/** Builds the record from the wire actually sent, so it cannot drift from what the venue received. */
export const placementRecordFromWire = (
  wire: OrderWire,
  symbol: string,
  placedAt: number,
): HyperliquidPlacementRecord => {
  const trigger = wire.t.trigger;
  return {
    type: trigger ? (trigger.tpsl === 'tp' ? OrderType.TakeProfit : OrderType.StopLoss) : OrderType.Limit,
    price: Number(trigger ? trigger.triggerPx : wire.p),
    side: wire.b ? OrderSide.Buy : OrderSide.Sell,
    amount: Number(wire.s),
    reduceOnly: wire.r,
    timeInForce: trigger ? undefined : parseHyperliquidOrderTimeInForce(wire.t.limit?.tif),
    symbol,
    placedAt,
  };
};
