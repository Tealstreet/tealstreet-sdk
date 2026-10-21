import { PositionSide, type Ticker } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../types';
import { ID_SUFFIX_RE, peelParenBlock, whitespaceTokenize } from './parserUtils';
import { evaluateLiveVarSource } from './reactiveVars';
import { resolveTrackedVar } from './trackedVars';
import type { PreprocessContext, PreprocessStore } from './types';

/**
 * Evaluatable condition tokens shared by `wait` (poll until true) and
 * `if` (one-shot evaluate at preprocess time). Both surfaces accept
 * the same vocabulary; only the runtime semantics differ.
 *
 * Forms (all relative to the focused symbol unless `[SYMBOL]` is given):
 *   position                  — any non-zero position
 *   long  / short             — side-specific position
 *   price (>|<|>=|<=) <num>   — compare last trade price
 *   pnl   (>|<|>=|<=) <num>   — compare focused-symbol position uPNL
 *   fill                      — no live orders (open/creating/partial)
 *
 * Returns `null` when args don't match any known shape — callers
 * surface that as "Invalid condition" or similar.
 */
/**
 * Optional context passed to every `check` call. Some condition kinds
 * (`@<liveVar>` LHS) need access to the PreprocessStore to look up
 * bindings at check time — they can't bake the value in at parse
 * time, that would defeat the point of a reactive var.
 *
 * Existing condition kinds (price / pnl / fundingrate / fill / …)
 * don't need the store and ignore this arg.
 */
export interface ConditionCheckContext {
  store?: PreprocessStore;
}

export interface ParsedCondition {
  /** Returns true when the condition is satisfied right now. */
  check: (
    exchange: ExchangeLike,
    focusedSymbol: string,
    ctx?: ConditionCheckContext,
  ) => boolean;
  /** Description used in error / log messages. */
  describe: string;
}

/** Order statuses we consider "live" for the `wait fill` family —
 *  i.e. the order is still on the book (or in flight). Done / cancelled
 *  / rejected statuses are NOT live, which is what releases the wait. */
function isLiveOrderStatus(status: unknown): boolean {
  const s = String(status);
  return s === 'open' || s === 'creating' || s === 'updating' || s === 'partial';
}

const COMPARATORS: Record<string, (a: number, b: number) => boolean> = {
  '>': (a, b) => a > b,
  '<': (a, b) => a < b,
  '>=': (a, b) => a >= b,
  '<=': (a, b) => a <= b,
  // Equality on floats — exact-match `==` is rarely what a user
  // wants for prices (the live value will drift in the last decimal
  // place), but it's useful for integer-ish values like `possize`,
  // `fundingRate * 10_000`, or comparing against zero. `!=` is the
  // expected dual. Both are exact compares — users who need
  // tolerance can do `wait price < x+epsilon` instead.
  '==': (a, b) => a === b,
  '!=': (a, b) => a !== b,
};

/**
 * Resolve `@<name>` to a numeric value via the condition-check
 * context. Tracked binding (max/min aggregator) wins, then live
 * (`sets`), then static (`set`) — matches the substitute layer's
 * precedence. Throws if no binding exists so a wait/if against a
 * typo'd var fails loudly on the first check rather than silently
 * looping until timeout.
 */
function resolveAtNameForCondition(
  name: string,
  exchange: ExchangeLike,
  focusedSymbol: string,
  cctx?: ConditionCheckContext,
): number {
  if (!cctx?.store) {
    throw new Error(
      `condition: @${name} needs the PreprocessStore — caller must pass ctx.store`,
    );
  }
  // Tracked first — every condition check is a poll, so the
  // aggregator updates per tick (this is what makes
  // `sets @stop @hi-100; wait pnl < @stop` work as a poor-man's
  // trailing stop).
  const trackedEntry = cctx.store.getTrackedVar(name);
  if (trackedEntry) {
    const pctx: PreprocessContext = {
      exchange,
      symbol: focusedSymbol,
      store: cctx.store,
    };
    return resolveTrackedVar(name, trackedEntry, pctx, new Set());
  }
  const liveSource = cctx.store.getLiveVar(name);
  if (liveSource !== undefined) {
    const pctx: PreprocessContext = {
      exchange,
      symbol: focusedSymbol,
      store: cctx.store,
    };
    return evaluateLiveVarSource(name, liveSource, pctx);
  }
  const staticVal = cctx.store.getVar(name);
  if (staticVal !== undefined) {
    const n = parseFloat(staticVal);
    if (!Number.isFinite(n)) {
      throw new Error(
        `condition: @${name}='${staticVal}' is not numeric`,
      );
    }
    return n;
  }
  throw new Error(`condition: @${name} is not set`);
}

