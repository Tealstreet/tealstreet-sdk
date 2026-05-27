import { describe, it, expect } from 'vitest';
import { PositionSide, MarginType, PositionMode } from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { preprocessCommand } from './preprocess';
import { MemoryPreprocessStore } from './memoryStore';
import type { PreprocessContext } from './types';
import type { ExchangeLike } from '../types';

function pos(partial: Partial<Position>): Position {
  return {
    id: partial.id ?? 'pos1',
    symbol: partial.symbol ?? 'BTCUSDT',
    side: partial.side ?? PositionSide.Long,
    entryPrice: partial.entryPrice ?? 50_000,
    notional: partial.notional ?? 50_000,
    leverage: partial.leverage ?? 1,
    marginType: partial.marginType ?? MarginType.Isolated,
    positionMode: partial.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: partial.unrealizedPnl ?? 0,
    contracts: partial.contracts ?? 1,
    liquidationPrice: partial.liquidationPrice ?? 0,
  } as Position;
}

function makeCtx(positions: Position[] = []): PreprocessContext {
  const store = new MemoryPreprocessStore();
  const exchange = {
    store: { positions, orders: [], markets: [], tickers: [] },
  } as unknown as ExchangeLike;
  return { exchange, symbol: 'BTCUSDT', store };
}

