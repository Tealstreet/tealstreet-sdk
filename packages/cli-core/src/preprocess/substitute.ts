import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { add, divide, multiply, subtract } from '@tealstreet/safe-cex/src/utils/safe-math';
import { splitChain } from './tokenize';
import { peelParenBlock } from './parserUtils';
import { buildSubstituteOperandResolver, tryEvalMathToken } from './math';
import { evaluateLiveVarSource } from './reactiveVars';
import { resolveTrackedVar } from './trackedVars';
import type { PreprocessContext } from './types';

/**
 * Commands with special substitution rules:
 *   - `set @x VALUE`: preserve `@x` (creation target), substitute VALUE
 *     so `set @x $entry` snapshots the current entry price.
 *   - `sets @x SOURCE`: no substitution at all — SOURCE is stored
 *     verbatim and re-evaluated on every `@x` use, so substituting it
 *     at definition time would freeze the value and break reactivity.
 *   - `unset`, `print`, `alias`, `unalias`: no substitution at all.
 *     The `alias` body is intentionally left dynamic — `alias x "buy
 *     at $entry"` resolves `$entry` at *use* time, not at definition
 *     time.
 */
export const FULL_PASSTHROUGH_COMMANDS = new Set([
  'unset',
  'print',
  'alias',
  'unalias',
  // `sets` (reactive var setter) — source stays literal so the
  // SetsCommand can store the source expression for later evaluation.
  'sets',
  // `track` (max/min aggregator setter) — source stays literal
  // for the same reason as `sets`. Substituting `$pnl` at
  // definition time would freeze the aggregator's source to a
  // numeric snapshot, breaking the per-read re-evaluation that
  // makes the aggregator work.
  'track',
  // `untrack` — `untrack @<name>` should preserve `@<name>` so the
  // UntrackCommand can read the literal arg.
  'untrack',
]);
export const SET_COMMAND = 'set';

/**
 * `when (<condition>) (<action>)` is split-brained on purpose.
 *
 * The CONDITION is substituted now, while the position that gives `$entry` and
 * `$roe101` their meaning is the one the user is looking at. The ACTION is kept
 * literal and re-preprocessed when it fires, because `be +3%` has to read the
 * position as it will be THEN — that is the whole point of arming it.
 */
export const WHEN_COMMAND = 'when';

/** Hard cap on alias-expansion recursion (catches accidental cycles). */
export const MAX_ALIAS_DEPTH = 8;

const POSITION_SIZE_CONSTANTS = new Set(['$size', '$lsize', '$ssize']);
const POSITION_PRICE_CONSTANTS = new Set([
  '$entry',
  '$lentry',
  '$sentry',
  // `$liq` — focused-symbol position's liquidation price. Behaves
  // like `$entry`: position-anchored, supports inline `+N` / `+N%`
  // offsets so `stop sell at $liq +1%` is a clean "1% above liq"
  // trailing stop. Errors when there's no open position on the
  // focused symbol or when the exchange reports liquidationPrice = 0
  // (some venues do that on isolated-margin positions before the
  // engine has computed it).
  '$liq',
]);
// Ticker-anchored price constants — read live from the focused
// symbol's ticker. `$last` / `$mark` / `$index` mirror the read
// commands of the same names; `$bid` / `$ask` give the top of book.
// `$mid` / `$midprice` prefer the exchange-published mid when set
// on `ticker.midPrice` (Hyperliquid's `midPx`, etc.) and fall back
// to a synthesised `(bid + ask) / 2` otherwise.
const TICKER_PRICE_CONSTANTS = new Set(['$bid', '$ask', '$last', '$mark', '$index', '$mid', '$midprice']);
// Anchor tokens that accept an inline `+N` / `-N` / `+N%` / `-N%`
// offset on the next token. Position-anchored ($entry/$lentry/$sentry)
// and ticker-anchored ($bid/$ask/$last/$mark/$index) use the same
// combining rules — see substituteTokens.
/**
 * `$roe<N>` — the price at which the focused position's ROE would be N%.
 *
 * It carries its number in the token (`$roe25`, `$roe-10`, `$roe101`), so
 * unlike every other constant it cannot be a Set member. Rather than special-
 * case it at each call site, the anchor set below overrides `has` — which also
 * gets it into the math resolver, since that takes these as Sets and only ever
 * calls `.has`.
 */
const ROE_TOKEN_RE = /^\$roe[+-]?\d+(\.\d+)?$/i;

export const isRoeToken = (tok: string): boolean => ROE_TOKEN_RE.test(tok);

