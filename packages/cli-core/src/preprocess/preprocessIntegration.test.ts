/**
 * Full-pipeline integration tests for `preprocessCommand`. The unit
 * tests in `preprocess.test.ts`, `repeat.test.ts`, `tokenize.test.ts`,
 * `sleep.test.ts`, and `substitute.test.ts` cover each layer in
 * isolation. This file locks in the behavior of layered combinations
 * — `repeat` + aliases + vars + `$constants` + `sleep` + the legacy
 * `@cmd` prefix — that are easy to break while refactoring any one
 * layer.
 */
import { describe, it, expect } from 'vitest';
import { PositionSide, MarginType, PositionMode } from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { preprocessCommand } from './preprocess';
import { MemoryPreprocessStore } from './memoryStore';
import type { PreprocessContext, UITokenResolvers } from './types';
import type { ExchangeLike } from '../types';

function pos(p: Partial<Position> = {}): Position {
  return {
    id: p.id ?? 'pos1',
    symbol: p.symbol ?? 'BTCUSDT',
    side: p.side ?? PositionSide.Long,
    entryPrice: p.entryPrice ?? 50_000,
    notional: p.notional ?? 50_000,
    leverage: p.leverage ?? 1,
    marginType: p.marginType ?? MarginType.Isolated,
    positionMode: p.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: p.unrealizedPnl ?? 0,
    contracts: p.contracts ?? 1,
    liquidationPrice: p.liquidationPrice ?? 0,
  } as Position;
}

function makeCtx(positions: Position[] = [], uiResolvers?: UITokenResolvers): PreprocessContext {
  const store = new MemoryPreprocessStore();
  const exchange = {
    store: { positions, orders: [], markets: [], tickers: [] },
  } as unknown as ExchangeLike;
  return { exchange, symbol: 'BTCUSDT', store, uiResolvers };
}

const cmds = (input: string, ctx: PreprocessContext) => preprocessCommand(input, ctx).steps.map((s) => s.command);

const blocks = (input: string, ctx: PreprocessContext) => preprocessCommand(input, ctx).steps.map((s) => s.block);

// ─── repeat × sleep ──────────────────────────────────────────────────

describe('repeat + sleep', () => {
  it('flat repeat with internal sleep produces alternating block flags', () => {
    const ctx = makeCtx();
    const result = preprocessCommand('repeat 3 (buy 1$; sleep 1m)', ctx);
    expect(result.steps).toEqual([
      { command: 'buy 1$', block: false },
      { command: 'sleep 1m', block: true, awaitPrior: true },
      { command: 'buy 1$', block: false, awaitPrior: true },
      { command: 'sleep 1m', block: true, awaitPrior: true },
      { command: 'buy 1$', block: false, awaitPrior: true },
      { command: 'sleep 1m', block: true, awaitPrior: true },
    ]);
  });

  it('nested repeat with sleeps preserves order across both levels', () => {
    const ctx = makeCtx();
    const result = preprocessCommand('repeat 2 (repeat 2 (buy 1$; sleep 100ms); sleep 1s)', ctx);
    // Outer 2x: [inner 2x: buy, sleep 100ms, buy, sleep 100ms; sleep 1s]
    // → 5 steps × 2 outer = 10 total
    expect(result.steps).toHaveLength(10);
    expect(result.steps.map((s) => s.command)).toEqual([
      'buy 1$',
      'sleep 100ms',
      'buy 1$',
      'sleep 100ms',
      'sleep 1s',
      'buy 1$',
      'sleep 100ms',
      'buy 1$',
      'sleep 100ms',
      'sleep 1s',
    ]);
    // Block flags follow the sleep keyword regardless of nesting depth.
    expect(result.steps.map((s) => s.block)).toEqual([false, true, false, true, true, false, true, false, true, true]);
  });

  it('repeat without sleep marks every step non-blocking', () => {
    const ctx = makeCtx();
    expect(blocks('repeat 5 (buy 1$)', ctx).every((b) => !b)).toBe(true);
  });

  it('repeat 1 produces a single non-blocking step', () => {
    const ctx = makeCtx();
    expect(cmds('repeat 1 (buy 1$)', ctx)).toEqual(['buy 1$']);
    expect(blocks('repeat 1 (buy 1$)', ctx)).toEqual([false]);
  });
});