describe('preprocessCommand', () => {
  describe('plain commands', () => {
    it('returns a single step for a single command', () => {
      const result = preprocessCommand('buy 100$', makeCtx());
      expect(result.steps).toEqual([{ command: 'buy 100$', block: false }]);
      expect(result.warnings).toEqual([]);
    });

    it('returns empty steps for whitespace-only input', () => {
      const result = preprocessCommand('   ', makeCtx());
      expect(result.steps).toEqual([]);
    });
  });

  describe('chaining', () => {
    it('splits on `;`', () => {
      const result = preprocessCommand('buy 1; sell 2', makeCtx());
      expect(result.steps.map((s) => s.command)).toEqual(['buy 1', 'sell 2']);
    });

    it('splits on `&`', () => {
      const result = preprocessCommand('buy 1 & sell 2', makeCtx());
      expect(result.steps.map((s) => s.command)).toEqual(['buy 1', 'sell 2']);
    });

    it('mixes ; and &', () => {
      const result = preprocessCommand('a; b & c', makeCtx());
      expect(result.steps.map((s) => s.command)).toEqual(['a', 'b', 'c']);
    });
  });

  describe('repeat', () => {
    it('expands repeat blocks then splits', () => {
      const result = preprocessCommand('repeat 3 (buy 1)', makeCtx());
      expect(result.steps.map((s) => s.command)).toEqual(['buy 1', 'buy 1', 'buy 1']);
    });

    it('preserves sleep block flag inside a repeat', () => {
      const result = preprocessCommand('repeat 2 (buy 1; sleep 1m)', makeCtx());
      expect(result.steps).toEqual([
        { command: 'buy 1', block: false },
        { command: 'sleep 1m', block: true, awaitPrior: true },
        { command: 'buy 1', block: false, awaitPrior: true },
        { command: 'sleep 1m', block: true, awaitPrior: true },
      ]);
    });
  });

  describe('substitution + chaining together', () => {
    it('applies substitution to each segment', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      ctx.store.setVar('size', '100$');
      const result = preprocessCommand('buy @size; sell @size at $entry', ctx);
      expect(result.steps.map((s) => s.command)).toEqual(['buy 100$', 'sell 100$ at 50000']);
    });

    it('sleep is detected after substitution', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('pause', 'sleep 5s');
      const result = preprocessCommand('pause; buy 1', ctx);
      expect(result.steps).toEqual([
        { command: 'sleep 5s', block: true },
        { command: 'buy 1', block: false, awaitPrior: true },
      ]);
    });
  });

  describe('legacy @cmd prefix', () => {
    it('strips the prefix and warns once', () => {
      const result = preprocessCommand('@buy 1; @sell 1', makeCtx());
      expect(result.steps.map((s) => s.command)).toEqual(['buy 1', 'sell 1']);
      expect(result.warnings).toEqual([
        {
          code: 'deprecated-at-prefix',
          message: "the '@cmd' parallel prefix is deprecated — use '&' between commands instead",
        },
      ]);
    });

    it('does not strip when @<word> is a known var', () => {
      const ctx = makeCtx();
      ctx.store.setVar('mybuy', 'buy 100$');
      const result = preprocessCommand('@mybuy', ctx);
      expect(result.steps.map((s) => s.command)).toEqual(['buy 100$']);
      expect(result.warnings).toEqual([]);
    });
  });

  describe('errors propagate', () => {
    it('throws on unknown var', () => {
      expect(() => preprocessCommand('buy @missing', makeCtx())).toThrow(/Unknown variable/);
    });

    it('throws on UI-only constants', () => {
      expect(() => preprocessCommand('buy 1 at $click', makeCtx())).toThrow(/web terminal/);
    });

    it('throws on $entry without a position', () => {
      expect(() => preprocessCommand('buy 1 at $entry', makeCtx())).toThrow(/no position/);
    });
  });

  describe('retry prefix', () => {
    it('attaches `retry: N` to the step and strips the prefix', () => {
      const r = preprocessCommand('retry 3 buy 100$', makeCtx());
      expect(r.steps).toEqual([{ command: 'buy 100$', block: false, retry: 3 }]);
    });

    it('only applies to the segment with the prefix, not subsequent steps', () => {
      const r = preprocessCommand('retry 2 buy 100$; sell 100$', makeCtx());
      expect(r.steps).toEqual([
        { command: 'buy 100$', block: false, retry: 2 },
        { command: 'sell 100$', block: false, awaitPrior: true },
      ]);
    });

    it('retry 0 (await but do not re-fire) is a valid value', () => {
      const r = preprocessCommand('retry 0 buy 100$', makeCtx());
      expect(r.steps[0].retry).toBe(0);
    });

    it('retry above the cap (>10) falls through — segment runs as a normal step', () => {
      const r = preprocessCommand('retry 99 buy 100$', makeCtx());
      // The segment still substitutes through preprocessSegment; the
      // dispatcher will reject `retry 99 buy 100$` as an unknown
      // command, which is the desired loud failure.
      expect(r.steps[0]).toEqual({
        command: 'retry 99 buy 100$',
        block: false,
      });
      expect(r.steps[0].retry).toBeUndefined();
    });

    it('combines with `set ec` so retry-exhausted steps continue', () => {
      const r = preprocessCommand('set ec; retry 3 buy 100$', makeCtx());
      expect(r.steps).toEqual([
        { command: 'buy 100$', block: false, retry: 3, errorMode: 'continue', awaitPrior: true },
      ]);
    });

    it('retry inside a repeat applies to every iteration', () => {
      const r = preprocessCommand('repeat 2 (retry 1 buy 100$)', makeCtx());
      expect(r.steps).toEqual([
        { command: 'buy 100$', block: false, retry: 1 },
        { command: 'buy 100$', block: false, retry: 1, awaitPrior: true },
      ]);
    });
  });

  describe('set ec (continue-on-error)', () => {
    it('flips the chain flag and drops the marker step', () => {
      const r = preprocessCommand('set ec; buy 100$; sell 100$', makeCtx());
      expect(r.steps).toEqual([
        { command: 'buy 100$', block: false, errorMode: 'continue', awaitPrior: true },
        { command: 'sell 100$', block: false, errorMode: 'continue', awaitPrior: true },
      ]);
    });

    it('only applies to steps AFTER the marker', () => {
      const r = preprocessCommand('buy 100$; set ec; sell 100$', makeCtx());
      expect(r.steps).toEqual([
        { command: 'buy 100$', block: false },
        { command: 'sell 100$', block: false, errorMode: 'continue', awaitPrior: true },
      ]);
    });

    it('case-insensitive: `Set EC` and `SET EC` both flip the flag', () => {
      const r = preprocessCommand('Set EC; buy 100$', makeCtx());
      expect(r.steps[0].errorMode).toBe('continue');
    });

    it('does NOT flip when the third token is anything other than nothing', () => {
      // `set ec something` — falls through to the normal SET handling
      // (not a recognized var name, but preprocess passes it through
      // and the dispatcher rejects). Anyway, the flag must stay off.
      const r = preprocessCommand('set ec extra; buy 100$', makeCtx());
      expect(r.steps[r.steps.length - 1].errorMode).toBeUndefined();
    });

    it('flag does NOT persist across separate preprocessCommand calls', () => {
      const ctx = makeCtx();
      preprocessCommand('set ec', ctx);
      const r = preprocessCommand('buy 100$', ctx);
      expect(r.steps[0].errorMode).toBeUndefined();
    });

    it("skipped marker doesn't count as a step", () => {
      const r = preprocessCommand('set ec', makeCtx());
      expect(r.steps).toEqual([]);
    });
  });

  describe('set ro / set po — chain-wide order modifiers', () => {
    it('`set ro` injects `reduce` into subsequent buy/sell steps', () => {
      const r = preprocessCommand('set ro; buy 1$; sell 2$', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ reduce', 'sell 2$ reduce']);
    });

    it('`set po` injects `po` into subsequent buy/sell steps', () => {
      const r = preprocessCommand('set po; buy 1$ at 49000', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ at 49000 po']);
    });

    it('`set ro; set po` injects both', () => {
      const r = preprocessCommand('set ro; set po; buy 1$ at 49000', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ at 49000 reduce po']);
    });

    it('`unset ro` reverses mid-chain', () => {
      const r = preprocessCommand('set ro; buy 1$; unset ro; buy 2$', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ reduce', 'buy 2$']);
    });

    it('`unset po` reverses mid-chain', () => {
      const r = preprocessCommand('set po; buy 1$ at 49000; unset po; buy 2$ at 49000', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ at 49000 po', 'buy 2$ at 49000']);
    });

    it('does NOT inject into non-order-placing commands', () => {
      const r = preprocessCommand('set ro; cancel; print all', makeCtx());
      // cancel and print stay untouched.
      expect(r.steps.map((s) => s.command)).toEqual(['cancel', 'print all']);
    });

    it('does NOT inject into close/stop (already reduce-only by construction)', () => {
      const r = preprocessCommand('set ro; close BTCUSDT', makeCtx());
      // close stays as-is. The `reduce` would be redundant.
      expect(r.steps.map((s) => s.command)).toEqual(['close BTCUSDT']);
    });

    it('idempotent — does not double-inject when keyword already present', () => {
      const r = preprocessCommand('set ro; buy 1$ reduce', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ reduce']);
    });

    it('idempotent — does not double-inject `po`', () => {
      const r = preprocessCommand('set po; buy 1$ at 49000 po', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ at 49000 po']);
    });

    it('only applies to steps AFTER the marker', () => {
      const r = preprocessCommand('buy 1$; set ro; buy 2$', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$', 'buy 2$ reduce']);
    });

    it('flags do not persist across separate preprocessCommand calls', () => {
      const ctx = makeCtx();
      preprocessCommand('set ro; set po', ctx);
      const r = preprocessCommand('buy 1$', ctx);
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$']);
    });

    it("skipped markers don't count as steps", () => {
      const r = preprocessCommand('set ro; set po; unset ro; unset po', makeCtx());
      expect(r.steps).toEqual([]);
    });

    it('case-insensitive — `Set RO` / `SET PO` both work', () => {
      const r = preprocessCommand('Set RO; SET PO; buy 1$ at 49000', makeCtx());
      expect(r.steps[0].command).toBe('buy 1$ at 49000 reduce po');
    });

    it('extends to scale / swarm / chase / twap heads', () => {
      const ctx = makeCtx();
      ctx.store.setVar('s', '100$');
      const r = preprocessCommand(
        'set ro; scale buy 1$ into 5 from 49000 to 50000; swarm buy 1$ into 5; chase buy 1$; twap buy 1$ into 5 over 10',
        ctx,
      );
      const cmds = r.steps.map((s) => s.command);
      expect(cmds[0]).toMatch(/scale buy 1\$.*reduce/);
      expect(cmds[1]).toMatch(/swarm buy 1\$.*reduce/);
      expect(cmds[2]).toMatch(/chase buy 1\$.*reduce/);
      expect(cmds[3]).toMatch(/twap buy 1\$.*reduce/);
    });

    it('`set po` only injects on buy/sell — scale stays untouched (post-only by default)', () => {
      const r = preprocessCommand('set po; buy 1$ at 49000; scale buy 1$ into 5 from 49000 to 50000', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$ at 49000 po', 'scale buy 1$ into 5 from 49000 to 50000']);
    });
  });

  describe('end (early-terminate chain)', () => {
    it('bare `end` drops every subsequent segment', () => {
      const r = preprocessCommand('buy 1$; end; sell 1$', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$']);
      expect(r.endEarly).toBe(true);
    });

    it('`end` first → zero steps', () => {
      const r = preprocessCommand('end; buy 1$', makeCtx());
      expect(r.steps).toEqual([]);
      expect(r.endEarly).toBe(true);
    });

    it('`end` last → behaves as no-op for emitted steps but still flags endEarly', () => {
      const r = preprocessCommand('buy 1$; end', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$']);
      expect(r.endEarly).toBe(true);
    });

    it('case-insensitive — `END` and `End` work', () => {
      expect(preprocessCommand('buy 1$; END; sell 1$', makeCtx()).steps).toEqual([{ command: 'buy 1$', block: false }]);
      expect(preprocessCommand('buy 1$; End; sell 1$', makeCtx()).steps).toEqual([{ command: 'buy 1$', block: false }]);
    });

    it('the marker itself never emits a step', () => {
      const r = preprocessCommand('end', makeCtx());
      expect(r.steps).toEqual([]);
      expect(r.endEarly).toBe(true);
    });

    it('endEarly is omitted when no `end` fired', () => {
      const r = preprocessCommand('buy 1$', makeCtx());
      expect(r.endEarly).toBeUndefined();
    });

    it('flags do not persist across separate preprocessCommand calls', () => {
      const ctx = makeCtx();
      preprocessCommand('end', ctx);
      const r = preprocessCommand('buy 1$', ctx);
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$']);
      expect(r.endEarly).toBeUndefined();
    });

    it('aliased to `end` — `alias bail end` then `bail; buy 1$` short-circuits', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('bail', 'end');
      const r = preprocessCommand('bail; buy 1$', ctx);
      expect(r.steps).toEqual([]);
      expect(r.endEarly).toBe(true);
    });

    it('inside `repeat` — fires on the first iteration, drops the rest', () => {
      // repeat textually expands to:
      //   buy 1$; end; buy 1$; end; buy 1$; end
      // The first `end` cuts everything after.
      const r = preprocessCommand('repeat 3 (buy 1$; end)', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$']);
      expect(r.endEarly).toBe(true);
    });

    it('retry prefix on `end` is silently discarded', () => {
      const r = preprocessCommand('buy 1$; retry 3 end; sell 1$', makeCtx());
      expect(r.steps.map((s) => s.command)).toEqual(['buy 1$']);
      expect(r.endEarly).toBe(true);
    });

    it('a chain modifier hit BEFORE the end is preserved on earlier steps', () => {
      // `set ec` flipped on, one buy emits with errorMode='continue',
      // then `end` cuts the rest.
      const r = preprocessCommand('set ec; buy 1$; end; sell 1$', makeCtx());
      expect(r.steps).toEqual([{ command: 'buy 1$', block: false, errorMode: 'continue', awaitPrior: true }]);
      expect(r.endEarly).toBe(true);
    });
  });

  describe('@<name> = <body> capture syntax', () => {
    it('strips the prefix and attaches captureTo to the resulting step', () => {
      const r = preprocessCommand('@o1 = buy 1$', makeCtx());
      expect(r.steps).toEqual([{ command: 'buy 1$', block: false, captureTo: 'o1' }]);
    });

    it('tolerates extra whitespace around the `=`', () => {
      // Loose whitespace is the more common shape users will type;
      // tight `@x=buy` is also valid.
      expect(preprocessCommand('@x =buy 1$', makeCtx()).steps).toEqual([
        { command: 'buy 1$', block: false, captureTo: 'x' },
      ]);
      expect(preprocessCommand('@x= buy 1$', makeCtx()).steps).toEqual([
        { command: 'buy 1$', block: false, captureTo: 'x' },
      ]);
      expect(preprocessCommand('@x=buy 1$', makeCtx()).steps).toEqual([
        { command: 'buy 1$', block: false, captureTo: 'x' },
      ]);
    });

    it('captureTo composes with retry — retry prefix peels FIRST, then capture', () => {
      // `retry 2 @o1 = buy 1$` is the natural form for "retry up to
      // 2x and capture the IDs of whichever attempt finally succeeds."
      // The retry prefix is detected first, so the capture-prefix
      // detector sees `@o1 = buy 1$` after retry peel.
      const r = preprocessCommand('retry 2 @o1 = buy 1$', makeCtx());
      expect(r.steps).toEqual([{ command: 'buy 1$', block: false, retry: 2, captureTo: 'o1' }]);
    });

    it('captureTo composes with `set ec`', () => {
      const r = preprocessCommand('set ec; @o1 = buy 1$', makeCtx());
      expect(r.steps).toEqual([
        { command: 'buy 1$', block: false, errorMode: 'continue', captureTo: 'o1', awaitPrior: true },
      ]);
    });

    it('chain context — only the captured segment carries captureTo', () => {
      const r = preprocessCommand('cancel; @o1 = buy 1$; sell 2$', makeCtx());
      expect(r.steps).toEqual([
        { command: 'cancel', block: false },
        { command: 'buy 1$', block: false, captureTo: 'o1', awaitPrior: true },
        { command: 'sell 2$', block: false, awaitPrior: true },
      ]);
    });

    it('does NOT trigger the legacy `@cmd` parallel-prefix warning', () => {
      // The legacy strip should be skipped entirely when the capture
      // prefix matched first — otherwise we'd warn the user that
      // they used the deprecated `@cmd` form.
      const r = preprocessCommand('@o1 = buy 1$', makeCtx());
      expect(r.warnings).toEqual([]);
    });

    it('body still gets full substitution', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      const r = preprocessCommand('@o1 = buy 1$ at $entry', ctx);
      expect(r.steps).toEqual([{ command: 'buy 1$ at 50000', block: false, captureTo: 'o1' }]);
    });

    it('inside `if` — captureTo on the OUTER segment propagates to the chosen branch step', () => {
      const ctx = makeCtx([pos({ contracts: 1 })]);
      const r = preprocessCommand('@o1 = if (position) (buy 1$)', ctx);
      expect(r.steps).toEqual([{ command: 'buy 1$', block: false, captureTo: 'o1' }]);
    });

    it('inside `if` else branch also inherits captureTo', () => {
      const ctx = makeCtx(); // no positions → else branch
      const r = preprocessCommand('@o1 = if (position) (sleep) else (buy 1$)', ctx);
      expect(r.steps).toEqual([{ command: 'buy 1$', block: false, captureTo: 'o1' }]);
    });

    it('alias body expanded into multiple sub-segments — every step gets captureTo (last-write-wins)', () => {
      // Documented limitation: when the body fans out into multiple
      // sub-segments via alias expansion, each carries captureTo so
      // the runner writes the var per-step. Only the last write
      // sticks. Locked here so a refactor doesn't accidentally drop
      // the capture from some steps.
      const ctx = makeCtx();
      ctx.store.setAlias('pair', 'buy 1$; sell 1$');
      const r = preprocessCommand('@o1 = pair', ctx);
      expect(r.steps).toEqual([
        { command: 'buy 1$', block: false, captureTo: 'o1' },
        { command: 'sell 1$', block: false, captureTo: 'o1', awaitPrior: true },
      ]);
    });

    it('a body that does not start with a capturable command still gets captureTo', () => {
      // Preprocess doesn't know which commands actually produce
      // data.orderIds — the runner discovers that at execution time
      // (empty array, warning). Locked in here so the syntax is
      // accepted broadly and the runner is the single arbiter.
      const r = preprocessCommand('@x = cancel', makeCtx());
      expect(r.steps).toEqual([{ command: 'cancel', block: false, captureTo: 'x' }]);
    });

    it('does NOT misfire on `set @x V` (no `=` separator with @-prefix LHS)', () => {
      // `set @x VALUE` is the static-var write — the `@x` is the
      // target, but there's no `=` in the segment. Capture-prefix
      // must not engage.
      const r = preprocessCommand('set @x 100', makeCtx());
      expect(r.steps).toEqual([{ command: 'set @x 100', block: false }]);
      expect(r.steps[0].captureTo).toBeUndefined();
    });

    it('does NOT misfire on a body containing `=` somewhere later', () => {
      // Only the LEADING `@<name> = ` shape triggers. A bare `=`
      // mid-command (e.g. `wait price >= 50000`) is unaffected.
      const ctx = makeCtx();
      const r = preprocessCommand('wait price >= 50000', ctx);
      expect(r.steps[0].captureTo).toBeUndefined();
    });

    it('captureTo does not persist across separate preprocessCommand calls', () => {
      const ctx = makeCtx();
      preprocessCommand('@o1 = buy 1$', ctx);
      const r = preprocessCommand('buy 2$', ctx);
      expect(r.steps[0].captureTo).toBeUndefined();
    });
  });

  describe('deferred substitution — single-chain capture references', () => {
    // The preprocess layer now tracks captures-so-far as it walks
    // segments. A later segment's `@<name>` that matches a prior
    // captureTo target is left LITERAL in the output (so substitution
    // doesn't throw "Unknown variable" before the capture writeback
    // fires) and recorded in step.deferredSubstitution. The runner
    // finalises the substitution after the prior step's writeback.

    it('`@o1 = buy 1$; cancel @o1` leaves @o1 literal in the cancel step', () => {
      const r = preprocessCommand('@o1 = buy 1$; cancel @o1', makeCtx());
      expect(r.steps.length).toBe(2);
      expect(r.steps[0]).toEqual({ command: 'buy 1$', block: false, captureTo: 'o1' });
      expect(r.steps[1].command).toBe('cancel @o1');
      expect(r.steps[1].deferredSubstitution).toEqual(['o1']);
    });

    it('multi-capture chain — each pending name is deferred only where referenced', () => {
      const r = preprocessCommand('@o1 = buy 1$; @o2 = buy 2$; cancel @o1; cancel @o2', makeCtx());
      // Steps 0 and 1 are the captures themselves — no deferred subs.
      expect(r.steps[0].deferredSubstitution).toBeUndefined();
      expect(r.steps[1].deferredSubstitution).toBeUndefined();
      // Step 2: cancel @o1 → defers o1 only (o2 also pending but
      // not referenced).
      expect(r.steps[2].command).toBe('cancel @o1');
      expect(r.steps[2].deferredSubstitution).toEqual(['o1']);
      // Step 3: cancel @o2 → defers o2 only.
      expect(r.steps[3].command).toBe('cancel @o2');
      expect(r.steps[3].deferredSubstitution).toEqual(['o2']);
    });

    it('@-refs to names NOT yet captured still throw "Unknown variable"', () => {
      // The pending-captures set is name-scoped. A typo in the
      // reference shouldn't silently defer — the user gets the same
      // error they'd get without any captures in the chain.
      const ctx = makeCtx();
      expect(() => preprocessCommand('@o1 = buy 1$; echo @typo', ctx)).toThrow(/Unknown variable @typo/);
    });

    it('@-refs BEFORE the capture still throw (forward-only reference)', () => {
      // `cancel @o1; @o1 = buy 1$` — the cancel comes first, so @o1
      // isn't yet pending when its segment is preprocessed. Locked
      // here so the captures-so-far semantic is unambiguous.
      const ctx = makeCtx();
      expect(() => preprocessCommand('cancel @o1; @o1 = buy 1$', ctx)).toThrow(/Unknown variable @o1/);
    });

    it('math operand referencing a pending capture rejects with a clear error', () => {
      // Capture targets hold comma-lists of order IDs — doing math
      // on them is nonsensical. Reject early with a helpful message.
      const ctx = makeCtx();
      expect(() => preprocessCommand('@o1 = buy 1$; echo @o1*2', ctx)).toThrow(/@o1 is a pending capture target/);
    });

    it('deferredSubstitution omitted when there are no deferrals', () => {
      // A normal chain with no captures shouldn't carry the field at
      // all (smaller serialized step, cleaner test inspection).
      const r = preprocessCommand('buy 1$; sell 2$', makeCtx());
      expect(r.steps.every((s) => s.deferredSubstitution === undefined)).toBe(true);
    });

    it('captures inside `if` propagate to OUTER segments', () => {
      // `@o1 = if (cond) (buy 1$); cancel @o1` — the inner captureTo
      // is the OUTER captureTo (inheritance rule), so the chain's
      // pendingCaptures gains o1 after the if-segment. The next
      // cancel defers o1.
      const ctx = makeCtx([pos({ contracts: 1 })]);
      const r = preprocessCommand('@o1 = if (position) (buy 1$); cancel @o1', ctx);
      expect(r.steps.length).toBe(2);
      expect(r.steps[0]).toMatchObject({ captureTo: 'o1' });
      expect(r.steps[1].command).toBe('cancel @o1');
      expect(r.steps[1].deferredSubstitution).toEqual(['o1']);
    });

    it('outer chain captures are visible INSIDE a nested if-branch body', () => {
      // `@o1 = buy 1$; if (cond) (cancel @o1)` — the inner branch's
      // cancel sees @o1 as pending (passed via outerPendingCaptures).
      const ctx = makeCtx([pos({ contracts: 1 })]);
      const r = preprocessCommand('@o1 = buy 1$; if (position) (cancel @o1)', ctx);
      expect(r.steps.length).toBe(2);
      expect(r.steps[0]).toMatchObject({ captureTo: 'o1' });
      // Inner cancel inherits the deferred substitution.
      expect(r.steps[1].command).toBe('cancel @o1');
      expect(r.steps[1].deferredSubstitution).toEqual(['o1']);
    });

    it("repeat-expanded captures: later iteration defers the earlier iteration's name", () => {
      // `repeat 2 (@o1 = buy 1$; cancel @o1)` textually expands to:
      //   @o1 = buy 1$; cancel @o1; @o1 = buy 1$; cancel @o1
      // After segment 1 captures o1, the segment 2 cancel defers.
      // After segment 3 re-captures (still o1), segment 4 also
      // defers. Each cancel resolves against whichever value is
      // current at runtime — last-write-wins is the documented
      // semantic.
      const r = preprocessCommand('repeat 2 (@o1 = buy 1$; cancel @o1)', makeCtx());
      expect(r.steps.length).toBe(4);
      expect(r.steps[0]).toMatchObject({ captureTo: 'o1' });
      expect(r.steps[1]).toMatchObject({
        command: 'cancel @o1',
        deferredSubstitution: ['o1'],
      });
      expect(r.steps[2]).toMatchObject({ captureTo: 'o1' });
      expect(r.steps[3]).toMatchObject({
        command: 'cancel @o1',
        deferredSubstitution: ['o1'],
      });
    });

    it('pendingCaptures resets across separate preprocessCommand calls', () => {
      const ctx = makeCtx();
      preprocessCommand('@o1 = buy 1$', ctx);
      // Fresh call: @o1 is NOT in pendingCaptures for THIS chain. The
      // store has @o1 bound (from the prior `set`-like writeback in
      // a real runner, but the test's MemoryPreprocessStore has no
      // var set here either), so substitute throws "Unknown".
      expect(() => preprocessCommand('cancel @o1', ctx)).toThrow(/Unknown variable @o1/);
    });
  });

  // TSBUG-106: `;` is sequential (the runner drains in-flight steps
  // before it → awaitPrior), `&` is parallel (no barrier). The chain
  // head normally has no awaitPrior (nothing in flight to drain); a
  // pathological leading `;` would set it, but that's a harmless no-op.
  describe('awaitPrior (`;` sequential vs `&` parallel)', () => {
    const aw = (cmd: string) => preprocessCommand(cmd, makeCtx()).steps.map((s) => s.awaitPrior ?? false);

    it('`;` marks the following step as a barrier', () => {
      expect(aw('buy 1; sell 1')).toEqual([false, true]);
    });

    it('`&` does NOT mark the following step', () => {
      expect(aw('buy 1 & sell 1')).toEqual([false, false]);
    });

    it('mixes per-separator across a chain', () => {
      // a ; b & c ; d → [first, ;, &, ;]
      expect(aw('buy 1; buy 2 & buy 3; buy 4')).toEqual([false, true, false, true]);
    });

    it('repeat with internal `;` makes every iteration boundary sequential', () => {
      expect(aw('repeat 3 (buy 1)')).toEqual([false, true, true]);
    });

    it('repeat joins iterations with `;` (sequential boundary) but keeps the in-iteration `&` parallel', () => {
      // expandRepeats joins copies with `;`, so: buy(head) & sleep ; buy & sleep ; buy & sleep
      // → [false(head), false(&), true(;), false(&), ...]
      expect(aw('repeat 3 (buy 1 & sleep 1s)').slice(0, 4)).toEqual([false, false, true, false]);
    });

    it('alias multi-statement body honors its own `;`', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('pair', 'buy 1$; sell 1$');
      // First sub-step (buy) is the chain head → no barrier; the `;`
      // before sell makes it sequential.
      expect(preprocessCommand('pair', ctx).steps.map((s) => s.awaitPrior ?? false)).toEqual([false, true]);
    });
  });
});