class PriceAnchorConstants extends Set<string> {
  override has(tok: string): boolean {
    return super.has(tok) || isRoeToken(tok);
  }
}

const PRICE_ANCHOR_CONSTANTS: Set<string> = new PriceAnchorConstants([
  ...POSITION_PRICE_CONSTANTS,
  ...TICKER_PRICE_CONSTANTS,
]);
// Numeric account / PnL constants. Plain numbers — no inline offset
// combining, no math expressions (yet); they substitute as a single
// number token and any further arithmetic is the user's problem.
//
//   $free / $used / $total — balance fields (settlement currency)
//   $upnl                  — exchange-reported unrealized PnL (balance.upnl)
//   $pnl                   — focused position's unrealizedPnl
//   $totalupnl             — sum of unrealizedPnl across all positions
//   $totallongusd / $totalshortusd — sum of notional by side
//   $totallong / $totalshort       — same sums divided by total balance
const ACCOUNT_NUMBER_CONSTANTS = new Set([
  '$free',
  '$used',
  '$total',
  '$upnl',
  '$pnl',
  '$totalupnl',
  '$totallongusd',
  '$totalshortusd',
  '$totallong',
  '$totalshort',
  // `$margin` — focused-symbol position's used margin
  //   (notional / leverage). Useful for sizing reduce/add orders
  //   against a leverage-aware budget.
  '$margin',
  // `$ticksize` — market.precision.price for the focused symbol.
  //   Lets `bump +$ticksize` move an order one tick without the
  //   user looking up the tick.
  // `$minsize` — market.limits.amount.min for the focused symbol.
  //   Useful for "buy the minimum size" patterns when sweeping a
  //   bunch of low-liquidity symbols.
  '$ticksize',
  '$minsize',
]);
// Constants that resolve to a value derived from runtime, not from
// any specific store field. Each substitution re-evaluates them.
const SYNTHESIZED_CONSTANTS = new Set([
  // ichibot's `random` — uniform in [0.35, 1). Fresh on every
  // substitution call so `buy 10*$random, repeat 9` produces nine
  // distinct sizes. The 0.35 floor (not 0) matches ichibot —
  // tiny-size orders below typical exchange minimums are rarely
  // what users want.
  '$random',
]);
// Inline offsets accept absolute (`+100` / `-50.5`) and percent
// (`+1%` / `-1.1%`) forms. Percent resolves at substitution time as
// `anchor + anchor * pct/100`.
const ENTRY_OFFSET_RE = /^[+-]\d+(\.\d+)?%?$/;
const UI_ONLY_CONSTANT_RE = /^\$(click|ordersize|quick([1-7]))$/;

/**
 * Take a single command segment (post-repeat-expansion) and:
 *   1. Expand the leading word as an alias if defined (recursive, capped).
 *   2. Substitute any `$constants` and `@varname` tokens inline.
 *
 * Returns the rewritten segment. Throws on unknown variables, missing
 * positions for `$size`/`$entry`, alias cycles, or UI-only constants
 * (`$click`, `$ordersize`, `$quick<n>` — those resolve in the web app).
 *
 * Special-cased commands: see `FULL_PASSTHROUGH_COMMANDS` and `SET_COMMAND`.
 */
export function preprocessSegment(
  raw: string,
  ctx: PreprocessContext,
  expectedFutureCaptures?: Set<string>,
  deferredCaptureNames?: Set<string>,
): string {
  const trimmed = raw.trim();
  if (!trimmed) return trimmed;

  const tokens = whitespaceTokenize(trimmed);
  const head = tokens[0];

  if (FULL_PASSTHROUGH_COMMANDS.has(head)) {
    return trimmed;
  }

  if (head === SET_COMMAND) {
    // `set @x VALUE...` — preserve the @x target literally, substitute
    // the value tokens so `set @x $entry` snapshots the current price.
    if (tokens.length < 3) return trimmed;
    const target = tokens[1];
    const valueTokens = substituteTokens(
      tokens.slice(2),
      ctx,
      new Set(),
      false,
      expectedFutureCaptures,
      deferredCaptureNames,
    );
    return [head, target, ...valueTokens].join(' ');
  }

  const expanded = expandAlias(trimmed, ctx, 0);
  const expandedTokens = whitespaceTokenize(expanded);
  // `wait` args defer live-var substitution so the poller can
  // re-evaluate each tick. Static @-vars still substitute normally
  // (they're frozen anyway). Checked against the post-alias-expansion
  // head so `alias w "wait …"` then `w` works.
  const expandedHead = expandedTokens[0]?.toLowerCase();
  // Checked AFTER alias expansion, same as `wait` below, so `alias be3 "when
  // (price >= $entry) (be +3%)"` gets the split-brain treatment too. Checked
  // before substitution, because substituting the action is the thing this
  // exists to prevent.
  if (expandedHead === WHEN_COMMAND) {
    return preprocessWhenSegment(expanded, ctx);
  }
  const keepLiveVarsLiteral = expandedHead === 'wait';
  const substituted = substituteTokens(
    expandedTokens,
    ctx,
    new Set(),
    keepLiveVarsLiteral,
    expectedFutureCaptures,
    deferredCaptureNames,
  );
  return substituted.join(' ');
}

