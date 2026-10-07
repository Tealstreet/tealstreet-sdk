import type { OemsReadResult } from './prompt';

import { isBook, shapeFor } from './readShapes';
import type {
  OemsAccountRef,
  OemsBalanceRef,
  OemsContext,
  OemsContextDropped,
  OemsMarketRef,
  OemsOrderRef,
  OemsOrderType,
  OemsPositionRef,
} from './context.types';
import type { OemsTranscriptTurn } from './prompt';

/**
 * Bounds on what may reach a prompt.
 *
 * The context is built in the browser and posted to our own endpoint, so it is
 * the user's own data — but the endpoint pays for it by the token, and a
 * client that sends ten thousand positions costs a hundred times what one
 * request should. These caps are an ABUSE bound, not a product limit: they sit
 * far above any real book, and a user who genuinely holds more than fifty
 * positions loses the tail of a list rather than the request.
 *
 * Truncation is silent on purpose at this layer. The route reports what it
 * dropped; making the bounding function throw would turn "you have a lot of
 * positions" into a failed request.
 */

export const OEMS_CONTEXT_LIMITS = Object.freeze({
  accounts: 40,
  positions: 50,
  orders: 100,
  balances: 40,
  markets: 60,
  transcriptTurns: 10,
  /** One account name, symbol or currency. Generous; a venue symbol is ~12. */
  stringChars: 64,
  /** One transcript turn, and the request itself. */
  messageChars: 2_000,
});

const clip = (value: unknown, max: number): string => (typeof value === 'string' ? value.slice(0, max) : '');

const clipName = (value: unknown): string => clip(value, OEMS_CONTEXT_LIMITS.stringChars);

/** Non-finite numbers are zeroed rather than dropped: `JSON.stringify` turns
 *  `NaN` and `Infinity` into `null`, which reads to a model as "unknown" on a
 *  field that is never optional. */
const num = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) ? value : 0);

const boundAccount = (account: OemsAccountRef): OemsAccountRef => ({
  name: clipName(account.name),
  exchange: clipName(account.exchange),
  active: account.active === true,
  testnet: account.testnet === true,
  connected: account.connected === true,
});

const boundPosition = (position: OemsPositionRef): OemsPositionRef => ({
  account: clipName(position.account),
  symbol: clipName(position.symbol),
  side: position.side === 'short' ? 'short' : 'long',
  contracts: num(position.contracts),
  nativeContracts: num(position.nativeContracts),
  entryPrice: num(position.entryPrice),
  markPrice: num(position.markPrice),
  notional: num(position.notional),
  unrealizedPnl: num(position.unrealizedPnl),
  liquidationPrice: num(position.liquidationPrice),
  leverage: num(position.leverage),
  positionMode: position.positionMode === 'hedged' ? 'hedged' : 'one-way',
});

const ORDER_TYPES: readonly OemsOrderType[] = [
  'market',
  'limit',
  'conditional',
  'stop_market',
  'take_profit_market',
  'trailing_stop_market',
];

/** Normalised against the known set rather than clipped: every other string
 *  here is a name, this one is an enum, and an unknown value is noise a
 *  planner would try to reason about. */
const boundOrderType = (value: unknown): OemsOrderType =>
  ORDER_TYPES.includes(value as OemsOrderType) ? (value as OemsOrderType) : 'limit';

const boundOrder = (order: OemsOrderRef): OemsOrderRef => ({
  account: clipName(order.account),
  id: clipName(order.id),
  symbol: clipName(order.symbol),
  side: order.side === 'sell' ? 'sell' : 'buy',
  type: boundOrderType(order.type),
  amount: num(order.amount),
  filled: num(order.filled),
  remaining: num(order.remaining),
  nativeRemaining: num(order.nativeRemaining),
  price: num(order.price),
  reduceOnly: order.reduceOnly === true,
});

const boundBalance = (balance: OemsBalanceRef): OemsBalanceRef => ({
  account: clipName(balance.account),
  currency: clipName(balance.currency),
  total: num(balance.total),
  free: num(balance.free),
  used: num(balance.used),
  upnl: num(balance.upnl),
});

