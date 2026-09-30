import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { PositionSide, MarginType, PositionMode } from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { preprocessSegment } from './substitute';
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

describe('preprocessSegment', () => {
  describe('passthrough', () => {
    it('returns trimmed segment for plain trading commands with no tokens', () => {
      expect(preprocessSegment('  buy 100$  ', makeCtx())).toBe('buy 100$');
    });

    it('unset/print/alias/unalias pass through unchanged', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      ctx.store.setAlias('b1', 'buy 1');
      expect(preprocessSegment('unset @x', ctx)).toBe('unset @x');
      expect(preprocessSegment('print all', ctx)).toBe('print all');
      expect(preprocessSegment('alias b1 "buy at $entry"', ctx)).toBe('alias b1 "buy at $entry"');
      expect(preprocessSegment('unalias b1', ctx)).toBe('unalias b1');
    });

    it('set preserves @target but snapshots the value', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      // @x is the creation target — must stay literal so SetCommand can
      // pick it up. $entry is in the value position, so it snapshots.
      expect(preprocessSegment('set @x $entry', ctx)).toBe('set @x 50000');
      expect(preprocessSegment('set @x 100$', ctx)).toBe('set @x 100$');
    });
  });

  /**
   * `when (<cond>) (<action>)` is split-brained on purpose: the CONDITION is
   * resolved now, while the position giving `$entry` its meaning is the one
   * the user is looking at, and the ACTION is kept literal because `be +3%`
   * must read the position as it will be when it fires.
   *
   * Getting this backwards is silent and expensive. Substituting the action
   * would freeze `be +3%` to a number computed before TP1 ever filled.
   */
  describe('when (<condition>) (<action>)', () => {
    it('resolves the condition at arm time', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('when ($entry) (be)', ctx)).toBe('when (50000) (be)');
    });

    it('leaves the action untouched', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      // `$entry ` with a trailing SPACE, so the token is one the substitute
      // layer would definitely resolve. `(stop $entry)` is not a useful
      // assertion: the token there is `$entry)` and survives by accident.
      expect(preprocessSegment('when (price >= 100) (stop $entry -1%)', ctx)).toBe(
        'when (price >= 100) (stop $entry -1%)',
      );
    });

    it('keeps a chained action whole', () => {
      const ctx = makeCtx([pos({})]);
      expect(preprocessSegment('when (price >= 100) (be +3%; trail 20%)', ctx)).toBe(
        'when (price >= 100) (be +3%; trail 20%)',
      );
    });

    it('leaves a malformed form for the command to complain about', () => {
      const ctx = makeCtx([pos({})]);
      expect(preprocessSegment('when price >= 100 be', ctx)).toBe('when price >= 100 be');
    });

    /**
     * Reached through an alias the head is the ALIAS, so the check has to run
     * after expansion — exactly where `wait`'s `keepLiveVarsLiteral` check
     * runs, and for the same reason. Before this it was checked first, and an
     * aliased `when` had BOTH halves substituted: the action froze to a price
     * computed before the trigger ever fired.
     */
    it('still splits the halves when reached through an alias', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      ctx.store.setAlias('be3', 'when ($entry) (stop $entry -1%)');
      expect(preprocessSegment('be3', ctx)).toBe('when (50000) (stop $entry -1%)');
    });

    /**
     * A live var re-evaluates per read; a condition is re-checked per poll.
     * Freezing it here would compare against a snapshot taken when the trigger
     * was armed, which is the opposite of what `sets` means — and `wait`, the
     * other consumer of this condition vocabulary, keeps them literal for the
     * same reason.
     */
    it('leaves a live var in the condition for the poller to re-read', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      ctx.store.setLiveVar('hi', '$entry');
      expect(preprocessSegment('when (@hi >= 100) (be)', ctx)).toBe('when (@hi >= 100) (be)');
    });

    it('does not capture a command that merely starts with the same letters', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('whenever (x) (y)', ctx)).toBe('whenever (x) (y)');
    });
  });

  describe('user variables (@x)', () => {
    it('substitutes @x with its stored value', () => {
      const ctx = makeCtx();
      ctx.store.setVar('size', '100$');
      expect(preprocessSegment('buy @size', ctx)).toBe('buy 100$');
    });

    it('throws on unknown @x', () => {
      const ctx = makeCtx();
      expect(() => preprocessSegment('buy @missing', ctx)).toThrow(/Unknown variable @missing/);
    });

    it('does not treat bare @ as a var', () => {
      const ctx = makeCtx();
      // bare @ stays as-is
      expect(preprocessSegment('buy 100$ @', ctx)).toBe('buy 100$ @');
    });
  });

  describe('size constants ($size, $lsize, $ssize)', () => {
    it('$size resolves to total |contracts| for current symbol', () => {
      const ctx = makeCtx([pos({ side: PositionSide.Long, contracts: 1.5 })]);
      expect(preprocessSegment('close $size', ctx)).toBe('close 1.5');
    });

    it('$size sums across hedge sides', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
      ]);
      expect(preprocessSegment('close $size', ctx)).toBe('close 1.5');
    });

    it('$lsize / $ssize select side', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
      ]);
      expect(preprocessSegment('close $lsize', ctx)).toBe('close 1');
      expect(preprocessSegment('close $ssize', ctx)).toBe('close 0.5');
    });

    it('throws when there is no position', () => {
      const ctx = makeCtx();
      expect(() => preprocessSegment('close $size', ctx)).toThrow(/no position/);
      expect(() => preprocessSegment('close $lsize', ctx)).toThrow(/no long position/);
    });
  });

  describe('price constants ($entry, $lentry, $sentry)', () => {
    it('$entry resolves to the position entry price', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('buy 0.1 at $entry', ctx)).toBe('buy 0.1 at 50000');
    });

    it('$entry with positive offset combines', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('buy 0.1 at $entry +250', ctx)).toBe('buy 0.1 at 50250');
    });

    it('$entry with negative offset combines', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('buy 0.1 at $entry -100', ctx)).toBe('buy 0.1 at 49900');
    });

    it('$entry with positive percent offset combines', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('buy 0.1 at $entry +1%', ctx)).toBe('buy 0.1 at 50500');
    });

    it('$entry with negative percent offset combines', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('buy 0.1 at $entry -1%', ctx)).toBe('buy 0.1 at 49500');
    });

    it('$entry with fractional percent offset combines', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      expect(preprocessSegment('buy 0.1 at $entry +0.5%', ctx)).toBe(
        'buy 0.1 at 50250',
      );
      expect(preprocessSegment('buy 0.1 at $entry -0.1%', ctx)).toBe(
        'buy 0.1 at 49950',
      );
    });

    it('$entry percent offset works on $lentry / $sentry too', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, entryPrice: 50_000, contracts: 1 }),
        pos({ id: 's', side: PositionSide.Short, entryPrice: 60_000, contracts: 0.5 }),
      ]);
      expect(preprocessSegment('sell 0.1 at $lentry +1%', ctx)).toBe(
        'sell 0.1 at 50500',
      );
      expect(preprocessSegment('buy 0.1 at $sentry -1%', ctx)).toBe(
        'buy 0.1 at 59400',
      );
    });

    it('$entry errors when ambiguous in hedge mode', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
      ]);
      expect(() => preprocessSegment('buy 0.1 at $entry', ctx)).toThrow(/multiple positions/);
    });

    it('$lentry / $sentry pick by side', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, entryPrice: 50_000, contracts: 1 }),
        pos({ id: 's', side: PositionSide.Short, entryPrice: 60_000, contracts: 0.5 }),
      ]);
      expect(preprocessSegment('buy 0.1 at $lentry', ctx)).toBe('buy 0.1 at 50000');
      expect(preprocessSegment('buy 0.1 at $sentry', ctx)).toBe('buy 0.1 at 60000');
    });

    it('$liq resolves to the focused position\'s liquidation price', () => {
      const ctx = makeCtx([
        pos({ contracts: 1, entryPrice: 50_000, liquidationPrice: 45_000 }),
      ]);
      expect(preprocessSegment('stop sell at $liq', ctx)).toBe('stop sell at 45000');
    });

    it('$liq supports the same inline offsets as $entry', () => {
      const ctx = makeCtx([
        pos({ contracts: 1, entryPrice: 50_000, liquidationPrice: 45_000 }),
      ]);
      expect(preprocessSegment('stop sell at $liq +1%', ctx)).toBe('stop sell at 45450');
      expect(preprocessSegment('stop sell at $liq -100', ctx)).toBe('stop sell at 44900');
    });

    it('$liq throws when no position is open on focused symbol', () => {
      const ctx = makeCtx();
      expect(() => preprocessSegment('stop sell at $liq', ctx)).toThrow(/no position for BTCUSDT/);
    });

    it('$liq throws when liquidation price is missing or zero', () => {
      const ctx = makeCtx([
        pos({ contracts: 1, entryPrice: 50_000, liquidationPrice: 0 }),
      ]);
      expect(() => preprocessSegment('stop sell at $liq', ctx)).toThrow(/not available/);
    });

    it('$liq is ambiguous in hedge mode and errors', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, contracts: 1, liquidationPrice: 45_000 }),
        pos({ id: 's', side: PositionSide.Short, contracts: 0.5, liquidationPrice: 70_000 }),
      ]);
      expect(() => preprocessSegment('stop sell at $liq', ctx)).toThrow(/ambiguous/);
    });
  });

  describe('market-derived constants ($ticksize, $minsize)', () => {
    function ctxWithMarket(overrides: Partial<{
      precisionAmount: number;
      precisionPrice: number;
      minAmount: number;
    }> = {}): PreprocessContext {
      const ctx = makeCtx();
      (ctx.exchange!.store as { markets: unknown[] }).markets = [
        {
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          precision: {
            amount: overrides.precisionAmount ?? 0.001,
            price: overrides.precisionPrice ?? 0.5,
          },
          limits: {
            amount: { min: overrides.minAmount ?? 0.001, max: 1000 },
            leverage: { min: 1, max: 100 },
          },
        },
      ];
      return ctx;
    }

    it('$ticksize reads market.precision.price', () => {
      const ctx = ctxWithMarket({ precisionPrice: 0.5 });
      expect(preprocessSegment('echo $ticksize', ctx)).toBe('echo 0.5');
    });

    it('$minsize reads market.limits.amount.min', () => {
      const ctx = ctxWithMarket({ minAmount: 0.001 });
      expect(preprocessSegment('echo $minsize', ctx)).toBe('echo 0.001');
    });

    it('$minsize falls back to precision.amount when limits.amount.min is missing', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { markets: unknown[] }).markets = [
        {
          symbol: 'BTCUSDT',
          precision: { amount: 0.002, price: 0.5 },
          // no limits object at all
        },
      ];
      expect(preprocessSegment('echo $minsize', ctx)).toBe('echo 0.002');
    });

    it('both throw when market not loaded for focused symbol', () => {
      const ctx = makeCtx();
      expect(() => preprocessSegment('echo $ticksize', ctx)).toThrow(/market not loaded/);
      expect(() => preprocessSegment('echo $minsize', ctx)).toThrow(/market not loaded/);
    });

    it('$ticksize composes with bump-style math (`bump +$ticksize`)', () => {
      // The bump command is downstream; we just verify the
      // substituted token is a plain number that math can chew on.
      const ctx = ctxWithMarket({ precisionPrice: 0.1 });
      expect(preprocessSegment('echo 1+$ticksize', ctx)).toBe('echo 1.1');
    });
  });

  describe('UI-only constants', () => {
    describe('without resolvers (standalone CLI)', () => {
      it.each(['$click', '$ordersize', '$quick1', '$quick7'])(
        '%s throws with web-only message',
        (token) => {
          const ctx = makeCtx();
          expect(() => preprocessSegment(`buy 0.1 at ${token}`, ctx)).toThrow(/web terminal/);
        },
      );
    });

    describe('with resolvers (web)', () => {
      it('$click substitutes with the resolved price', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { click: () => '52000' };
        expect(preprocessSegment('buy 0.1 at $click', ctx)).toBe('buy 0.1 at 52000');
      });

      it('$click throws when no click recorded yet', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { click: () => undefined };
        expect(() => preprocessSegment('buy 0.1 at $click', ctx)).toThrow(/no chart click/);
      });

      it('$ordersize substitutes with the resolved size token', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { orderSize: () => '$1000' };
        expect(preprocessSegment('buy $ordersize', ctx)).toBe('buy $1000');
      });

      it('$ordersize throws when the form is empty', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { orderSize: () => undefined };
        expect(() => preprocessSegment('buy $ordersize', ctx)).toThrow(/linked order form/);
      });

      it('$quick<n> substitutes the n-th button value', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { quick: (n) => (n === 3 ? '$10000' : undefined) };
        expect(preprocessSegment('buy $quick3', ctx)).toBe('buy $10000');
      });

      it('$quick<n> throws when slot is empty', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { quick: () => undefined };
        expect(() => preprocessSegment('buy $quick1', ctx)).toThrow(/no value/);
      });

      it.each([1, 2, 3, 4, 5, 6, 7])('$quick%i is accepted', (n) => {
        const ctx = makeCtx();
        ctx.uiResolvers = { quick: () => '$1' };
        expect(() => preprocessSegment(`buy $quick${n}`, ctx)).not.toThrow();
      });

      it('$quick0 / $quick8 are not matched as UI constants — fall through as unknown vars', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = { quick: () => '$1' };
        // The regex only matches 1..7. Out-of-range tokens are not
        // UI-only constants — they fall to the `@var` path and throw
        // for unknown variables (the `$` prefix here is just a literal
        // that the substitute layer leaves alone).
        expect(() => preprocessSegment('buy $quick0', ctx)).not.toThrow();
        expect(() => preprocessSegment('buy $quick8', ctx)).not.toThrow();
      });

      it('throws on each token when the matching resolver is missing', () => {
        const ctx = makeCtx();
        ctx.uiResolvers = {}; // resolvers object present but empty
        expect(() => preprocessSegment('buy 0.1 at $click', ctx)).toThrow(/no chart click/);
        expect(() => preprocessSegment('buy $ordersize', ctx)).toThrow(/linked order form/);
        expect(() => preprocessSegment('buy $quick1', ctx)).toThrow(/no value/);
      });
    });
  });

  describe('aliases', () => {
    it('expands a leading alias and substitutes the rest', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('b1', 'buy 100$');
      expect(preprocessSegment('b1', ctx)).toBe('buy 100$');
    });

    it('appends trailing args after expansion', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('b1', 'buy 100$');
      expect(preprocessSegment('b1 ETHUSDT', ctx)).toBe('buy 100$ ETHUSDT');
    });

    it('expands recursively', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('b1', 'b2');
      ctx.store.setAlias('b2', 'buy 100$');
      expect(preprocessSegment('b1', ctx)).toBe('buy 100$');
    });

    it('throws on cycles', () => {
      const ctx = makeCtx();
      ctx.store.setAlias('a', 'b');
      ctx.store.setAlias('b', 'a');
      expect(() => preprocessSegment('a', ctx)).toThrow(/depth exceeded/);
    });
  });

  describe('ticker price constants ($bid, $ask, $last, $mark, $index)', () => {
    function ctxWithTicker(
      tickerOverrides: Partial<{
        bid: number;
        ask: number;
        last: number;
        mark: number;
        index: number;
      }> = {},
    ): PreprocessContext {
      const ctx = makeCtx();
      const ticker = {
        symbol: 'BTCUSDT',
        bid: tickerOverrides.bid ?? 49_995,
        ask: tickerOverrides.ask ?? 50_005,
        last: tickerOverrides.last ?? 50_000,
        mark: tickerOverrides.mark ?? 50_002,
        index: tickerOverrides.index ?? 50_001,
      };
      (ctx.exchange!.store as { tickers: typeof ticker[] }).tickers = [ticker];
      return ctx;
    }

    it.each([
      ['$bid', 49995],
      ['$ask', 50005],
      ['$last', 50000],
      ['$mark', 50002],
      ['$index', 50001],
    ])('%s resolves to the ticker field (%d)', (tok, expected) => {
      const ctx = ctxWithTicker();
      expect(preprocessSegment(`buy 0.1 at ${tok}`, ctx)).toBe(
        `buy 0.1 at ${expected}`,
      );
    });

    it('combines `+N` absolute offset', () => {
      const ctx = ctxWithTicker();
      expect(preprocessSegment('buy 0.1 at $bid +5', ctx)).toBe('buy 0.1 at 50000');
      expect(preprocessSegment('sell 0.1 at $ask -5', ctx)).toBe('sell 0.1 at 50000');
    });

    it('combines `+N%` percent offset', () => {
      const ctx = ctxWithTicker();
      // $last = 50_000 → +1% = 50_500
      expect(preprocessSegment('buy 0.1 at $last +1%', ctx)).toBe('buy 0.1 at 50500');
      // $mark = 50_002 → -0.5% = 49751.99
      expect(preprocessSegment('sell 0.1 at $mark -0.5%', ctx)).toBe('sell 0.1 at 49751.99');
    });

    it('throws when the ticker is missing for the focused symbol', () => {
      const ctx = makeCtx();
      // No tickers loaded.
      expect(() => preprocessSegment('buy 0.1 at $bid', ctx)).toThrow(/no ticker loaded/);
    });

    it('throws when the field is not a finite number', () => {
      const ctx = ctxWithTicker();
      // mark is undefined / NaN / 0-as-falsy guarded only against
      // non-number; an explicit NaN should also be rejected because
      // NaN isn't useful as a price.
      const tickers = ctx.exchange!.store!.tickers as { mark?: number }[];
      tickers[0].mark = NaN;
      expect(() => preprocessSegment('buy 0.1 at $mark', ctx)).toThrow(/not available/);
    });

    it('throws when not connected to an exchange', () => {
      const ctx = makeCtx();
      ctx.exchange = undefined;
      expect(() => preprocessSegment('buy 0.1 at $last', ctx)).toThrow(/not connected/);
    });

    it('reads from the FOCUSED symbol, not other symbols in the ticker store', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_995, ask: 50_005, last: 50_000, mark: 50_002, index: 50_001 },
        { symbol: 'ETHUSDT', bid: 2_999, ask: 3_001, last: 3_000, mark: 3_000, index: 3_000 },
      ];
      ctx.symbol = 'ETHUSDT';
      expect(preprocessSegment('buy 1 at $bid', ctx)).toBe('buy 1 at 2999');
    });

    it('$mid prefers ticker.midPrice when set (exchange-published mid)', () => {
      // Mirrors the Hyperliquid case: bid/ask are impact prices, but
      // the venue publishes a separate canonical mid (`midPx`) which
      // safe-cex maps to `ticker.midPrice`. `$mid` should read that
      // — NOT compute (bid+ask)/2 — because Hyperliquid's bid/ask
      // are impact-derived and noticeably wider than the true mid.
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_950, ask: 50_050, midPrice: 50_000, last: 50_000 },
      ];
      expect(preprocessSegment('buy 1 at $mid', ctx)).toBe('buy 1 at 50000');
      expect(preprocessSegment('buy 1 at $midprice', ctx)).toBe('buy 1 at 50000');
    });

    it('$mid falls back to (bid+ask)/2 when midPrice is unset', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_990, ask: 50_010, last: 50_000, mark: 50_000, index: 50_000 },
      ];
      // (49_990 + 50_010) / 2 = 50_000
      expect(preprocessSegment('buy 1 at $mid', ctx)).toBe('buy 1 at 50000');
    });

    it('$mid ignores a non-finite midPrice and falls back', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_990, ask: 50_010, midPrice: NaN },
      ];
      // NaN doesn't pass the finite check → fallback fires.
      expect(preprocessSegment('echo $mid', ctx)).toBe('echo 50000');
    });

    it('$mid accepts inline offsets like the other price anchors', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_990, ask: 50_010, midPrice: 50_000 },
      ];
      // mid 50_000 -1% = 49_500
      expect(preprocessSegment('buy 1 at $mid -1%', ctx)).toBe('buy 1 at 49500');
      // mid 50_000 + 100 = 50_100
      expect(preprocessSegment('buy 1 at $mid +100', ctx)).toBe('buy 1 at 50100');
    });

    it('$mid throws clearly when bid is missing AND midPrice is missing', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', ask: 50_010 },
      ];
      expect(() => preprocessSegment('buy 1 at $mid', ctx)).toThrow(/bid not available/);
    });

    it('$mid throws clearly when ask is missing AND midPrice is missing', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_990 },
      ];
      expect(() => preprocessSegment('buy 1 at $mid', ctx)).toThrow(/ask not available/);
    });

    it('$mid throws when ticker is missing entirely', () => {
      const ctx = makeCtx();
      expect(() => preprocessSegment('buy 1 at $mid', ctx)).toThrow(/no ticker loaded/);
    });

    it('$mid handles fractional results from the (bid+ask)/2 fallback', () => {
      const ctx = makeCtx();
      (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
        { symbol: 'BTCUSDT', bid: 49_995, ask: 50_006 },
      ];
      // (49_995 + 50_006) / 2 = 50_000.5
      expect(preprocessSegment('echo $mid', ctx)).toBe('echo 50000.5');
    });
  });

  describe('account / pnl constants', () => {
    function withBalance(
      ctx: PreprocessContext,
      b: Partial<{ free: number; used: number; total: number; upnl: number }> = {},
    ): void {
      (ctx.exchange!.store as { balance: unknown }).balance = {
        free: b.free ?? 9_000,
        used: b.used ?? 1_000,
        total: b.total ?? 10_000,
        upnl: b.upnl ?? 50,
      };
    }

    it.each([
      ['$free', 9000],
      ['$used', 1000],
      ['$total', 10000],
      ['$upnl', 50],
    ])('%s reads the matching balance field (%d)', (tok, expected) => {
      const ctx = makeCtx();
      withBalance(ctx);
      expect(preprocessSegment(`print ${tok}`, ctx)).toBe(
        `print ${tok}`, // print is FULL_PASSTHROUGH
      );
      // Use echo / a non-passthrough head so substitute actually fires.
      expect(preprocessSegment(`buy ${tok}$`, ctx)).toBe(`buy ${tok}$`);
      // The token itself must NOT be embedded in another token; spaces
      // around it allow substitute to see it as a standalone token.
      expect(preprocessSegment(`echo ${tok}`, ctx)).toBe(`echo ${expected}`);
    });

    it('throws when balance is not loaded', () => {
      const ctx = makeCtx();
      expect(() => preprocessSegment('echo $free', ctx)).toThrow(/no balance loaded/);
    });

    it('throws when not connected to an exchange', () => {
      const ctx = makeCtx();
      ctx.exchange = undefined;
      expect(() => preprocessSegment('echo $total', ctx)).toThrow(/not connected/);
    });

    it('$pnl sums unrealized PnL across the focused symbol', () => {
      const ctx = makeCtx([
        pos({ id: 'l', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1, unrealizedPnl: 75 }),
        pos({ id: 's', symbol: 'BTCUSDT', side: PositionSide.Short, contracts: 0.5, unrealizedPnl: -25 }),
        pos({ id: 'eth', symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 1, unrealizedPnl: 999 }),
      ]);
      withBalance(ctx);
      // Both BTC sides → 75 + (-25) = 50; ETH not included.
      expect(preprocessSegment('echo $pnl', ctx)).toBe('echo 50');
    });

    it('$pnl errors when there is no position on the focused symbol', () => {
      const ctx = makeCtx([
        pos({ symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 1, unrealizedPnl: 10 }),
      ]);
      withBalance(ctx);
      expect(() => preprocessSegment('echo $pnl', ctx)).toThrow(/no position for BTCUSDT/);
    });

    it('$totalupnl sums unrealized PnL across ALL non-zero positions', () => {
      const ctx = makeCtx([
        pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1, unrealizedPnl: 30 }),
        pos({ id: 'b', symbol: 'ETHUSDT', side: PositionSide.Short, contracts: 0.5, unrealizedPnl: -10 }),
        pos({ id: 'c', symbol: 'SOLUSDT', side: PositionSide.Long, contracts: 0, unrealizedPnl: 100 }),
      ]);
      withBalance(ctx);
      expect(preprocessSegment('echo $totalupnl', ctx)).toBe('echo 20');
    });

    it('$totallongusd / $totalshortusd sum notional by side', () => {
      const ctx = makeCtx([
        pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1, notional: 50_000 }),
        pos({ id: 'b', symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10, notional: 30_000 }),
        pos({ id: 'c', symbol: 'SOLUSDT', side: PositionSide.Short, contracts: 100, notional: 20_000 }),
      ]);
      withBalance(ctx);
      expect(preprocessSegment('echo $totallongusd', ctx)).toBe('echo 80000');
      expect(preprocessSegment('echo $totalshortusd', ctx)).toBe('echo 20000');
    });

    it('$totallong / $totalshort divide notional by total balance (ratio)', () => {
      const ctx = makeCtx([
        pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1, notional: 50_000 }),
        pos({ symbol: 'SOLUSDT', side: PositionSide.Short, contracts: 100, notional: 25_000 }),
      ]);
      withBalance(ctx, { total: 100_000 });
      expect(preprocessSegment('echo $totallong', ctx)).toBe('echo 0.5');
      expect(preprocessSegment('echo $totalshort', ctx)).toBe('echo 0.25');
    });

    it('$totallong throws when total balance is zero', () => {
      const ctx = makeCtx([
        pos({ side: PositionSide.Long, contracts: 1, notional: 50_000 }),
      ]);
      withBalance(ctx, { total: 0 });
      expect(() => preprocessSegment('echo $totallong', ctx)).toThrow(/total balance/);
    });

    it('returns 0 totals when there are no positions on the requested side', () => {
      const ctx = makeCtx([
        pos({ side: PositionSide.Long, contracts: 1, notional: 50_000 }),
      ]);
      withBalance(ctx);
      expect(preprocessSegment('echo $totalshortusd', ctx)).toBe('echo 0');
      expect(preprocessSegment('echo $totalshort', ctx)).toBe('echo 0');
    });

    it('$margin = abs(notional) / leverage on the focused symbol', () => {
      // 50_000 notional / 10x leverage = 5_000 margin.
      const ctx = makeCtx([
        pos({ contracts: 1, notional: 50_000, leverage: 10 }),
      ]);
      withBalance(ctx);
      expect(preprocessSegment('echo $margin', ctx)).toBe('echo 5000');
    });

    it('$margin sums across long + short positions on the focused symbol', () => {
      const ctx = makeCtx([
        pos({ id: 'l', side: PositionSide.Long, contracts: 1, notional: 50_000, leverage: 10 }),
        pos({ id: 's', side: PositionSide.Short, contracts: 0.5, notional: 30_000, leverage: 5 }),
      ]);
      withBalance(ctx);
      // 50000/10 + 30000/5 = 5000 + 6000 = 11000
      expect(preprocessSegment('echo $margin', ctx)).toBe('echo 11000');
    });

    it('$margin defaults leverage to 1 when missing/zero', () => {
      const ctx = makeCtx([
        pos({ contracts: 1, notional: 50_000, leverage: 0 }),
      ]);
      withBalance(ctx);
      expect(preprocessSegment('echo $margin', ctx)).toBe('echo 50000');
    });

    it('$margin throws when no position on focused symbol', () => {
      const ctx = makeCtx([
        pos({ symbol: 'ETHUSDT', contracts: 1, notional: 3000, leverage: 1 }),
      ]);
      withBalance(ctx);
      expect(() => preprocessSegment('echo $margin', ctx)).toThrow(/no position for BTCUSDT/);
    });
  });

  describe('$random — synthesized constant', () => {
    beforeEach(() => {
      // Deterministic value: Math.random() → 0.5 → 0.35 + 0.5*0.65 = 0.675.
      vi.spyOn(Math, 'random').mockReturnValue(0.5);
    });
    afterEach(() => {
      vi.restoreAllMocks();
    });

    it('substitutes a numeric token', () => {
      const ctx = makeCtx();
      expect(preprocessSegment('echo $random', ctx)).toBe('echo 0.675');
    });

    it('works inside a math expression', () => {
      const ctx = makeCtx();
      // 10 * 0.675 = 6.75
      expect(preprocessSegment('buy 10*$random', ctx)).toBe('buy 6.75');
    });

    it('resolves to a value in [0.35, 1)', () => {
      vi.restoreAllMocks();
      const ctx = makeCtx();
      for (let i = 0; i < 50; i++) {
        const out = preprocessSegment('echo $random', ctx);
        const n = parseFloat(out.replace('echo ', ''));
        expect(n).toBeGreaterThanOrEqual(0.35);
        expect(n).toBeLessThan(1);
      }
    });

    it('multiple substitutions in one chain get distinct values', () => {
      vi.restoreAllMocks();
      const ctx = makeCtx();
      // Track values across many runs to make sure we don't accidentally
      // freeze on the first value.
      const seen = new Set<string>();
      for (let i = 0; i < 10; i++) {
        seen.add(preprocessSegment('echo $random', ctx));
      }
      expect(seen.size).toBeGreaterThan(1);
    });

    it('does not require an exchange connection', () => {
      const ctx = makeCtx();
      ctx.exchange = null as unknown as ExchangeLike;
      expect(() => preprocessSegment('echo $random', ctx)).not.toThrow();
    });
  });

  describe('live vars (@x bound via setLiveVar)', () => {
    function ctxWithTickers(
      tickers: Array<{
        symbol: string;
        last?: number;
        bid?: number;
        ask?: number;
        mark?: number;
        index?: number;
      }>,
    ): PreprocessContext {
      const ctx = makeCtx();
      (ctx.exchange!.store! as { tickers: typeof tickers }).tickers = tickers;
      return ctx;
    }

    it('substitutes @x by evaluating the source at substitute time', () => {
      const ctx = ctxWithTickers([
        { symbol: 'BTCUSDT', bid: 49_990, ask: 50_010 },
      ]);
      ctx.store.setLiveVar('spread', '$ask-$bid');
      // @spread → 50010 - 49990 = 20.
      expect(preprocessSegment('echo @spread', ctx)).toBe('echo 20');
    });

    it('the same @x re-evaluates each substitution (reactivity)', () => {
      const tickers = [{ symbol: 'BTCUSDT', last: 100 }];
      const ctx = ctxWithTickers(tickers);
      ctx.store.setLiveVar('x', '$last');

      expect(preprocessSegment('echo @x', ctx)).toBe('echo 100');
      tickers[0].last = 150;
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 150');
      tickers[0].last = 200;
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 200');
    });

    it('live binding wins over a same-name static (mutex enforced at storage)', () => {
      // Storage layer auto-clears the static on setLiveVar — sanity-check
      // that substitution doesn't return the now-cleared static instead.
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
      ctx.store.setVar('x', '999');
      ctx.store.setLiveVar('x', '$last');
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 50000');
    });

    it('flipping back to static via setVar re-substitutes the static value', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
      ctx.store.setLiveVar('x', '$last');
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 50000');
      ctx.store.setVar('x', '777');
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 777');
    });

    it('live var inside a math token resolves through the math path', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
      ctx.store.setLiveVar('x', '$last');
      // `@x*2` is a math token — operand resolver must follow the live
      // binding too, not fall through to "unknown variable".
      expect(preprocessSegment('echo @x*2', ctx)).toBe('echo 100000');
    });

    it('live var inside math token with a $constant operand', () => {
      const ctx = ctxWithTickers([
        { symbol: 'BTCUSDT', last: 50_000, bid: 49_990, ask: 50_010 },
      ]);
      ctx.store.setLiveVar('spread', '$ask-$bid');
      // @spread = 20, $bid = 49990 → @spread*2+$bid = 50030.
      expect(preprocessSegment('echo @spread*2+$bid', ctx)).toBe('echo 50030');
    });

    it('multi-token source (anchor + offset) evaluates correctly', () => {
      const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
      ctx.store.setLiveVar('above', '$entry +1%');
      expect(preprocessSegment('echo @above', ctx)).toBe('echo 50500');
    });

    it('numeric-literal source works (degenerate live var)', () => {
      const ctx = makeCtx();
      ctx.store.setLiveVar('k', '42');
      expect(preprocessSegment('echo @k', ctx)).toBe('echo 42');
    });

    it('nested live vars — @y references @x', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 100 }]);
      ctx.store.setLiveVar('x', '$last');
      ctx.store.setLiveVar('y', '@x*2');
      expect(preprocessSegment('echo @y', ctx)).toBe('echo 200');
    });

    it('cycle detection — direct `sets @a @a` self-loop', () => {
      const ctx = makeCtx();
      ctx.store.setLiveVar('a', '@a');
      expect(() => preprocessSegment('echo @a', ctx)).toThrow(
        /live var @a: cycle detected.*@a/i,
      );
    });

    it('cycle detection — two-step `@a → @b → @a`', () => {
      const ctx = makeCtx();
      ctx.store.setLiveVar('a', '@b');
      ctx.store.setLiveVar('b', '@a');
      expect(() => preprocessSegment('echo @a', ctx)).toThrow(
        /live var.*cycle detected/i,
      );
    });

    it('cycle detection — three-step `@a → @b → @c → @a`', () => {
      const ctx = makeCtx();
      ctx.store.setLiveVar('a', '@b');
      ctx.store.setLiveVar('b', '@c');
      ctx.store.setLiveVar('c', '@a');
      expect(() => preprocessSegment('echo @a', ctx)).toThrow(
        /live var.*cycle detected/i,
      );
    });

    it('cycle detection through MATH operand (not just bare @-token)', () => {
      const ctx = makeCtx();
      ctx.store.setLiveVar('a', '@b*2');
      ctx.store.setLiveVar('b', '@a+1');
      expect(() => preprocessSegment('echo @a', ctx)).toThrow(
        /live var.*cycle detected/i,
      );
    });

    it('non-cycle reuse — @a referenced twice from different places is fine', () => {
      // Diamond shape: @c uses @a and @b; both @a and @b independently
      // reference @x. Not a cycle — each subtree completes before the
      // sibling starts, so `seen` clears between branches.
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 100 }]);
      ctx.store.setLiveVar('x', '$last');
      ctx.store.setLiveVar('a', '@x');
      ctx.store.setLiveVar('b', '@x*2');
      ctx.store.setLiveVar('c', '@a+@b');
      // @c = @a + @b = 100 + 200 = 300
      expect(preprocessSegment('echo @c', ctx)).toBe('echo 300');
    });

    it('errors inside a live-var source are wrapped with the var name', () => {
      const ctx = makeCtx(); // No position → $entry throws.
      ctx.store.setLiveVar('above', '$entry');
      expect(() => preprocessSegment('echo @above', ctx)).toThrow(
        /live var @above source '\$entry'.*no position/i,
      );
    });

    it('still throws "Unknown variable" when the name is neither static nor live', () => {
      // Existing error message preserved for plain @-tokens that have
      // no binding at all.
      const ctx = makeCtx();
      expect(() => preprocessSegment('echo @missing', ctx)).toThrow(
        /Unknown variable @missing/,
      );
    });

    it('clearing the live binding then using @x falls back to static (when present)', () => {
      // Storage layer mutex prevents this from happening in normal use,
      // but `unsetLiveVar` alone leaves no binding at all → the same
      // "Unknown variable" path fires.
      const ctx = makeCtx();
      ctx.store.setLiveVar('x', '$last');
      ctx.store.unsetLiveVar('x');
      expect(() => preprocessSegment('echo @x', ctx)).toThrow(
        /Unknown variable @x/,
      );
    });

    describe('inside `wait` args, live vars stay LITERAL', () => {
      // Wait re-evaluates on each poll, so substituting the live var
      // at preprocess time would freeze the value and break reactivity.
      // Locked here so a refactor doesn't silently flip the behavior.

      it('`wait @x > 25` preserves @x in the emitted command', () => {
        const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
        ctx.store.setLiveVar('x', '$last');
        expect(preprocessSegment('wait @x > 25', ctx)).toBe('wait @x > 25');
      });

      it('static @-vars inside wait STILL substitute (freezing is fine for static)', () => {
        const ctx = makeCtx();
        ctx.store.setVar('x', '50');
        // Static @x → frozen value 50. The wait poller wouldn't see a
        // change anyway since the var doesn't tick, so substituting now
        // costs nothing and keeps the wait parser simple.
        expect(preprocessSegment('wait @x > 25', ctx)).toBe('wait 50 > 25');
      });

      it('non-wait commands still substitute live vars normally', () => {
        const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
        ctx.store.setLiveVar('x', '$last');
        // The defer is wait-specific. `echo` resolves to the current
        // value as before.
        expect(preprocessSegment('echo @x', ctx)).toBe('echo 50000');
      });

      it('an alias whose body is `wait …` still triggers the defer', () => {
        const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
        ctx.store.setLiveVar('x', '$last');
        ctx.store.setAlias('w', 'wait @x > 25');
        // After alias expansion, head becomes `wait` — defer applies.
        expect(preprocessSegment('w', ctx)).toBe('wait @x > 25');
      });
    });
  });

  describe('tracked vars (@x bound via setTrackedVar)', () => {
    function ctxWithTickers(
      tickers: Array<{
        symbol: string;
        last?: number;
        bid?: number;
        ask?: number;
        mark?: number;
      }>,
    ): PreprocessContext {
      const ctx = makeCtx();
      (ctx.exchange!.store! as { tickers: typeof tickers }).tickers = tickers;
      return ctx;
    }

    it('substitutes @x by RESOLVING the tracker (seeding on first read)', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 100 }]);
      ctx.store.setTrackedVar('hi', 'max', '$last');
      expect(preprocessSegment('echo @hi', ctx)).toBe('echo 100');
      expect(ctx.store.getTrackedVar('hi')!.value).toBe(100);
    });

    it('ratchets to a new max on subsequent reads when the source rises', () => {
      const tickers = [{ symbol: 'BTCUSDT', last: 100 }];
      const ctx = ctxWithTickers(tickers);
      ctx.store.setTrackedVar('hi', 'max', '$last');
      expect(preprocessSegment('echo @hi', ctx)).toBe('echo 100');
      tickers[0].last = 200;
      expect(preprocessSegment('echo @hi', ctx)).toBe('echo 200');
      tickers[0].last = 150;
      // Falls back below the high — substitution returns the high.
      expect(preprocessSegment('echo @hi', ctx)).toBe('echo 200');
    });

    it('min kind ratchets DOWN', () => {
      const tickers = [{ symbol: 'BTCUSDT', last: 100 }];
      const ctx = ctxWithTickers(tickers);
      ctx.store.setTrackedVar('lo', 'min', '$last');
      expect(preprocessSegment('echo @lo', ctx)).toBe('echo 100');
      tickers[0].last = 50;
      expect(preprocessSegment('echo @lo', ctx)).toBe('echo 50');
      tickers[0].last = 200;
      expect(preprocessSegment('echo @lo', ctx)).toBe('echo 50');
    });

    it('tracked beats live (precedence) — but storage mutex makes the case rare', () => {
      // Hand-construct a state where both bindings exist (the mutex
      // would normally prevent this, but we want to lock in the
      // precedence at the substitute layer too).
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
      ctx.store.setLiveVar('x', '$last');
      // Force-add a tracker on the same name through the underlying
      // map (set sets clears, so go around it). We test via the
      // setTrackedVar API which clears the live var; that's the
      // documented behavior. After this call only the tracker
      // remains.
      ctx.store.setTrackedVar('x', 'max', '$last');
      expect(ctx.store.getLiveVar('x')).toBeUndefined();
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 50000');
    });

    it('flipping to static via setVar resolves to the frozen value', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
      ctx.store.setTrackedVar('x', 'max', '$last');
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 50000');
      ctx.store.setVar('x', '999');
      expect(preprocessSegment('echo @x', ctx)).toBe('echo 999');
    });

    it('tracker inside a math token resolves through the math path', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 100 }]);
      ctx.store.setTrackedVar('hi', 'max', '$last');
      // First eval seeds @hi to 100. Math: @hi*2 = 200.
      expect(preprocessSegment('echo @hi*2', ctx)).toBe('echo 200');
    });

    it('tracker source-eval errors propagate at first use', () => {
      const ctx = makeCtx(); // no positions
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      expect(() => preprocessSegment('echo @hi', ctx)).toThrow(
        /live var @hi source.*no position/i,
      );
    });

    it('inside `wait` args, trackers stay LITERAL', () => {
      const ctx = ctxWithTickers([{ symbol: 'BTCUSDT', last: 100 }]);
      ctx.store.setTrackedVar('hi', 'max', '$last');
      // wait-segment: defer so the poller updates the tracker on
      // every tick. Without this, the wait would freeze at the
      // first observation and never reflect a new high.
      expect(preprocessSegment('wait @hi > 50', ctx)).toBe('wait @hi > 50');
    });
  });
});