function expandAlias(input: string, ctx: PreprocessContext, depth: number): string {
  if (depth > MAX_ALIAS_DEPTH) {
    throw new Error(`alias: expansion depth exceeded — possible cycle`);
  }
  // Peel off the first chain segment so an alias body like
  // `inner; sleep 1m` can recurse cleanly: head extraction must stop at
  // top-level `;` / `&` rather than greedily swallowing them. The rest
  // of the chain is reattached afterward with its original separators.
  const segments = splitChain(input);
  if (segments.length === 0) return input;
  const first = segments[0].command;
  const m = first.match(/^(\S+)(\s+([\s\S]*))?$/);
  if (!m) return input;
  const head = m[1];
  const tail = m[3] ?? '';
  const body = ctx.store.getAlias(head);
  if (body === undefined) return input;
  const newFirst = tail ? `${body} ${tail}` : body;
  const rest = segments.slice(1);
  const tailChain = rest.map((s) => `${s.separator ?? ';'} ${s.command}`).join(' ');
  const next = tailChain ? `${newFirst} ${tailChain}` : newFirst;
  return expandAlias(next, ctx, depth + 1);
}

/**
 * Expand the leading alias of a segment without performing any
 * substitution. Returns the segment unchanged if the head isn't an
 * alias, or if the head is a special-cased command (FULL_PASSTHROUGH
 * / SET) where alias lookup is skipped intentionally.
 *
 * The pipeline uses this to splice multi-statement alias bodies back
 * into the top-level chain — e.g. `alias pair "buy 1$; sell 1$"`
 * followed by `pair` should produce two steps, not one segment with
 * an embedded `;`.
 */
export function expandSegmentAlias(raw: string, ctx: PreprocessContext): string {
  const trimmed = raw.trim();
  if (!trimmed) return trimmed;
  const headMatch = trimmed.match(/^(\S+)/);
  if (!headMatch) return trimmed;
  const head = headMatch[1];
  if (FULL_PASSTHROUGH_COMMANDS.has(head)) return trimmed;
  if (head === SET_COMMAND) return trimmed;
  return expandAlias(trimmed, ctx, 0);
}

/** Whitespace tokenizer that leaves quote characters in place. We don't
 *  strip quotes here — substitution doesn't need to look inside them, and
 *  preserving them lets the downstream `parseArgs` handle quoted values
 *  consistently. */
export function whitespaceTokenize(input: string): string[] {
  const out: string[] = [];
  let buf = '';
  let inQuotes: '"' | "'" | null = null;
  for (let i = 0; i < input.length; i++) {
    const c = input[i];
    if (inQuotes) {
      if (c === inQuotes && input[i - 1] !== '\\') inQuotes = null;
      buf += c;
      continue;
    }
    if (c === '"' || c === "'") {
      inQuotes = c;
      buf += c;
      continue;
    }
    if (c === ' ' || c === '\t') {
      if (buf) {
        out.push(buf);
        buf = '';
      }
      continue;
    }
    buf += c;
  }
  if (buf) out.push(buf);
  return out;
}

/**
 * Substitute every token in the input array. The optional `seen`
 * parameter is the live-var recursion stack — callers from outside
 * the live-var module should omit it. Live-var evaluation threads it
 * through to detect cycles like `sets @a @b; sets @b @a`.
 *
 * `keepLiveVarsLiteral` defers substitution of `@<liveVar>` tokens
 * (still substitutes `@<staticVar>`). Used for `wait` arguments —
 * the wait poller re-evaluates the condition on each tick, so a
 * reactive var as the LHS needs to stay literal long enough to
 * reach the condition parser. Static vars are frozen by design so
 * substituting them now or later produces the same result.
 *
 * `expectedFutureCaptures` lists `@<name>` targets that an EARLIER
 * step in the same chain will `captureTo`. Substituting those at
 * preprocess time would throw "Unknown variable" because the
 * capture writeback hasn't fired yet. The fix is to keep them
 * literal here and let the runner finalize them after the prior
 * step ran. Names that are kept literal are added to
 * `deferredCaptureNames` (output set, mutated by the call) so the
 * step can carry a `deferredSubstitution` field telling the runner
 * which @-tokens to resolve at execution time.
 */
