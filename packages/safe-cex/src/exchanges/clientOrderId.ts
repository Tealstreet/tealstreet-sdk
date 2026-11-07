import type { Order, PlaceOrderOpts } from '../types';

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

/**
 * What a cancel+replace `updateOrder` resolves to: the replacement's venue
 * order IDs, plus the client order ID the replacement went out with. It is
 * still a `string[]`, so callers reading `result[0]` are unaffected; the ID is
 * what identifies the replacement when the venue answers without an order ID
 * (Hyperliquid positional TP/SL, `"waitingForTrigger"`).
 */
export type ReplacementOrderIds = string[] & { readonly clientOrderId?: string };

export const withReplacementClientOrderId = (orderIds: string[], clientOrderId: string): ReplacementOrderIds =>
  Object.assign(orderIds, { clientOrderId });

export const replacementClientOrderIdOf = (orderIds: unknown): string | undefined => {
  const clientOrderId = (orderIds as { clientOrderId?: unknown } | null | undefined)?.clientOrderId;
  return typeof clientOrderId === 'string' && clientOrderId ? clientOrderId : undefined;
};

/** `updateOrder`'s result across a worker bridge, where array properties may not survive. */
export type WorkerOrderIds = { orderIds: string[]; clientOrderId?: string };

export const encodeWorkerOrderIds = (orderIds: string[]): WorkerOrderIds => {
  const clientOrderId = replacementClientOrderIdOf(orderIds);
  return { orderIds: [...orderIds], ...(clientOrderId ? { clientOrderId } : {}) };
};

export const decodeWorkerOrderIds = ({ orderIds, clientOrderId }: WorkerOrderIds): ReplacementOrderIds =>
  clientOrderId ? withReplacementClientOrderId([...orderIds], clientOrderId) : [...orderIds];

/**
 * `definitive` is true only when the venue was asked about closed orders too
 * and said no order carries the ID; an open-orders scan can never say that.
 */
export type ClientOrderLookup = { found: true; order: Order } | { found: false; definitive: boolean };

// Where mappers put the venue's client ID on `info` (CLIENT_ORDER_IDS.md); `c` is Binance/BingX/Bitrue WS.
const CLIENT_ID_INFO_FIELDS = [
  'clOrdId',
  'clientOrderId',
  'orderLinkId',
  'clientOid',
  'clOrdID',
  'c',
  'client_id',
  'client_order_id',
  'clientId',
] as const;

// Venues default unset IDs to '', 0 or the string "undefined"; none of those is an ID.
const sameClientOrderId = (value: unknown, clientOrderId: string) =>
  value !== undefined && value !== null && String(value) === clientOrderId;

/** Whether a mapped order carries `clientOrderId`, wherever its venue puts it. */
export const orderCarriesClientOrderId = (order: Order, clientOrderId: string): boolean => {
  if (!clientOrderId || clientOrderId === '0' || clientOrderId === 'undefined') return false;
  if (order.id === clientOrderId || order.parentId === clientOrderId || order.clientOrderId === clientOrderId) {
    return true;
  }
  const info = order.info as Record<string, unknown> | undefined;
  return !!info && CLIENT_ID_INFO_FIELDS.some((field) => sameClientOrderId(info[field], clientOrderId));
};