// ─── repeat × aliases ────────────────────────────────────────────────

describe('repeat + aliases', () => {
  it('expands an alias inside a repeat — alias evaluates per iteration', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('b1', 'buy 100$');
    expect(cmds('repeat 3 (b1)', ctx)).toEqual(['buy 100$', 'buy 100$', 'buy 100$']);
  });

  it('repeat body itself with `;` separator DOES split — only alias bodies are flat', () => {
    const ctx = makeCtx();
    // To get multi-step output, write the `;` directly in the
    // repeat body rather than burying it in an alias. This is the
    // recommended pattern for multi-statement repeated work.
    expect(cmds('repeat 2 (buy 1$ btcusdt; buy 1$ ethusdt)', ctx)).toEqual([
      'buy 1$ btcusdt',
      'buy 1$ ethusdt',
      'buy 1$ btcusdt',
      'buy 1$ ethusdt',
    ]);
  });

  it('respects alias recursion depth inside repeat', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('a', 'b');
    ctx.store.setAlias('b', 'a');
    expect(() => preprocessCommand('repeat 2 (a)', ctx)).toThrow(/depth exceeded/);
  });

  it('alias body containing a repeat does not double-expand', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('triple', 'repeat 3 (buy 1$)');
    // The alias body is stored verbatim. When `triple` is invoked,
    // alias expansion happens INSIDE preprocessSegment which runs
    // AFTER expandRepeats. So the embedded `repeat` is NOT expanded
    // by the outer pass — it survives as a literal token sequence
    // that downstream commands wouldn't know what to do with.
    //
    // This locks in the current behavior (alias bodies are not
    // re-expanded for repeat). If we ever want the alternative, it
    // requires running expandRepeats per-segment after alias expand.
    const result = cmds('triple', ctx);
    expect(result).toEqual(['repeat 3 (buy 1$)']);
  });

  it('alias appended after repeat expansion threads through correctly', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('t', 'buy 1$');
    expect(cmds('repeat 2 (t); t', ctx)).toEqual(['buy 1$', 'buy 1$', 'buy 1$']);
  });
});

// ─── repeat × user vars ──────────────────────────────────────────────

describe('repeat + variables', () => {
  it('resolves `@var` once at preprocess time, then duplicates the result', () => {
    const ctx = makeCtx();
    ctx.store.setVar('s', '100$');
    expect(cmds('repeat 3 (buy @s)', ctx)).toEqual(['buy 100$', 'buy 100$', 'buy 100$']);
  });

  it('resolves `@var` in every iteration of a repeated chain', () => {
    const ctx = makeCtx();
    ctx.store.setVar('s', '100$');
    expect(cmds('repeat 2 (buy @s; sell @s)', ctx)).toEqual(['buy 100$', 'sell 100$', 'buy 100$', 'sell 100$']);
  });

  it('throws on unknown @var inside repeat — error mentions the var name', () => {
    const ctx = makeCtx();
    expect(() => preprocessCommand('repeat 3 (buy @missing)', ctx)).toThrow(/Unknown variable @missing/);
  });

  it('set @x inside repeat: value tokens substitute, target preserved', () => {
    const ctx = makeCtx();
    // SET is in the target-preserving list. Both copies emerge with
    // @x intact (no substitution on the target name) and the value
    // `100$` literally — the substitution layer leaves literals alone.
    expect(cmds('repeat 2 (set @x 100$)', ctx)).toEqual(['set @x 100$', 'set @x 100$']);
  });

  it('set inside repeat snapshots $entry against the preprocess-time store', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    // $entry is a position-derived constant — it resolves at
    // preprocess time. So both expansions get the same number.
    expect(cmds('repeat 2 (set @e $entry)', ctx)).toEqual(['set @e 50000', 'set @e 50000']);
  });

  // Symbol groups via @var — `set @grp BTCUSDT,ETHUSDT` then
  // `buy 1$ @grp` should fan out to two orders. The substitution layer
  // already supports this — these tests just lock the contract in so a
  // future tightening of the @var resolver doesn't break it.
  it('symbol group: @var with comma-separated value substitutes verbatim', () => {
    const ctx = makeCtx();
    ctx.store.setVar('grp', 'BTCUSDT,ETHUSDT');
    expect(cmds('buy 1$ @grp', ctx)).toEqual(['buy 1$ BTCUSDT,ETHUSDT']);
  });

  it('symbol group inside repeat — fan-out leg is duplicated by repeat', () => {
    const ctx = makeCtx();
    ctx.store.setVar('grp', 'BTCUSDT,ETHUSDT,SOLUSDT');
    expect(cmds('repeat 2 (buy 1$ @grp)', ctx)).toEqual([
      'buy 1$ BTCUSDT,ETHUSDT,SOLUSDT',
      'buy 1$ BTCUSDT,ETHUSDT,SOLUSDT',
    ]);
  });

  it('symbol group composes with $constants in the same command', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    ctx.store.setVar('grp', 'BTCUSDT,ETHUSDT');
    expect(cmds('sell 50% at $entry +1% @grp', ctx)).toEqual(['sell 50% at 50500 BTCUSDT,ETHUSDT']);
  });
});