export function substituteTokens(
  tokens: string[],
  ctx: PreprocessContext,
  seen: Set<string> = new Set(),
  keepLiveVarsLiteral = false,
  expectedFutureCaptures?: Set<string>,
  deferredCaptureNames?: Set<string>,
): string[] {
  const out: string[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const tok = tokens[i];

    const uiMatch = UI_ONLY_CONSTANT_RE.exec(tok);
    if (uiMatch) {
      const resolved = resolveUIConstant(uiMatch[1], uiMatch[2], ctx);
      out.push(resolved);
      continue;
    }

    // Price anchor with offset → combine. Absolute offsets (`+100`,
    // `-50.5`) add directly; percent offsets (`+1%`, `-1.1%`) resolve
    // as anchor * (1 + pct/100). Both shapes collapse to a single
    // absolute price token so downstream command parsers see one
    // number, not a price + a stray offset. Applies to $entry-family
    // (position-anchored) and $bid/$ask/$last/$mark/$index
    // (ticker-anchored) the same way.
    if (PRICE_ANCHOR_CONSTANTS.has(tok) && i + 1 < tokens.length && ENTRY_OFFSET_RE.test(tokens[i + 1])) {
      const base = resolvePriceAnchor(tok, ctx);
      const next = tokens[i + 1];
      let offset: number;
      if (next.endsWith('%')) {
        const pct = parseFloat(next.slice(0, -1));
        offset = base * (pct / 100);
      } else {
        offset = parseFloat(next);
      }
      out.push(String(base + offset));
      i++;
      continue;
    }

    if (POSITION_SIZE_CONSTANTS.has(tok)) {
      out.push(String(resolveSizeConstant(tok, ctx)));
      continue;
    }

    if (PRICE_ANCHOR_CONSTANTS.has(tok)) {
      out.push(String(resolvePriceAnchor(tok, ctx)));
      continue;
    }

    if (ACCOUNT_NUMBER_CONSTANTS.has(tok)) {
      out.push(String(resolveAccountNumber(tok, ctx)));
      continue;
    }

    if (SYNTHESIZED_CONSTANTS.has(tok)) {
      out.push(String(resolveSynthesizedConstant(tok)));
      continue;
    }

    // Within-token math (`1*@qtymul`, `$entry-100`, `$ask-$bid`, …).
    // Detected by shape — only fires when the token has at least one
    // arithmetic operator between two operand-shaped chunks. Pure
    // single-operand tokens like `$entry`, `@qtymul`, `100$` reach
    // the branches below unchanged. The `seen` set is threaded so a
    // math token like `@a*2` where `@a` is a live var participates in
    // cycle detection just like a bare `@a` would.
    const mathResolver = buildSubstituteOperandResolver(
      ctx,
      (t) => resolveSizeConstant(t, ctx),
      (t) => resolvePriceAnchor(t, ctx),
      (t) => resolveAccountNumber(t, ctx),
      (t) => resolveSynthesizedConstant(t),
      POSITION_SIZE_CONSTANTS,
      PRICE_ANCHOR_CONSTANTS,
      ACCOUNT_NUMBER_CONSTANTS,
      SYNTHESIZED_CONSTANTS,
      seen,
      expectedFutureCaptures,
    );
    const mathResult = tryEvalMathToken(tok, mathResolver);
    if (mathResult !== null) {
      out.push(mathResult);
      continue;
    }

    if (tok.startsWith('@') && tok.length > 1) {
      const name = tok.slice(1);
      // Pending-capture defer — an earlier step in this chain will
      // `captureTo: name`. Substituting now would throw "Unknown
      // variable" because the writeback hasn't fired yet. Leave the
      // token literal and record the name; the runner substitutes
      // it after the prior step's capture writes back. Checked
      // BEFORE the tracked / live / static lookups so a name that
      // happens to be both a pending capture target AND a current
      // binding falls into the deferred path (the storage mutex
      // makes the AND case rare; this is the safer precedence).
      if (expectedFutureCaptures?.has(name)) {
        out.push(tok);
        deferredCaptureNames?.add(name);
        continue;
      }
      // Tracked var binding (max/min aggregator) — checked FIRST of
      // the three regular kinds. The resolver re-evaluates the
      // source and updates the stored extreme as a side-effect of
      // every lookup. Three-way storage mutex (set / sets / track)
      // guarantees this name isn't ALSO a live or static binding.
      const trackedEntry = ctx.store.getTrackedVar(name);
      if (trackedEntry) {
        // wait args: defer so the poller's per-tick lookups
        // continue updating the high/low-water mark instead of
        // freezing it at chain-preprocess time.
        if (keepLiveVarsLiteral) {
          out.push(tok);
          continue;
        }
        const value = resolveTrackedVar(name, trackedEntry, ctx, seen);
        out.push(String(value));
        continue;
      }
      // Live var binding takes precedence over static — re-evaluates
      // against current ctx on every substitution. Mutual exclusion
      // with static vars at the storage layer guarantees we never
      // see both a live and a static binding for the same name.
      const liveSource = ctx.store.getLiveVar(name);
      if (liveSource !== undefined) {
        // `keepLiveVarsLiteral` (set when preprocessing `wait` args)
        // skips substitution here so the live binding reaches the
        // wait poller and re-evaluates per tick.
        if (keepLiveVarsLiteral) {
          out.push(tok);
          continue;
        }
        const value = evaluateLiveVarSource(name, liveSource, ctx, seen);
        out.push(String(value));
        continue;
      }
      const val = ctx.store.getVar(name);
      if (val === undefined) {
        throw new Error(`Unknown variable @${name}`);
      }
      out.push(val);
      continue;
    }

    out.push(tok);
  }
  return out;
}