const boundMarket = (market: OemsMarketRef): OemsMarketRef => ({
  symbol: clipName(market.symbol),
  account: clipName(market.account),
  lastPrice: num(market.lastPrice),
  amountStep: num(market.amountStep),
  priceStep: num(market.priceStep),
  minAmount: num(market.minAmount),
  maxAmount: num(market.maxAmount),
  maxLeverage: num(market.maxLeverage),
  contractSize: num(market.contractSize),
  isInverse: market.isInverse === true,
  isSpot: market.isSpot === true,
});

const take = <T>(list: readonly T[] | undefined, max: number, map: (item: T) => T): T[] =>
  Array.isArray(list) ? list.slice(0, max).map(map) : [];

/**
 * How many rows a pass removed, MERGED with what an earlier pass recorded.
 *
 * A context is bounded in the browser (so the allowlist matches what the model
 * is shown) and again in the route (which cannot trust a browser about its own
 * payload). By the second pass the excess is already gone, so a pass reporting
 * only its own subtraction would answer zero and erase the record — the same
 * trap `boundOne` fell into for read results.
 */
const droppedCount = (before: unknown, after: readonly unknown[], carried: number | undefined): number | undefined => {
  const dropped = Math.max(0, (Array.isArray(before) ? before.length : 0) - after.length);
  const merged = Math.max(dropped, Number.isFinite(carried) ? (carried as number) : 0);
  return merged > 0 ? merged : undefined;
};

const omitEmpty = (dropped: OemsContextDropped): OemsContextDropped | undefined =>
  Object.values(dropped).some((value) => value !== undefined) ? dropped : undefined;

export const boundOemsContext = (context: OemsContext): OemsContext => {
  const accounts = take(context.accounts, OEMS_CONTEXT_LIMITS.accounts, boundAccount);
  const positions = take(context.positions, OEMS_CONTEXT_LIMITS.positions, boundPosition);
  const orders = take(context.orders, OEMS_CONTEXT_LIMITS.orders, boundOrder);
  const balances = take(context.balances, OEMS_CONTEXT_LIMITS.balances, boundBalance);
  const markets = take(context.markets, OEMS_CONTEXT_LIMITS.markets, boundMarket);
  const carried = context.dropped;

  const dropped = omitEmpty({
    accounts: droppedCount(context.accounts, accounts, carried?.accounts),
    positions: droppedCount(context.positions, positions, carried?.positions),
    orders: droppedCount(context.orders, orders, carried?.orders),
    balances: droppedCount(context.balances, balances, carried?.balances),
    markets: droppedCount(context.markets, markets, carried?.markets),
  });

  return {
    generatedAt: num(context.generatedAt),
    activeAccount: clipName(context.activeAccount),
    activeSymbol: clipName(context.activeSymbol),
    activeAccounts: Array.isArray(context.activeAccounts)
      ? context.activeAccounts.slice(0, OEMS_CONTEXT_LIMITS.accounts).map(clipName)
      : [],
    accounts,
    positions,
    orders,
    balances,
    markets,
    fatFingerNotional: num(context.fatFingerNotional),
    ...(dropped ? { dropped } : {}),
  };
};

/** Keeps the MOST RECENT turns — the tail is what the next request is about. */
export const boundOemsTranscript = (transcript: readonly OemsTranscriptTurn[]): OemsTranscriptTurn[] =>
  (Array.isArray(transcript) ? transcript : []).slice(-OEMS_CONTEXT_LIMITS.transcriptTurns).map((turn) => ({
    role: turn?.role === 'assistant' ? 'assistant' : 'user',
    content: clip(turn?.content, OEMS_CONTEXT_LIMITS.messageChars),
  }));

/**
 * Caps for a READ RESULT on its way back into the prompt.
 *
 * The snapshot is bounded because we build it. A read result is bounded because
 * a venue does: `getMarkets` returns several hundred `Market` objects, each
 * carrying `info?: any` — the raw venue payload, of no fixed size and no fixed
 * shape — and 500 candles is roughly 55KB of JSON before anything else is
 * added. Unbounded, one read ends the turn with a truncated response.
 */