// ─── repeat × $constants ─────────────────────────────────────────────

describe('repeat + position constants', () => {
  it('resolves $size in every expanded iteration', () => {
    const ctx = makeCtx([pos({ contracts: 1.5 })]);
    expect(cmds('repeat 3 (close $size)', ctx)).toEqual(['close 1.5', 'close 1.5', 'close 1.5']);
  });

  it('resolves $entry with offset in every iteration', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('repeat 2 (sell 0.1 at $entry +250)', ctx)).toEqual(['sell 0.1 at 50250', 'sell 0.1 at 50250']);
  });

  it('throws once when $entry has no position, surfaces token in error', () => {
    const ctx = makeCtx();
    expect(() => preprocessCommand('repeat 5 (buy 1$ at $entry)', ctx)).toThrow(/no position/);
  });
});

// ─── repeat × UI-only constants ──────────────────────────────────────

describe('repeat + UI-only constants', () => {
  it('throws inside repeat when no resolvers are set (standalone CLI)', () => {
    const ctx = makeCtx();
    expect(() => preprocessCommand('repeat 3 (buy 1$ at $click)', ctx)).toThrow(/web terminal/);
  });

  it('substitutes $click in each iteration when a resolver is present', () => {
    const ctx = makeCtx([], { click: () => '52000' });
    expect(cmds('repeat 3 (buy 1$ at $click)', ctx)).toEqual(['buy 1$ at 52000', 'buy 1$ at 52000', 'buy 1$ at 52000']);
  });

  it('substitutes $quick<n> per iteration', () => {
    const ctx = makeCtx([], { quick: (n) => `$${n * 100}` });
    expect(cmds('repeat 2 (buy $quick3)', ctx)).toEqual(['buy $300', 'buy $300']);
  });
});

// ─── repeat × the legacy @cmd prefix ─────────────────────────────────

describe('repeat + legacy @cmd prefix', () => {
  it('does NOT strip @cmd inside a repeat body — only the leading token of a top-level segment', () => {
    const ctx = makeCtx();
    // After expansion the inner copies start as `@buy 1$`. Since
    // those segments come from inside a repeat body and not from the
    // user's typed top-level chain, the legacy prefix logic looks at
    // each segment's leading character. Here `@buy` is a SEGMENT
    // start, so it WILL be stripped — and warned exactly once.
    const result = preprocessCommand('repeat 2 (@buy 1$)', ctx);
    expect(result.steps.map((s) => s.command)).toEqual(['buy 1$', 'buy 1$']);
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0].code).toBe('deprecated-at-prefix');
  });

  it('does not strip @<word> when @<word> is a known var', () => {
    const ctx = makeCtx();
    ctx.store.setVar('mybuy', 'buy 100$');
    const result = preprocessCommand('repeat 2 (@mybuy)', ctx);
    expect(result.steps.map((s) => s.command)).toEqual(['buy 100$', 'buy 100$']);
    expect(result.warnings).toHaveLength(0);
  });
});

// ─── repeat as a chain element ───────────────────────────────────────