/**
 * Resolve a UI-only constant — `$click`, `$ordersize`, or
 * `$quick<n>` — through `ctx.uiResolvers`. Falls back to throwing
 * "only available in the web terminal" when no resolver is present
 * (the standalone CLI's path) or when the resolver returns
 * `undefined` (the value isn't available right now).
 */
function resolveUIConstant(base: string, quickN: string | undefined, ctx: PreprocessContext): string {
  const resolvers = ctx.uiResolvers;
  if (!resolvers) {
    throw new Error(`$${base} is only available in the web terminal — use an absolute value`);
  }

  if (base === 'click') {
    const v = resolvers.click?.();
    if (v === undefined || v === '') {
      throw new Error('$click: no chart click recorded yet');
    }
    return v;
  }
  if (base === 'ordersize') {
    const v = resolvers.orderSize?.();
    if (v === undefined || v === '') {
      throw new Error('$ordersize: no size in the linked order form');
    }
    return v;
  }
  // `$quick<n>` (n parsed by the regex)
  const n = parseInt(quickN ?? '', 10);
  if (!Number.isFinite(n) || n < 1 || n > 7) {
    throw new Error(`$quick: index must be 1..7 (got ${quickN})`);
  }
  const v = resolvers.quick?.(n);
  if (v === undefined || v === '') {
    throw new Error(`$quick${n}: no value configured for slot ${n}`);
  }
  return v;
}

function resolveSizeConstant(tok: string, ctx: PreprocessContext): number {
  ctx.exchange?.syncAccountState?.();
  if (!ctx.exchange) {
    throw new Error(`${tok}: not connected to an exchange`);
  }
  const positions = (ctx.exchange.store?.positions ?? []).filter((p) => p.symbol === ctx.symbol && p.contracts !== 0);
  if (tok === '$lsize') {
    const long = positions.find((p) => p.side === PositionSide.Long);
    if (!long) throw new Error(`$lsize: no long position for ${ctx.symbol}`);
    return Math.abs(long.contracts);
  }
  if (tok === '$ssize') {
    const short = positions.find((p) => p.side === PositionSide.Short);
    if (!short) throw new Error(`$ssize: no short position for ${ctx.symbol}`);
    return Math.abs(short.contracts);
  }
  if (positions.length === 0) {
    throw new Error(`$size: no position for ${ctx.symbol}`);
  }
  return positions.reduce((acc, p) => acc + Math.abs(p.contracts), 0);
}

function resolvePriceAnchor(tok: string, ctx: PreprocessContext): number {
  if (!TICKER_PRICE_CONSTANTS.has(tok)) ctx.exchange?.syncAccountState?.();
  if (isRoeToken(tok)) {
    return resolveRoePrice(tok, ctx);
  }
  if (TICKER_PRICE_CONSTANTS.has(tok)) {
    return resolveTickerPriceConstant(tok, ctx);
  }
  return resolvePositionPriceConstant(tok, ctx);
}

/**
 * Inverts safe-cex's own `calculateReturns`, so a target typed here and a
 * figure read off the positions table mean the same thing:
 *
 *     roe = (upnl / (entry * contracts)) * leverage * 100
 *
 * and for a long, `upnl` at price P is `(P - entry) * contracts`, so the
 * contracts cancel and P = entry * (1 + roe / (leverage * 100)). A short moves
 * the other way.
 *
 * Leverage is refused rather than defaulted when absent: a 0 would report every
 * target as the entry price, which is the same trap `withKnownReturns` documents
 * for the positions table.
 */
