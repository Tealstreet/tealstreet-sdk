/**
 * The writer RPC: one venue write per request to a `writesOnly` engine, wherever it is
 * hosted. The wire contract is `sdk-rs/engine/schema/writer-rpc/writer-rpc.schema.json`;
 * these guards and the engine both test against its `fixtures.json`.
 */
import type { SerializedCommandError } from '../commandOutcome';

import { serializeCommandError } from '../commandOutcome';

export const WRITER_OPS = [
  'placeOrders',
  'amendOrder',
  'amendOrderWithSymbol',
  'cancelOrders',
  'cancelOrdersWithSymbols',
  'cancelSymbolOrders',
  'setLeverage',
  'setMarginType',
  'setPositionMode',
  'setStpMode',
  'setAccountTier',
  'scheduleCancel',
  'refreshMarkets',
] as const;

export type WriterOp = (typeof WRITER_OPS)[number];

export const WRITE_TIMING_STAGES = [
  'called',
  'received',
  'accepted',
  'signed',
  'sent',
  'response',
  'reply',
  'resolved',
] as const;

export type WriteTimingStage = (typeof WRITE_TIMING_STAGES)[number];
export type WriteMarks = Partial<Record<WriteTimingStage, number>>;

/** An engine host command without a completion id; the hosting engine stamps that. */
export type WriterCommand = { op: WriterOp; [field: string]: unknown };

export type ContextOrder = {
  id: string;
  symbol: string;
  side: 'buy' | 'sell';
  type: string;
  price: number;
  amount: number;
  remaining: number;
  reduceOnly: boolean;
  clientOrderId?: string;
  triggerPrice?: number;
};

export type ContextPosition = { symbol: string; side: 'long' | 'short'; contracts: number };

export type SymbolSettings = { hedged?: boolean; positionIdx?: number; marginMode?: 'isolated' | 'cross' };

/** The store fields one write reads, in command units. The writer keeps none of it. */
export type WriteContext = {
  orders?: ContextOrder[];
  positions?: ContextPosition[];
  settings?: Record<string, SymbolSettings>;
};

export type WriterRequest = {
  kind: 'writerRequest';
  id: string;
  account: string;
  command: WriterCommand;
  context: WriteContext;
  marks?: WriteMarks;
};

export type WriterReply =
  | { kind: 'writerReply'; id: string; ok: true; result: unknown; marks?: WriteMarks }
  | { kind: 'writerReply'; id: string; ok: false; error: SerializedCommandError; marks?: WriteMarks };

const ORDER_SIDES = ['buy', 'sell'];
const ORDER_TYPES = ['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'];
const POSITION_SIDES = ['long', 'short'];
const MARGIN_MODES = ['isolated', 'cross'];

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const onlyKeys = (value: Record<string, unknown>, keys: readonly string[]) =>
  Object.keys(value).every((key) => keys.includes(key));
const isId = (value: unknown): value is string => typeof value === 'string' && value.length > 0;
const isNumber = (value: unknown): value is number => typeof value === 'number';
const optional = (value: unknown, check: (value: unknown) => boolean) => value === undefined || check(value);

const isMarks = (value: unknown): value is WriteMarks =>
  isRecord(value) &&
  Object.entries(value).every(
    ([stage, at]) => (WRITE_TIMING_STAGES as readonly string[]).includes(stage) && isNumber(at),
  );

const isContextOrder = (value: unknown): value is ContextOrder =>
  isRecord(value) &&
  onlyKeys(value, [
    'id',
    'symbol',
    'side',
    'type',
    'price',
    'amount',
    'remaining',
    'reduceOnly',
    'clientOrderId',
    'triggerPrice',
  ]) &&
  typeof value.id === 'string' &&
  typeof value.symbol === 'string' &&
  ORDER_SIDES.includes(value.side as string) &&
  ORDER_TYPES.includes(value.type as string) &&
  isNumber(value.price) &&
  isNumber(value.amount) &&
  isNumber(value.remaining) &&
  typeof value.reduceOnly === 'boolean' &&
  optional(value.clientOrderId, (id) => typeof id === 'string') &&
  optional(value.triggerPrice, isNumber);

const isContextPosition = (value: unknown): value is ContextPosition =>
  isRecord(value) &&
  onlyKeys(value, ['symbol', 'side', 'contracts']) &&
  typeof value.symbol === 'string' &&
  POSITION_SIDES.includes(value.side as string) &&
  isNumber(value.contracts);

const isSymbolSettings = (value: unknown): value is SymbolSettings =>
  isRecord(value) &&
  onlyKeys(value, ['hedged', 'positionIdx', 'marginMode']) &&
  optional(value.hedged, (hedged) => typeof hedged === 'boolean') &&
  optional(value.positionIdx, (index) => Number.isInteger(index)) &&
  optional(value.marginMode, (mode) => MARGIN_MODES.includes(mode as string));

const isContext = (value: unknown): value is WriteContext =>
  isRecord(value) &&
  onlyKeys(value, ['orders', 'positions', 'settings']) &&
  optional(value.orders, (orders) => Array.isArray(orders) && orders.every(isContextOrder)) &&
  optional(value.positions, (positions) => Array.isArray(positions) && positions.every(isContextPosition)) &&
  optional(value.settings, (settings) => isRecord(settings) && Object.values(settings).every(isSymbolSettings));

const isCommand = (value: unknown): value is WriterCommand =>
  isRecord(value) && (WRITER_OPS as readonly string[]).includes(value.op as string) && !('context' in value);

export const isWriterRequest = (value: unknown): value is WriterRequest =>
  isRecord(value) &&
  onlyKeys(value, ['kind', 'id', 'account', 'command', 'context', 'marks']) &&
  value.kind === 'writerRequest' &&
  isId(value.id) &&
  isId(value.account) &&
  isCommand(value.command) &&
  isContext(value.context) &&
  optional(value.marks, isMarks);

export const isWriterReply = (value: unknown): value is WriterReply => {
  if (!isRecord(value) || !onlyKeys(value, ['kind', 'id', 'ok', 'result', 'error', 'marks'])) return false;
  if (value.kind !== 'writerReply' || !isId(value.id) || !optional(value.marks, isMarks)) return false;
  if (value.ok === true) return 'result' in value && !('error' in value);
  return value.ok === false && !('result' in value) && isRecord(value.error) && typeof value.error.message === 'string';
};

export const writerSuccess = (id: string, result: unknown, marks?: WriteMarks): WriterReply => ({
  kind: 'writerReply',
  id,
  ok: true,
  // JSON has no undefined: a write that resolves nothing replies null.
  result: result === undefined ? null : result,
  ...(marks ? { marks } : {}),
});

export const writerFailure = (id: string, error: unknown, marks?: WriteMarks): WriterReply => ({
  kind: 'writerReply',
  id,
  ok: false,
  error: serializeCommandError(error),
  ...(marks ? { marks } : {}),
});