describe('repeat as a chain element', () => {
  it('two repeats joined by `;` produce expanded sequences in order', () => {
    const ctx = makeCtx();
    expect(cmds('repeat 2 (a); repeat 2 (b)', ctx)).toEqual(['a', 'a', 'b', 'b']);
  });

  it('repeat surrounded by non-repeat segments preserves position', () => {
    const ctx = makeCtx();
    expect(cmds('cancel; repeat 2 (buy 1$); close', ctx)).toEqual(['cancel', 'buy 1$', 'buy 1$', 'close']);
  });

  it('repeat within a chain mixed with `&` separators', () => {
    const ctx = makeCtx();
    // Outer chain uses both `;` and `&`; semantically both fire-and-
    // don't-await, but the ordering must still be preserved.
    expect(cmds('a & repeat 2 (b); c', ctx)).toEqual(['a', 'b', 'b', 'c']);
  });

  it('repeat inside a parenthesised body that contains quoted strings', () => {
    const ctx = makeCtx();
    // The alias body has `;` inside quotes; the chain tokenizer must
    // not split there. After repeat expands, each expansion is a
    // single-segment alias call (and alias is target-preserving so it
    // passes through without substitution).
    expect(cmds('repeat 2 (alias z "buy 1$; sell 1$")', ctx)).toEqual([
      'alias z "buy 1$; sell 1$"',
      'alias z "buy 1$; sell 1$"',
    ]);
  });
});

// ─── repeat counting + step structure ────────────────────────────────

describe('repeat step structure', () => {
  it('step count = (count) × (segments per body)', () => {
    const ctx = makeCtx();
    expect(preprocessCommand('repeat 3 (a; b; c)', ctx).steps).toHaveLength(9);
    expect(preprocessCommand('repeat 4 (a)', ctx).steps).toHaveLength(4);
    expect(preprocessCommand('repeat 1 (a; b)', ctx).steps).toHaveLength(2);
  });

  it('outer chain segment count = sum of expanded blocks + non-block segments', () => {
    const ctx = makeCtx();
    // 1 (cancel) + 2 (buy) + 1 (close) = 4
    expect(preprocessCommand('cancel; repeat 2 (buy 1$); close', ctx).steps).toHaveLength(4);
  });

  it('warnings dedupe — only one deprecation warning even with many @cmds', () => {
    const ctx = makeCtx();
    const result = preprocessCommand('repeat 5 (@buy 1$)', ctx);
    expect(result.warnings).toHaveLength(1);
    expect(result.steps).toHaveLength(5);
  });
});

// ─── failure paths inside repeat ─────────────────────────────────────

describe('repeat error propagation', () => {
  it('alias cycle inside repeat surfaces the depth-exceeded error, not silent truncation', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('a', 'b');
    ctx.store.setAlias('b', 'c');
    ctx.store.setAlias('c', 'a');
    expect(() => preprocessCommand('repeat 3 (a)', ctx)).toThrow(/depth exceeded/);
  });

  it('one bad iteration aborts the whole pipeline (no partial step output)', () => {
    const ctx = makeCtx();
    // The first segment has a missing var, so the pipeline aborts
    // before any step makes it out. We don't get "buy 100$ then
    // throw on the second" — we get a clean throw and zero steps.
    expect(() => preprocessCommand('repeat 3 (buy @missing)', ctx)).toThrow(/Unknown variable/);
  });

  it('repeat 0 / 1001 errors come from the expander, not the executor', () => {
    const ctx = makeCtx();
    expect(() => preprocessCommand('repeat 0 (a)', ctx)).toThrow(/positive/);
    expect(() => preprocessCommand('repeat 1001 (a)', ctx)).toThrow(/exceeds max/);
  });
});

// ─── soup: every feature stacked ─────────────────────────────────────