function resolveRoePrice(tok: string, ctx: PreprocessContext): number {
  if (!ctx.exchange) {
    throw new Error(`${tok}: not connected to an exchange`);
  }
  const positions = (ctx.exchange.store?.positions ?? []).filter((p) => p.symbol === ctx.symbol && p.contracts !== 0);
  if (positions.length === 0) {
    throw new Error(`${tok}: no position for ${ctx.symbol}`);
  }
  if (positions.length > 1) {
    throw new Error(`${tok}: multiple positions on ${ctx.symbol}, ambiguous`);
  }

  const position = positions[0];

  // REFUSED on inverse, restored after being wrongly removed.
  //
  // It was removed on the grounds that `Position.contracts` is normalised to
  // base, so `entryPrice * contracts` is a valid notional either way. That is
  // false: every inverse mapper derives `contracts` from the MARK, not the
  // entry (`bybitinverse.exchange.ts`, `bitgetinverse`, `okx` all
  // compute `notionalUSD / (mark || last)`), so the divisor drifts with the
  // market and the identity only holds at `mark == entry`. On top of that,
  // bybitinverse and bitgetinverse leave `unrealizedPnl` BASE-denominated,
  // so the numerator is in the wrong currency as well.
  //
  // A wrong price on a trading command is worse than a refusal.
  const market = ctx.exchange.store?.markets?.find((m) => m.symbol === ctx.symbol);
  if (market?.isInverse) {
    throw new Error(`${tok}: ROE targets are not supported on inverse markets — use a price.`);
  }

  const leverage = position.leverage;
  if (!leverage || !Number.isFinite(leverage)) {
    throw new Error(`${tok}: leverage not available on ${ctx.symbol}, cannot convert ROE to a price`);
  }

  const roe = parseFloat(tok.slice('$roe'.length));
  if (!Number.isFinite(roe)) {
    throw new Error(`${tok}: not a usable ROE percentage`);
  }

  // safe-math throughout: `50000 * (1 + 25/1000)` is 51249.99999999999 in
  // plain floating point, and a price token is read by command parsers.
  const move = divide(roe, multiply(leverage, 100));
  const factor = position.side === PositionSide.Long ? add(1, move) : subtract(1, move);
  return multiply(position.entryPrice, factor);
}

function resolvePositionPriceConstant(tok: string, ctx: PreprocessContext): number {
  if (!ctx.exchange) {
    throw new Error(`${tok}: not connected to an exchange`);
  }
  const positions = (ctx.exchange.store?.positions ?? []).filter((p) => p.symbol === ctx.symbol && p.contracts !== 0);
  if (tok === '$lentry') {
    const long = positions.find((p) => p.side === PositionSide.Long);
    if (!long) throw new Error(`$lentry: no long position for ${ctx.symbol}`);
    return long.entryPrice;
  }
  if (tok === '$sentry') {
    const short = positions.find((p) => p.side === PositionSide.Short);
    if (!short) throw new Error(`$sentry: no short position for ${ctx.symbol}`);
    return short.entryPrice;
  }
  if (tok === '$liq') {
    if (positions.length === 0) {
      throw new Error(`$liq: no position for ${ctx.symbol}`);
    }
    if (positions.length > 1) {
      throw new Error(`$liq: multiple positions on ${ctx.symbol}, ambiguous`);
    }
    const liq = positions[0].liquidationPrice;
    if (!Number.isFinite(liq) || liq <= 0) {
      throw new Error(`$liq: liquidation price not available on ${ctx.symbol}`);
    }
    return liq;
  }
  if (positions.length === 0) {
    throw new Error(`$entry: no position for ${ctx.symbol}`);
  }
  if (positions.length > 1) {
    throw new Error(`$entry: multiple positions on ${ctx.symbol}, use $lentry or $sentry`);
  }
  return positions[0].entryPrice;
}

const TICKER_FIELD_BY_TOKEN: Record<string, 'bid' | 'ask' | 'last' | 'mark' | 'index'> = {
  $bid: 'bid',
  $ask: 'ask',
  $last: 'last',
  $mark: 'mark',
  $index: 'index',
};