/**
 * `$roe<N>` — the price at which the focused position's ROE would be N%.
 *
 * Asked for as a TP ladder denominated in ROE rather than price (Discord,
 * 2026-09-29): legs at 25% / 60% / 100% ROE and a runner armed at 101%.
 * Inverts safe-cex's `calculateReturns`, so a target typed here and the figure
 * in the positions table mean the same thing.
 */
describe('$roe', () => {
  const long = (over: Partial<Position> = {}) => pos({ entryPrice: 50_000, leverage: 10, ...over });

  // 25% ROE at 10x is a 2.5% price move.
  it('converts an ROE target to a price on a long', () => {
    expect(preprocessSegment('sell 30%pos $roe25 reduce', makeCtx([long()]))).toBe('sell 30%pos 51250 reduce');
  });

  it('moves the other way on a short', () => {
    const ctx = makeCtx([long({ side: PositionSide.Short })]);
    expect(preprocessSegment('buy 30%pos $roe25 reduce', ctx)).toBe('buy 30%pos 48750 reduce');
  });

  it('scales with leverage — the same ROE is a smaller move at higher leverage', () => {
    expect(preprocessSegment('sell $roe100', makeCtx([long({ leverage: 10 })]))).toBe('sell 55000');
    expect(preprocessSegment('sell $roe100', makeCtx([long({ leverage: 20 })]))).toBe('sell 52500');
  });

  it('takes a negative target, below entry on a long', () => {
    expect(preprocessSegment('stop $roe-50', makeCtx([long()]))).toBe('stop 47500');
  });

  // The whole point: his ladder and his runner in one chain.
  it('reads inside a wait condition, which is how the runner arms', () => {
    const ctx = makeCtx([long()]);
    (ctx.exchange as unknown as { store: { tickers: unknown[] } }).store.tickers = [
      { symbol: 'BTCUSDT', last: 50_000, mark: 50_000, bid: 49_999, ask: 50_001, index: 50_000 },
    ];
    expect(preprocessSegment('wait $last >= $roe101', ctx)).toBe('wait 50000 >= 55050');
  });

  /**
   * A 0 leverage would silently report every target as the entry price — the
   * same trap `withKnownReturns` documents for the positions table.
   */
  it('refuses rather than defaulting when leverage is unavailable', () => {
    expect(() => preprocessSegment('sell $roe25', makeCtx([long({ leverage: 0 })]))).toThrow(/leverage not available/);
  });

  it('refuses with no position, and when the side is ambiguous', () => {
    expect(() => preprocessSegment('sell $roe25', makeCtx([]))).toThrow(/no position/);
    const both = makeCtx([long({ id: 'l' }), long({ id: 's', side: PositionSide.Short })]);
    expect(() => preprocessSegment('sell $roe25', both)).toThrow(/ambiguous/);
  });

  /**
   * Does NOT branch on `isInverse`, deliberately. `Position.contracts` is
   * normalised to base, which is what makes `size * price-distance` hold on
   * both — so `utils/risk.ts` and `calculateReturns` contain no `isInverse`
   * and neither does this. A price-layer inverse branch is a bug, not a fix.
   */
  it('answers the same way on an inverse market, like the rest of the price layer', () => {
    const ctx = makeCtx([long()]);
    (ctx.exchange as unknown as { store: { markets: unknown[] } }).store.markets = [
      { symbol: 'BTCUSDT', base: 'BTC', quote: 'USD', isInverse: true },
    ];
    expect(preprocessSegment('sell $roe25', ctx)).toBe('sell 51250');
  });

  it('is not confused with a plain word starting the same way', () => {
    expect(preprocessSegment('sell $roe', makeCtx([long()]))).toBe('sell $roe');
  });
});