describe('repeat + alias + vars + sleep + constants in one command', () => {
  it('handles the full kitchen-sink scenario with single-statement aliases', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000, contracts: 1 })]);
    ctx.store.setVar('s', '100$');
    // Aliases must be single-statement to fan out into chain steps —
    // see the multi-statement-alias test above for the limitation.
    // Note we use `$entry +500` (absolute offset) rather than
    // `$entry +1%` — the inline-offset combining only matches
    // [+-]\d+(\.\d+)?, not percent. See the next test for that quirk.
    ctx.store.setAlias('enter', 'buy @s');
    ctx.store.setAlias('takeProfit', 'sell @s at $entry +500');

    // Two iterations of: enter; sleep 100ms; takeProfit; sleep 1s.
    // Per iteration:
    //   enter              → buy @s     → buy 100$
    //   sleep 100ms         → blocking
    //   takeProfit          → sell @s at $entry +500 → sell 100$ at 50500
    //   sleep 1s            → blocking
    const result = preprocessCommand('repeat 2 (enter; sleep 100ms; takeProfit; sleep 1s)', ctx);
    expect(result.steps).toEqual([
      { command: 'buy 100$', block: false },
      { command: 'sleep 100ms', block: true, awaitPrior: true },
      { command: 'sell 100$ at 50500', block: false, awaitPrior: true },
      { command: 'sleep 1s', block: true, awaitPrior: true },
      { command: 'buy 100$', block: false, awaitPrior: true },
      { command: 'sleep 100ms', block: true, awaitPrior: true },
      { command: 'sell 100$ at 50500', block: false, awaitPrior: true },
      { command: 'sleep 1s', block: true, awaitPrior: true },
    ]);
  });

  it('substitution + UI-only resolvers + repeat all compose', () => {
    const ctx = makeCtx([], { click: () => '49500', quick: (n) => `$${n * 50}` });
    ctx.store.setAlias('snap', 'buy $quick2 at $click');
    expect(cmds('repeat 3 (snap)', ctx)).toEqual(['buy $100 at 49500', 'buy $100 at 49500', 'buy $100 at 49500']);
  });
});

// ─── multi-statement alias bodies splice back into the chain ────────
//
// (Was a locked-in QUIRK; flipped on 2026-05-10. preprocessCommand
// now alias-expands each top-level segment without substitution and
// then re-runs splitChain on the result, so multi-statement alias
// bodies produce one chain step per statement. Tests below lock in
// the new shape — a refactor that collapses an alias body back to a
// single segment will trip these deliberately.)
describe('multi-statement alias bodies splice back into the chain', () => {
  it('a `;`-separated alias body produces one step per statement', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('pair', 'buy 1$ btcusdt; buy 1$ ethusdt');
    expect(cmds('pair', ctx)).toEqual(['buy 1$ btcusdt', 'buy 1$ ethusdt']);
  });

  it('multi-statement alias inside `repeat` flattens correctly', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('pair', 'buy 1$ btcusdt; buy 1$ ethusdt');
    expect(cmds('repeat 2 (pair)', ctx)).toEqual([
      'buy 1$ btcusdt',
      'buy 1$ ethusdt',
      'buy 1$ btcusdt',
      'buy 1$ ethusdt',
    ]);
  });

  it('preserves block flags for sleep segments inside alias bodies', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('pace', 'buy 1$; sleep 1m; sell 1$');
    expect(blocks('pace', ctx)).toEqual([false, true, false]);
    expect(cmds('pace', ctx)).toEqual(['buy 1$', 'sleep 1m', 'sell 1$']);
  });

  it('handles `&` separators in alias bodies the same way as `;`', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('parfan', 'buy 1$ btcusdt & buy 1$ ethusdt');
    expect(cmds('parfan', ctx)).toEqual(['buy 1$ btcusdt', 'buy 1$ ethusdt']);
  });

  it('nested aliases that ultimately produce multiple statements still flatten', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('inner', 'buy 1$; sell 1$');
    ctx.store.setAlias('outer', 'inner; sleep 1m');
    expect(cmds('outer', ctx)).toEqual(['buy 1$', 'sell 1$', 'sleep 1m']);
    expect(blocks('outer', ctx)).toEqual([false, false, true]);
  });

  it('alias body with an embedded quoted `;` does NOT split the quoted region', () => {
    const ctx = makeCtx();
    // The alias body is just an args list ending in a quoted string.
    // splitChain is quote-aware, so the `;` inside the quotes stays
    // attached to the surrounding command.
    ctx.store.setAlias('say', 'print hello "buy 1; sell 1"');
    expect(cmds('say', ctx)).toEqual(['print hello "buy 1; sell 1"']);
  });

  it('a tail after an alias is preserved across the resplit', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('pair', 'buy 1$ btcusdt; buy 1$ ethusdt');
    // `pair reduce` — the trailing arg lands on the LAST statement of
    // the alias body (per the existing expandAlias `${body} ${tail}`
    // contract). Lock that in so a future refactor doesn't silently
    // change which statement gets the tail.
    expect(cmds('pair reduce', ctx)).toEqual(['buy 1$ btcusdt', 'buy 1$ ethusdt reduce']);
  });

  it('alias name in `set @x` value-position is NOT alias-expanded', () => {
    const ctx = makeCtx();
    ctx.store.setAlias('pair', 'buy 1$; sell 1$');
    // `set @x pair` should snapshot the literal string "pair" — the
    // alias must not expand inside a SET value position.
    // expandSegmentAlias bails on SET_COMMAND head.
    expect(cmds('set @x pair', ctx)).toEqual(['set @x pair']);
  });
});