function resolveTickerPriceConstant(tok: string, ctx: PreprocessContext): number {
  if (!ctx.exchange) {
    throw new Error(`${tok}: not connected to an exchange`);
  }
  const ticker = (ctx.exchange.store?.tickers ?? []).find((t) => t.symbol === ctx.symbol);
  if (!ticker) {
    throw new Error(`${tok}: no ticker loaded for ${ctx.symbol}`);
  }
  // `$mid` / `$midprice` prefer the exchange-published mid when set
  // — Hyperliquid's `midPx` lives on `ticker.midPrice` after the
  // safe-cex mapping, and is meaningfully different from
  // `(bid+ask)/2` because Hyperliquid's bid/ask come from impact
  // prices, not L1 top-of-book. Other exchanges leave `midPrice`
  // unset; we fall back to the synthesised midpoint.
  if (tok === '$mid' || tok === '$midprice') {
    const published = (ticker as unknown as Record<string, unknown>).midPrice;
    if (typeof published === 'number' && Number.isFinite(published)) {
      return published;
    }
    const bid = ticker.bid;
    const ask = ticker.ask;
    if (typeof bid !== 'number' || !Number.isFinite(bid)) {
      throw new Error(`${tok}: bid not available on ${ctx.symbol} ticker`);
    }
    if (typeof ask !== 'number' || !Number.isFinite(ask)) {
      throw new Error(`${tok}: ask not available on ${ctx.symbol} ticker`);
    }
    return (bid + ask) / 2;
  }
  const field = TICKER_FIELD_BY_TOKEN[tok];
  // Defensive — TICKER_PRICE_CONSTANTS gates this branch, so the
  // mapping always hits.
  if (!field) {
    throw new Error(`${tok}: unsupported ticker constant`);
  }
  const value = (ticker as unknown as Record<string, unknown>)[field];
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`${tok}: ${field} not available on ${ctx.symbol} ticker`);
  }
  return value;
}

const BALANCE_FIELD_BY_TOKEN: Record<string, 'free' | 'used' | 'total' | 'upnl'> = {
  $free: 'free',
  $used: 'used',
  $total: 'total',
  $upnl: 'upnl',
};

function resolveAccountNumber(tok: string, ctx: PreprocessContext): number {
  ctx.exchange?.syncAccountState?.();
  if (!ctx.exchange) {
    throw new Error(`${tok}: not connected to an exchange`);
  }
  const balanceField = BALANCE_FIELD_BY_TOKEN[tok];
  if (balanceField !== undefined) {
    const balance = ctx.exchange.store?.balance;
    if (!balance) {
      throw new Error(`${tok}: no balance loaded yet`);
    }
    const v = (balance as unknown as Record<string, unknown>)[balanceField];
    if (typeof v !== 'number' || !Number.isFinite(v)) {
      throw new Error(`${tok}: balance.${balanceField} not available`);
    }
    return v;
  }

  const positions = (ctx.exchange.store?.positions ?? []).filter((p) => p.contracts !== 0);

  if (tok === '$pnl') {
    const focused = positions.filter((p) => p.symbol === ctx.symbol);
    if (focused.length === 0) {
      throw new Error(`$pnl: no position for ${ctx.symbol}`);
    }
    return focused.reduce((acc, p) => acc + (p.unrealizedPnl ?? 0), 0);
  }

  if (tok === '$totalupnl') {
    return positions.reduce((acc, p) => acc + (p.unrealizedPnl ?? 0), 0);
  }

  if (tok === '$totallongusd' || tok === '$totalshortusd') {
    const wantSide = tok === '$totallongusd' ? PositionSide.Long : PositionSide.Short;
    return positions.filter((p) => p.side === wantSide).reduce((acc, p) => acc + Math.abs(p.notional ?? 0), 0);
  }

  if (tok === '$totallong' || tok === '$totalshort') {
    const wantSide = tok === '$totallong' ? PositionSide.Long : PositionSide.Short;
    const notional = positions
      .filter((p) => p.side === wantSide)
      .reduce((acc, p) => acc + Math.abs(p.notional ?? 0), 0);
    const total = ctx.exchange.store?.balance?.total;
    if (typeof total !== 'number' || !Number.isFinite(total) || total === 0) {
      throw new Error(`${tok}: account total balance not available`);
    }
    return notional / total;
  }

  if (tok === '$margin') {
    const focused = positions.filter((p) => p.symbol === ctx.symbol);
    if (focused.length === 0) {
      throw new Error(`$margin: no position for ${ctx.symbol}`);
    }
    return focused.reduce((acc, p) => {
      const lev = Number.isFinite(p.leverage) && p.leverage > 0 ? p.leverage : 1;
      return acc + Math.abs(p.notional ?? 0) / lev;
    }, 0);
  }

  if (tok === '$ticksize' || tok === '$minsize') {
    const market = (ctx.exchange.store?.markets ?? []).find((m) => m.symbol === ctx.symbol);
    if (!market) {
      throw new Error(`${tok}: market not loaded for ${ctx.symbol}`);
    }
    if (tok === '$ticksize') {
      const tick = market.precision?.price;
      if (typeof tick !== 'number' || !Number.isFinite(tick) || tick <= 0) {
        throw new Error(`$ticksize: not available on ${ctx.symbol}`);
      }
      return tick;
    }
    // $minsize — prefer the explicit min from limits.amount.min,
    // fall back to precision.amount when an exchange doesn't expose
    // a separate min (some treat the step as the min).
    const min = market.limits?.amount?.min ?? market.precision?.amount;
    if (typeof min !== 'number' || !Number.isFinite(min) || min <= 0) {
      throw new Error(`$minsize: not available on ${ctx.symbol}`);
    }
    return min;
  }

  // Defensive — gated by ACCOUNT_NUMBER_CONSTANTS.
  throw new Error(`${tok}: unsupported account constant`);
}