/**
 * Parse a condition's RHS — either a numeric literal or an `@<name>`
 * reference. The @-form lets users compute the threshold separately
 * (`track max @hi $pnl; sets @stop @hi-1`) and reference it bare on
 * the right side of a comparison (`wait pnl < @stop`). Each poll
 * re-resolves the @-target through the same precedence as the LHS
 * (tracker → live → static), so trackers ratchet correctly.
 *
 * Returns null for shapes we don't recognise (math tokens, garbage,
 * etc.). The condition parser falls through to null in that case,
 * which surfaces as "Invalid condition" upstream.
 */
type RhsTarget =
  | { kind: 'literal'; value: number }
  | { kind: 'var'; name: string };

function parseRhsTarget(arg: string): RhsTarget | null {
  if (!arg) return null;
  // Numeric literal — accept signed decimals only. parseFloat is
  // permissive ('123abc' → 123) so we gate it on a strict regex
  // first to keep accidental @-refs from sneaking through.
  if (/^[+-]?(?:\d+(?:\.\d+)?|\.\d+)$/.test(arg)) {
    return { kind: 'literal', value: parseFloat(arg) };
  }
  const m = arg.match(/^@([a-zA-Z][\w]*)$/);
  if (m) return { kind: 'var', name: m[1] };
  return null;
}

function resolveRhsTarget(
  target: RhsTarget,
  exchange: ExchangeLike,
  focusedSymbol: string,
  cctx?: ConditionCheckContext,
): number {
  if (target.kind === 'literal') return target.value;
  return resolveAtNameForCondition(target.name, exchange, focusedSymbol, cctx);
}

function describeRhs(target: RhsTarget): string {
  return target.kind === 'literal' ? String(target.value) : `@${target.name}`;
}

export function parseCondition(args: string[]): ParsedCondition | null {
  const condition = parseConditionBody(args);
  if (!condition || !['position', 'long', 'short', 'anyposition', 'pnl', 'fill'].includes(args[0]?.toLowerCase()))
    return condition;
  return {
    ...condition,
    check: (exchange, symbol, ctx) => {
      exchange.syncAccountState?.();
      return condition.check(exchange, symbol, ctx);
    },
  };
}