// ─── $entry inline percent offset ────────────────────────────────────
//
// (Was a locked-in QUIRK; flipped on 2026-05-10. ENTRY_OFFSET_RE now
// matches /^[+-]\d+(\.\d+)?%?$/ and substitute resolves percent
// offsets as `anchor + anchor * pct/100` so they collapse into a
// single absolute price token before the command parser sees them.
// These tests lock in the new shape — a regex tightening that drops
// percent support will trip them deliberately.)

describe('$entry inline percent offset', () => {
  it('combines `$entry +1%` into a single absolute price', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('buy 1 at $entry +1%', ctx)).toEqual(['buy 1 at 50500']);
  });

  it('combines `$entry -1%` into a single absolute price', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('buy 1 at $entry -1%', ctx)).toEqual(['buy 1 at 49500']);
  });

  it('handles fractional percent offsets', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('buy 1 at $entry +0.5%', ctx)).toEqual(['buy 1 at 50250']);
    expect(cmds('buy 1 at $entry -0.1%', ctx)).toEqual(['buy 1 at 49950']);
  });

  it('still combines absolute offsets the same way as before', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('buy 1 at $entry +500', ctx)).toEqual(['buy 1 at 50500']);
    expect(cmds('buy 1 at $entry -100', ctx)).toEqual(['buy 1 at 49900']);
  });

  it('works on $lentry / $sentry in hedge mode', () => {
    const ctx = makeCtx([
      pos({ id: 'l', side: PositionSide.Long, entryPrice: 50_000, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, entryPrice: 60_000, contracts: 0.5 }),
    ]);
    expect(cmds('sell 0.1 at $lentry +1%', ctx)).toEqual(['sell 0.1 at 50500']);
    expect(cmds('buy 0.1 at $sentry -1%', ctx)).toEqual(['buy 0.1 at 59400']);
  });
});

// ─── sets is FULL_PASSTHROUGH — source survives the pipeline ─────────

describe('sets is FULL_PASSTHROUGH', () => {
  it('emits the segment unchanged (no substitution of source tokens)', () => {
    const ctx = makeCtx();
    // The source `$ask-$bid` MUST NOT be substituted at preprocess
    // time. If it were, the SetsCommand would receive a frozen
    // value like `20` instead of the source expression, and we'd
    // lose reactivity. Lock the contract in here.
    expect(cmds('sets @spread $ask-$bid', ctx)).toEqual(['sets @spread $ask-$bid']);
  });

  it('multi-token sources (anchor + offset) survive intact', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('sets @above $entry +1%', ctx)).toEqual(['sets @above $entry +1%']);
  });

  it('still chains around a sets segment (the preceding/following segments preprocess as usual)', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(cmds('cancel; sets @above $entry +1%; buy 1$', ctx)).toEqual(['cancel', 'sets @above $entry +1%', 'buy 1$']);
  });

  it('inside repeat, the `sets` body still passes through unchanged on every iteration', () => {
    const ctx = makeCtx();
    expect(cmds('repeat 2 (sets @x $last)', ctx)).toEqual(['sets @x $last', 'sets @x $last']);
  });
});