export const OEMS_READ_RESULT_LIMITS = Object.freeze({
  /** Array elements kept, by shape of what the tool returns. */
  candles: 200,
  orderbookLevels: 20,
  rows: 60,
  /** Hard backstop per result, after the per-shape caps. */
  chars: 20_000,
  /**
   * Across every result in a turn, not per result. Results ACCUMULATE over
   * hops — four hops of six reads is twenty-four payloads riding in the same
   * prompt — so a per-result cap alone bounds nothing that matters.
   */
  totalChars: 24_000,
  results: 12,
  /** One venue error string. Generous; anything longer is a stack trace. */
  errorChars: 400,
});

/**
 * `info` is dropped wherever it appears.
 *
 * On `Market` it is typed `any` and holds the venue's own response — unbounded,
 * and the largest single contributor to a read result. It is also the field
 * most likely to carry venue-authored prose, which is the last thing that
 * should ride back into a prompt. Nothing the model reasons about needs it.
 */
const TOO_DEEP = '[…]';

const stripInfo = (value: unknown, depth = 0): unknown => {
  if (value === null || typeof value !== 'object') return value;
  // Returns a PLACEHOLDER at the cap, not the original object. Returning the
  // original re-admits whatever was below it — including a self-reference,
  // which then throws out of `JSON.stringify` and kills the turn. A venue
  // payload is not ours and may be shaped any way at all.
  if (depth > 6) return TOO_DEEP;
  if (Array.isArray(value)) return value.map((entry) => stripInfo(entry, depth + 1));
  const out: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
    if (key === 'info') continue;
    out[key] = stripInfo(child, depth + 1);
  }
  return out;
};

/** `JSON.stringify` can still throw — a `BigInt`, or a getter that raises.
 *  Sizing a result must never be the thing that fails a turn. */
const sizeOf = (value: unknown): number => {
  try {
    return JSON.stringify(value)?.length ?? 0;
  } catch {
    return Number.POSITIVE_INFINITY;
  }
};

const capArray = (value: readonly unknown[], keep: number): readonly unknown[] =>
  value.length <= keep ? value : value.slice(-keep);

/**
 * What one bounding pass did.
 *
 * `kept` and `total` are COUNTS, not prose, so no renderer recomputes them: the
 * CLI's since-removed MCP server hand-built "showing 60 of 612" from its own
 * copy of the input while the web path had nothing to render at all. A count is
 * also the difference between a model that narrows its next request and one
 * that reasons over a tail as if it were the set.
 *
 * Both are absent where no count is meaningful — an oversized object replaced
 * by a placeholder has no element count to report.
 */
export interface ReadResultBounding {
  readonly data: unknown;
  readonly truncated: boolean;
  readonly kept?: number;
  readonly total?: number;
}

/**
 * Bound one read result. Returns the value plus whether anything was dropped,
 * so the caller can TELL the model it is looking at a subset — a silently
 * truncated candle series would be reasoned about as if complete.
 */