/** Runtime-derived constants — not tied to exchange state. Currently
 *  just `$random`. The resolver is pure-by-token: callers pass the
 *  $name and get a number. Re-resolved on every substitution so
 *  `buy 10*$random, repeat 9` produces nine distinct sizes. */
function resolveSynthesizedConstant(tok: string): number {
  if (tok === '$random') {
    // Uniform in [0.35, 1). Floor matches ichibot's `random` (avoids
    // tiny sizes that would round to zero on most exchanges).
    return 0.35 + Math.random() * 0.65;
  }
  // Defensive — gated by SYNTHESIZED_CONSTANTS.
  throw new Error(`${tok}: unsupported synthesized constant`);
}

/**
 * Finalize forward-reference capture tokens (`@<name>`) in a command
 * string AFTER the capturing step has written back to the store. Every
 * `@<name>` token is replaced with the stored value; the trailing
 * `(?![A-Za-z0-9_])` boundary keeps `@o1` from matching `@o10`.
 *
 * Capture vars only ever hold a comma-joined order-ID list (the
 * preprocess layer rejects them as numeric/math operands), so a name
 * that resolves to "" means the capturing command produced ZERO order
 * IDs. Those names are returned in `emptyCaptures` so the runner can
 * **fail closed** rather than let the token collapse to "". This is
 * load-bearing: `@o1 = buy …; cancel @o1` where the buy returned no
 * IDs must not silently become a bare `cancel` — which CancelCommand
 * treats as "cancel every order on the focused symbol" (TSBUG-103).
 *
 * Used by both the standalone CLI runner (`apps/cli/src/repl.ts`) and
 * the web chain runner (`apps/web/.../CLI.tsx`); keep them on this
 * shared helper so the safety behavior can't drift between the two.
 */
export function resolveDeferredCaptures(
  command: string,
  names: readonly string[],
  getVar: (name: string) => string | undefined,
): { command: string; emptyCaptures: string[] } {
  let result = command;
  const emptyCaptures: string[] = [];
  for (const name of names) {
    const value = getVar(name) ?? '';
    if (value === '') emptyCaptures.push(name);
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const re = new RegExp(`@${escaped}(?![A-Za-z0-9_])`, 'g');
    result = result.replace(re, value);
  }
  return { command: result, emptyCaptures };
}


/**
 * Substitute only the condition half of a `when` form. See `WHEN_COMMAND`.
 *
 * A malformed form is returned untouched rather than thrown on, so the error
 * the user sees is `WhenCommand`'s — which can name both parens — instead of a
 * preprocess-layer one that cannot.
 */
function preprocessWhenSegment(trimmed: string, ctx: PreprocessContext): string {
  const afterHead = trimmed.slice(WHEN_COMMAND.length);
  if (!/^\s/.test(afterHead) && afterHead.length > 0) return trimmed;
  const rest = afterHead.trimStart();

  const cond = peelParenBlock(rest);
  if (!cond) return trimmed;
  const afterCond = rest.slice(cond.consumed);
  const action = peelParenBlock(afterCond.trimStart());
  if (!action) return trimmed;

  const condTokens = whitespaceTokenize(cond.body.trim());
  // `keepLiveVarsLiteral`, same as `wait`. A condition is re-CHECKED on every
  // poll, and `parseConditionText` resolves an `@x` LHS at check time, so
  // freezing a live var here would make `when (@hi >= 100)` compare against a
  // snapshot taken when it was armed. Static `@vars` are frozen anyway.
  const substituted = substituteTokens(condTokens, ctx, new Set(), true);
  return `${WHEN_COMMAND} (${substituted.join(' ')}) (${action.body})`;
}