function parseConditionBody(args: string[]): ParsedCondition | null {
  if (args.length === 0) return null;
  const head = args[0].toLowerCase();

  // `@<name> <op> <num>` — compare a live/static @-var against a
  // target. The LHS resolves at CHECK time so reactive vars actually
  // tick. The substitute layer is responsible for keeping
  // `@<liveVar>` tokens literal in wait args (substitute would
  // otherwise pre-evaluate the live var and freeze the value here).
  // For `if`, the condition body skips substitution entirely
  // (tryExpandIfElse runs before preprocessSegment), so this just
  // works.
  if (/^@[a-zA-Z][\w]*$/.test(args[0])) {
    const name = args[0].slice(1);
    const op = args[1];
    const rhs = parseRhsTarget(args[2]);
    if (!op || !rhs) return null;
    const cmp = COMPARATORS[op];
    if (!cmp) return null;
    return {
      describe: `@${name} ${op} ${describeRhs(rhs)}`,
      check: (exchange, focusedSymbol, cctx) => {
        const value = resolveAtNameForCondition(
          name,
          exchange,
          focusedSymbol,
          cctx,
        );
        return cmp(value, resolveRhsTarget(rhs, exchange, focusedSymbol, cctx));
      },
    };
  }

  if (head === 'position' || head === 'long' || head === 'short') {
    const sideFilter =
      head === 'long'
        ? PositionSide.Long
        : head === 'short'
          ? PositionSide.Short
          : null;
    const symbolArg = args[1];
    const explicitSymbol = symbolArg ? symbolArg.toUpperCase() : null;
    return {
      describe: `${head}${explicitSymbol ? ` ${explicitSymbol}` : ''}`,
      check: (exchange, focusedSymbol) => {
        const symbol = explicitSymbol ?? focusedSymbol;
        const positions = exchange.store?.positions ?? [];
        return positions.some(
          (p) =>
            p.symbol === symbol &&
            p.contracts !== 0 &&
            (sideFilter === null || p.side === sideFilter),
        );
      },
    };
  }

  // `anyposition` — any non-zero position on ANY symbol. Distinct
  // from `position` (which requires the focused symbol). Useful as
  // a global-scope wait, e.g. when you've fired multi-symbol entries
  // and want to react as soon as anything fills.
  if (head === 'anyposition') {
    return {
      describe: 'anyposition',
      check: (exchange) => {
        const positions = exchange.store?.positions ?? [];
        return positions.some((p) => p.contracts !== 0);
      },
    };
  }

  // Price-family ticker conditions: `price` = last trade, `mark` =
  // mark price, `index` = index price. Same comparator + symbol
  // shape; only the read-field differs. ichibot exposes all three
  // because mark and index can lag last on derivatives by enough
  // that strategies care which one triggers.
  const TICKER_FIELD_BY_HEAD: Record<string, 'last' | 'mark' | 'index'> = {
    price: 'last',
    mark: 'mark',
    index: 'index',
  };
  if (head === 'price' || head === 'pnl' || head === 'mark' || head === 'index') {
    const op = args[1];
    const rhs = parseRhsTarget(args[2]);
    if (!op || !rhs) return null;
    const cmp = COMPARATORS[op];
    if (!cmp) return null;
    const symbolArg = args[3];
    const explicitSymbol = symbolArg ? symbolArg.toUpperCase() : null;

    if (head === 'price' || head === 'mark' || head === 'index') {
      const field = TICKER_FIELD_BY_HEAD[head];
      // `wait price` reads BID when waiting for a downside cross
      // (op is `<` or `<=`) and ASK when waiting for an upside cross
      // (`>` / `>=`). Each side is the closest price to the trigger,
      // so the wait releases sooner and more accurately than reading
      // `last`. Falls back to the head's primary field (`last` for
      // price, `mark` / `index` otherwise) when the bid/ask half
      // isn't loaded. `mark` and `index` use their own field
      // directly — they don't have a bid/ask analog.
      const isUpside = op === '>' || op === '>=';
      const isDownside = op === '<' || op === '<=';
      return {
        describe: `${head} ${op} ${describeRhs(rhs)}${explicitSymbol ? ` ${explicitSymbol}` : ''}`,
        check: (exchange, focusedSymbol, cctx) => {
          const symbol = explicitSymbol ?? focusedSymbol;
          const ticker = (exchange.store?.tickers ?? []).find(
            (t: Ticker) => t.symbol === symbol,
          );
          if (!ticker) return false;
          let value: number | undefined;
          if (head === 'price') {
            if (isUpside && typeof ticker.ask === 'number' && Number.isFinite(ticker.ask)) {
              value = ticker.ask;
            } else if (isDownside && typeof ticker.bid === 'number' && Number.isFinite(ticker.bid)) {
              value = ticker.bid;
            } else {
              value = ticker.last;
            }
          } else {
            value = ticker[field];
          }
          if (typeof value !== 'number' || !Number.isFinite(value)) {
            return false;
          }
          return cmp(value, resolveRhsTarget(rhs, exchange, focusedSymbol, cctx));
        },
      };
    }

    // pnl — sum unrealizedPnl across all positions on the symbol
    return {
      describe: `pnl ${op} ${describeRhs(rhs)}${explicitSymbol ? ` ${explicitSymbol}` : ''}`,
      check: (exchange, focusedSymbol, cctx) => {
        const symbol = explicitSymbol ?? focusedSymbol;
        const positions = (exchange.store?.positions ?? []).filter(
          (p) => p.symbol === symbol && p.contracts !== 0,
        );
        if (positions.length === 0) return false;
        const total = positions.reduce(
          (acc, p) => acc + (p.unrealizedPnl ?? 0),
          0,
        );
        return cmp(total, resolveRhsTarget(rhs, exchange, focusedSymbol, cctx));
      },
    };
  }

  // Ticker-derived conditions that aren't bare price reads.
  //   fundingrate — ticker.fundingRate (rate as a decimal, e.g. 0.0001 = 1bp)
  //   premium     — ticker.mark - ticker.index (zero or absent → false)
  if (head === 'fundingrate' || head === 'premium') {
    const op = args[1];
    const rhs = parseRhsTarget(args[2]);
    if (!op || !rhs) return null;
    const cmp = COMPARATORS[op];
    if (!cmp) return null;
    const symbolArg = args[3];
    const explicitSymbol = symbolArg ? symbolArg.toUpperCase() : null;
    return {
      describe: `${head} ${op} ${describeRhs(rhs)}${explicitSymbol ? ` ${explicitSymbol}` : ''}`,
      check: (exchange, focusedSymbol, cctx) => {
        const symbol = explicitSymbol ?? focusedSymbol;
        const ticker = (exchange.store?.tickers ?? []).find(
          (t: Ticker) => t.symbol === symbol,
        );
        if (!ticker) return false;
        let value: unknown;
        if (head === 'fundingrate') {
          value = ticker.fundingRate;
        } else {
          // premium = mark - index. Both must be finite numbers.
          if (
            typeof ticker.mark !== 'number' ||
            !Number.isFinite(ticker.mark) ||
            typeof ticker.index !== 'number' ||
            !Number.isFinite(ticker.index)
          ) {
            return false;
          }
          value = ticker.mark - ticker.index;
        }
        if (typeof value !== 'number' || !Number.isFinite(value)) {
          return false;
        }
        return cmp(value, resolveRhsTarget(rhs, exchange, focusedSymbol, cctx));
      },
    };
  }

  if (head === 'fill') {
    const arg = args[1];
    return {
      describe: `fill${arg ? ` ${arg}` : ''}`,
      check: (exchange, focusedSymbol) => {
        const orders = exchange.store?.orders ?? [];
        // No arg → focused-symbol scope, today's default.
        if (!arg) {
          return orders.every((o) => o.symbol !== focusedSymbol || !isLiveOrderStatus(o.status));
        }
        // Known symbol takes precedence over ID-shape so an order ID
        // that happens to look like a market can't accidentally
        // hijack the symbol-scope semantic. Matches the cancel
        // command's precedence.
        const upper = arg.toUpperCase();
        const markets = exchange.store?.markets ?? [];
        const knownSymbol = markets.some((m) => m.symbol === upper);
        if (knownSymbol) {
          return orders.every((o) => o.symbol !== upper || !isLiveOrderStatus(o.status));
        }
        // Otherwise treat arg as `<id>` or `<id1>,<id2>,…`. Each
        // piece is matched against order ids by exact equality or
        // trailing-suffix (so `wait fill abc` matches `order-xyz-abc`).
        // Releases when EVERY captured order is no longer live — the
        // expected shape for `@bracket = buy …stop…; wait fill
        // @bracket; close` where the bracket has multi-leg fills/
        // cancels and the user wants the whole trade settled.
        const pieces = arg.split(',').map((p) => p.trim()).filter((p) => p);
        if (pieces.length > 0 && pieces.every((p) => ID_SUFFIX_RE.test(p))) {
          return orders.every((o) => {
            if (!isLiveOrderStatus(o.status)) return true;
            // Live order is OK only if none of the pieces match it.
            return !pieces.some((p) => o.id === p || o.id.endsWith(p));
          });
        }
        // Unknown symbol AND not an ID list — fall back to symbol-
        // scope behavior (matches no orders → releases immediately).
        // Same shape as today, so existing scripts that pass weird
        // tokens don't change behavior.
        return orders.every((o) => o.symbol !== upper || !isLiveOrderStatus(o.status));
      },
    };
  }

  return null;
}