export const boundReadResult = (tool: string, data: unknown): ReadResultBounding => {
  let truncated = false;
  let kept: number | undefined;
  let total: number | undefined;
  let value = stripInfo(data);
  const shape = shapeFor(tool, value);

  if (shape === 'orderbook' && isBook(value)) {
    const book = value;
    const levels = OEMS_READ_RESULT_LIMITS.orderbookLevels;
    // `isBook` only proves the KEY is there. A venue answering `{bids: 5}`, or
    // a forged body naming any tool at all, would otherwise reach `.slice` on
    // a number and throw out of `readBody` — which the route catches as
    // "Request body must be JSON", a 400 about the wrong thing.
    const bids = Array.isArray(book.bids) ? book.bids : [];
    const asks = Array.isArray(book.asks) ? book.asks : [];
    if (bids.length > levels || asks.length > levels) {
      truncated = true;
      // A book is two sides and one number has to stand for both. The SUM is
      // the honest one: a model told "40 of 900" can see it is holding the
      // touch and not the depth.
      kept = Math.min(bids.length, levels) + Math.min(asks.length, levels);
      total = bids.length + asks.length;
    }
    value = {
      // Nearest the touch is what matters, and a book is sorted from it.
      bids: bids.slice(0, levels),
      asks: asks.slice(0, levels),
    };
  } else if (Array.isArray(value)) {
    const keep = shape === 'candles' ? OEMS_READ_RESULT_LIMITS.candles : OEMS_READ_RESULT_LIMITS.rows;
    if (value.length > keep) {
      truncated = true;
      kept = keep;
      total = value.length;
    }
    value = capArray(value, keep);
  }

  // Backstop. Halving keeps the result VALID JSON; slicing the string would
  // hand the model a broken object and invite it to guess the rest.
  while (Array.isArray(value) && value.length > 1 && sizeOf(value) > OEMS_READ_RESULT_LIMITS.chars) {
    // `total` is what the caller ASKED for, so it is taken BEFORE the halving
    // and only when nothing above already set it — a result the per-shape cap
    // already trimmed must keep reporting the venue's figure, not this one.
    total = total ?? value.length;
    const halved = value.slice(-Math.floor(value.length / 2));
    value = halved;
    truncated = true;
    kept = halved.length;
  }

  // An OBJECT result — a book, a balance, a market — has nothing to halve, and
  // gating the backstop on `Array.isArray` left it with no character cap at
  // all. Only a pathological venue payload gets here (a bounded book is about
  // 2KB against a 20KB cap), and for that a note is a better answer than an
  // unbounded blob riding into the prompt.
  if (sizeOf(value) > OEMS_READ_RESULT_LIMITS.chars) {
    return { data: '[result too large to show]', truncated: true };
  }

  return { data: value, truncated, ...(kept === undefined ? {} : { kept }), ...(total === undefined ? {} : { total }) };
};

/**
 * Bound the accumulated results of a turn.
 *
 * Newest first when something has to go: an older read is likelier to be one
 * the model has already reasoned about, and the one it just asked for is the
 * one it is waiting on. Dropped results are REPLACED by a note rather than
 * vanishing, so the model does not re-request what it already spent a hop on.
 */
/**
 * Re-bounding MERGES with what the previous pass recorded; it does not replace
 * it.
 *
 * A result is bounded twice — once in the browser so the body fits, once in
 * the route so the PROMPT fits — and by the time the second pass runs the
 * first has already thrown the excess away. So a 900-candle series arrives
 * here as 200 elements, sits comfortably under the cap, and a pass that simply
 * reported its own findings would answer `truncated: false` and erase the only
 * record that anything was dropped.
 *
 * `total` is therefore the FIRST pass's figure — what the venue actually
 * returned — while `kept` is the LAST pass's, since that is what the model can
 * see.
 */
const boundOne = (result: OemsReadResult): ReadResultBounding => {
  const pass = boundReadResult(result.tool, result.data);
  const kept = pass.kept ?? result.kept;
  const total = result.total ?? pass.total;
  return {
    data: pass.data,
    truncated: pass.truncated || result.truncated === true,
    ...(kept === undefined ? {} : { kept }),
    ...(total === undefined ? {} : { total }),
  };
};

export const boundReadResults = (results: readonly OemsReadResult[]): readonly OemsReadResult[] => {
  const capped = results.slice(-OEMS_READ_RESULT_LIMITS.results);
  const kept: OemsReadResult[] = [];
  let chars = 0;

  for (let index = capped.length - 1; index >= 0; index -= 1) {
    const result = capped[index];
    const bounded = result.ok
      ? { ...result, ...boundOne(result) }
      : // A failure carries a VENUE string. It skips `boundOne` because there
        // is no `data` to bound, so clip it here or an error message is the one
        // unbounded thing left in the turn.
        { ...result, error: (result.error ?? '').slice(0, OEMS_READ_RESULT_LIMITS.errorChars) };
    const size = sizeOf(bounded);
    if (chars + size > OEMS_READ_RESULT_LIMITS.totalChars && kept.length > 0) {
      kept.unshift({
        tool: result.tool,
        args: result.args,
        ok: false,
        error: 'dropped — too much data was gathered this turn to show this one again',
      });
      continue;
    }
    chars += size;
    kept.unshift(bounded);
  }

  return kept;
};
