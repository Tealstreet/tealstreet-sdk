import type { PlaceOrderOpts } from '../types';

import { OrderType } from '../types';
import { uuid } from '../utils/uuid';

/**
 * How a venue honors `PlaceOrderOpts.clientOrderId`. The adapter sends the
 * caller value verbatim on exactly one native order, or rejects before any
 * write — see packages/safe-cex/CLAUDE.md → "Client order ID contract".
 */
export type ClientOrderIdRule = {
  readonly label: string;
  readonly requirement: string;
  readonly isValid: (clientOrderId: string) => boolean;
  readonly isHonorableShape?: (opts: PlaceOrderOpts) => boolean;
  /** The venue's own generated-id format, so a minted id keeps its broker attribution. */
  readonly mint?: () => string;
};

// Tried after a venue's own `mint`: one fits every alphanumeric rule, one every numeric rule.
export const defaultMintCandidates = (): string[] => [
  `ts${uuid()}`,
  String(100_000_000 + Math.floor(Math.random() * 900_000_000)),
];

export const isRegularClientOrderShape = (opts: PlaceOrderOpts): boolean =>
  (opts.type === OrderType.Limit || opts.type === OrderType.Market) &&
  opts.triggerPrice === undefined &&
  opts.stopLoss === undefined &&
  opts.takeProfit === undefined;

export const clientOrderIdShapeMessage = (label: string, opts: PlaceOrderOpts): string =>
  `${label} clientOrderId cannot be honored for this order shape (type=${opts.type}` +
  `${opts.stopLoss !== undefined ? ' +stopLoss' : ''}` +
  `${opts.takeProfit !== undefined ? ' +takeProfit' : ''}` +
  `${opts.triggerPrice !== undefined ? ' +triggerPrice' : ''}` +
  `${opts.close ? ' +close' : ''}` +
  '); caller client order IDs are supported only on plain Limit/Market orders';

export const clientOrderIdSplitMessage = (label: string): string =>
  `${label} clientOrderId is supported only when one order maps to one native order`;

export const patternClientOrderIdRule = (
  label: string,
  pattern: RegExp,
  overrides: Pick<ClientOrderIdRule, 'isHonorableShape' | 'mint'> = {},
): ClientOrderIdRule => ({
  label,
  requirement: `match ${pattern}`,
  isValid: (clientOrderId) => pattern.test(clientOrderId),
  ...overrides,
});

export type PlacedOrderRef = {
  /** Venue order IDs exactly as `placeOrder` resolved them. */
  readonly orderIds: string[];
  /** The client order ID sent on the native order — the caller's value, verbatim. */
  readonly clientOrderId: string;
};

export type ClientOrderIdPlaceOrderOpts = PlaceOrderOpts & { readonly clientOrderId: string };