/**
 * Higher-level entry point that accepts a STRING (not pre-tokenised
 * args). Handles two cases:
 *
 *   1. **Combinator form** — `either (<A>) or (<B>) [or (<C>) …]`
 *      releases when ANY sub-condition is true; `both (<A>) and
 *      (<B>) [and (<C>) …]` requires ALL. Sub-bodies are themselves
 *      parsed through `parseConditionText`, so nesting works:
 *      `either (both (long) and (price > 50000)) or (pnl > 200)`.
 *
 *   2. **Simple form** — anything else falls through to
 *      `parseCondition` after whitespace-tokenisation. So
 *      `parseConditionText('price > 50000')` works the same as
 *      `parseCondition(['price', '>', '50000'])`.
 *
 * Returns `null` on a malformed combinator (mismatched parens,
 * wrong keyword between groups, fewer than 2 sub-conditions, an
 * unrecognised sub-condition).
 */
export function parseConditionText(text: string): ParsedCondition | null {
  const trimmed = text.trim();
  const m = trimmed.match(/^(either|both)\s+(.+)$/is);
  if (m) {
    const kw = m[1].toLowerCase() as 'either' | 'both';
    const sep = kw === 'either' ? 'or' : 'and';
    const parts = peelParenGroupsSeparatedBy(m[2], sep);
    if (!parts || parts.length < 2) return null;
    const subs: ParsedCondition[] = [];
    for (const p of parts) {
      const sub = parseConditionText(p);
      if (!sub) return null;
      subs.push(sub);
    }
    const describe = `${kw} ${subs
      .map((s) => `(${s.describe})`)
      .join(` ${sep} `)}`;
    if (kw === 'either') {
      return {
        describe,
        check: (exchange, focusedSymbol, cctx) =>
          subs.some((s) => s.check(exchange, focusedSymbol, cctx)),
      };
    }
    return {
      describe,
      check: (exchange, focusedSymbol, cctx) =>
        subs.every((s) => s.check(exchange, focusedSymbol, cctx)),
    };
  }

  // Simple form — fall back to the pre-tokenised parser.
  const tokens = whitespaceTokenize(trimmed);
  return parseCondition(tokens);
}

/**
 * Walk `input` peeling paren groups separated by the given keyword.
 * Returns the unwrapped bodies, or `null` if the shape doesn't match.
 *
 * Expected shape: `(<group>) <sep> (<group>) [<sep> (<group>) …]`.
 * Whitespace around the parens and the keyword is tolerated.
 */
function peelParenGroupsSeparatedBy(
  input: string,
  sep: 'or' | 'and',
): string[] | null {
  const text = input;
  const sepRe = new RegExp(`^${sep}\\b`, 'i');
  const parts: string[] = [];
  let i = 0;
  while (i < text.length) {
    while (i < text.length && /\s/.test(text[i])) i++;
    if (i >= text.length) break;
    const peeled = peelParenBlock(text.slice(i));
    if (!peeled) return null;
    parts.push(peeled.body);
    i += peeled.consumed;
    while (i < text.length && /\s/.test(text[i])) i++;
    if (i >= text.length) break;
    if (!sepRe.test(text.slice(i))) return null;
    i += sep.length;
    // Must have at least one more group after the separator.
    if (i >= text.length) return null;
  }
  return parts;
}
