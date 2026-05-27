/**
 * Direct unit tests for the cli-core utility commands. The
 * preprocess integration suite exercises them indirectly through
 * substitution (`set`, `alias` skip the substitute layer), but the
 * commands themselves also touch the registry, the adapter, and the
 * preprocess store on every call. Test those code paths here.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SetCommand } from './SetCommand';
import { SetsCommand } from './SetsCommand';
import { TrackCommand } from './TrackCommand';
import { UntrackCommand } from './UntrackCommand';
import { UnsetCommand } from './UnsetCommand';
import { HistoryCommand } from './HistoryCommand';
import { PrintCommand } from './PrintCommand';
import { AliasCommand } from './AliasCommand';
import { UnaliasCommand } from './UnaliasCommand';
import { SleepCommand } from './SleepCommand';
import { ClearCommand } from './ClearCommand';
import { WhitelistCommand, UnwhitelistCommand } from './WhitelistCommand';
import { FatfingerCommand, UnfatfingerCommand } from './FatfingerCommand';
import { BudgetCommand } from './BudgetCommand';
import { SimulationCommand } from './SimulationCommand';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';
import type { CommandContext } from '../Command';
import type { CLIPlatformAdapter } from '../../types';

function makeAdapter(): CLIPlatformAdapter {
  return {
    print: vi.fn(),
    printInfo: vi.fn(),
    printError: vi.fn(),
    printSuccess: vi.fn(),
    printWarning: vi.fn(),
    printRaw: vi.fn(),
    listAccounts: vi.fn().mockReturnValue([]),
    getAccount: vi.fn(),
    addAccount: vi.fn(),
    updateAccount: vi.fn(),
    removeAccount: vi.fn(),
    setDefaultAccount: vi.fn(),
    getDefaultSymbol: vi.fn(),
    setDefaultSymbol: vi.fn(),
    getActiveAccounts: vi.fn().mockReturnValue([]),
    listExchanges: vi.fn().mockReturnValue([]),
    createExchange: vi.fn(),
    startExchange: vi.fn(),
    stopExchange: vi.fn(),
    startCapture: vi.fn(),
  } as unknown as CLIPlatformAdapter;
}

function ctx(args: string[], store?: MemoryPreprocessStore, adapter?: CLIPlatformAdapter): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol: 'BTCUSDT',
    accountName: 'a',
    exchange: null,
    adapter: adapter ?? makeAdapter(),
    preprocessStore: store,
  } as unknown as CommandContext;
}

// ─── SetCommand ──────────────────────────────────────────────────────

describe('SetCommand', () => {
  let cmd: SetCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new SetCommand();
    store = new MemoryPreprocessStore();
  });

  it('round-trips a single value into the store', async () => {
    await cmd.execute(ctx(['set', '@x', '100$'], store));
    expect(store.getVar('x')).toBe('100$');
  });

  it('joins multi-token values with a single space', async () => {
    // The preprocess layer already substitutes value tokens before
    // SetCommand sees them, but the command itself still needs to
    // join multi-token args back into one stored string.
    await cmd.execute(ctx(['set', '@expr', 'buy', '100$', 'at', '50000'], store));
    expect(store.getVar('expr')).toBe('buy 100$ at 50000');
  });

  it('overwrites an existing var', async () => {
    store.setVar('x', 'old');
    await cmd.execute(ctx(['set', '@x', 'new'], store));
    expect(store.getVar('x')).toBe('new');
  });

  it('errors on missing target', async () => {
    const r = await cmd.execute(ctx(['set'], store));
    expect(r.success).toBe(false);
  });

  it('errors when target does not start with @', async () => {
    const r = await cmd.execute(ctx(['set', 'x', 'value'], store));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid variable name/);
  });

  it('errors on bare `@` target', async () => {
    const r = await cmd.execute(ctx(['set', '@', 'value'], store));
    expect(r.success).toBe(false);
  });

  it('errors on missing value', async () => {
    const r = await cmd.execute(ctx(['set', '@x'], store));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Missing value/);
  });

  it('errors when preprocess store is missing from context', async () => {
    // Standalone usage without the store should fail loud, not
    // silently swallow the user's set.
    const r = await cmd.execute(ctx(['set', '@x', '1'], undefined));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/store unavailable/);
  });
});

// ─── SetsCommand (reactive var setter) ───────────────────────────────

describe('SetsCommand', () => {
  let cmd: SetsCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new SetsCommand();
    store = new MemoryPreprocessStore();
  });

  it('stores the source verbatim (not a snapshot)', async () => {
    await cmd.execute(ctx(['sets', '@spread', '$ask-$bid'], store));
    expect(store.getLiveVar('spread')).toBe('$ask-$bid');
  });

  it('joins multi-token sources with a single space (anchor + offset shape)', async () => {
    await cmd.execute(ctx(['sets', '@above', '$entry', '+1%'], store));
    expect(store.getLiveVar('above')).toBe('$entry +1%');
  });

  it('overwrites an existing live var', async () => {
    store.setLiveVar('x', '$last');
    await cmd.execute(ctx(['sets', '@x', '$mark'], store));
    expect(store.getLiveVar('x')).toBe('$mark');
  });

  it('clears a same-name static var (mutex via storage)', async () => {
    store.setVar('x', '42');
    await cmd.execute(ctx(['sets', '@x', '$ask'], store));
    expect(store.getVar('x')).toBeUndefined();
    expect(store.getLiveVar('x')).toBe('$ask');
  });

  it('errors on missing target', async () => {
    const r = await cmd.execute(ctx(['sets'], store));
    expect(r.success).toBe(false);
  });

  it('errors when target does not start with @', async () => {
    const r = await cmd.execute(ctx(['sets', 'x', '$last'], store));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid variable name/);
  });

  it('errors on bare `@` target', async () => {
    const r = await cmd.execute(ctx(['sets', '@', '$last'], store));
    expect(r.success).toBe(false);
  });

  it('errors on missing source', async () => {
    const r = await cmd.execute(ctx(['sets', '@x'], store));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Missing source/);
  });

  it('rejects unrecognised source tokens at definition time', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['sets', '@x', 'garbage'], store, adapter));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/unrecognised source token 'garbage'/);
    // Make sure NO binding was created — the user gets an explicit
    // rejection rather than a corrupt half-state.
    expect(store.getLiveVar('x')).toBeUndefined();
    expect(adapter.printError).toHaveBeenCalledWith(expect.stringMatching(/unrecognised source token 'garbage'/));
  });

  it('rejects size-shaped tokens that the substitute layer leaves intact', async () => {
    // `100$` would reach the substitute layer literal and the source
    // would never evaluate to a single number. Catch it early.
    const r = await cmd.execute(ctx(['sets', '@x', '100$'], store));
    expect(r.success).toBe(false);
    expect(store.getLiveVar('x')).toBeUndefined();
  });

  it('errors when preprocess store is missing from context', async () => {
    const r = await cmd.execute(ctx(['sets', '@x', '$last'], undefined));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/store unavailable/);
  });
});

// ─── TrackCommand (max/min aggregator setter) ────────────────────────

describe('TrackCommand', () => {
  let cmd: TrackCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new TrackCommand();
    store = new MemoryPreprocessStore();
  });

  describe('track max / track min', () => {
    it('registers a max tracker with the source verbatim', async () => {
      await cmd.execute(ctx(['track', 'max', '@hi', '$pnl'], store));
      expect(store.getTrackedVar('hi')).toEqual({
        kind: 'max',
        source: '$pnl',
        value: undefined,
      });
    });

    it('registers a min tracker', async () => {
      await cmd.execute(ctx(['track', 'min', '@lo', '$pnl'], store));
      expect(store.getTrackedVar('lo')).toMatchObject({ kind: 'min', source: '$pnl' });
    });

    it('joins multi-token sources', async () => {
      await cmd.execute(ctx(['track', 'max', '@spread', '$ask-$bid'], store));
      expect(store.getTrackedVar('spread')).toMatchObject({ source: '$ask-$bid' });
    });

    it('clears a same-name static var (mutex)', async () => {
      store.setVar('x', '42');
      await cmd.execute(ctx(['track', 'max', '@x', '$pnl'], store));
      expect(store.getVar('x')).toBeUndefined();
      expect(store.getTrackedVar('x')).toMatchObject({ kind: 'max' });
    });

    it('clears a same-name live var (mutex)', async () => {
      store.setLiveVar('x', '$ask');
      await cmd.execute(ctx(['track', 'max', '@x', '$pnl'], store));
      expect(store.getLiveVar('x')).toBeUndefined();
      expect(store.getTrackedVar('x')).toMatchObject({ kind: 'max' });
    });

    it('re-registering resets the value', async () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      store.updateTrackedVar('hi', 100);
      await cmd.execute(ctx(['track', 'max', '@hi', '$mark'], store));
      expect(store.getTrackedVar('hi')).toEqual({
        kind: 'max',
        source: '$mark',
        value: undefined,
      });
    });

    it('errors on invalid subcommand', async () => {
      const r = await cmd.execute(ctx(['track', 'banana', '@hi', '$pnl'], store));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Invalid subcommand/);
    });

    it('errors on missing target', async () => {
      const r = await cmd.execute(ctx(['track', 'max'], store));
      expect(r.success).toBe(false);
    });

    it('errors when target does not start with @', async () => {
      const r = await cmd.execute(ctx(['track', 'max', 'hi', '$pnl'], store));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Invalid variable name/);
    });

    it('errors on missing source', async () => {
      const r = await cmd.execute(ctx(['track', 'max', '@hi'], store));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Missing source/);
    });

    it('rejects unrecognised source tokens at definition time', async () => {
      const adapter = makeAdapter();
      const r = await cmd.execute(ctx(['track', 'max', '@hi', 'garbage'], store, adapter));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/unrecognised source token 'garbage'/);
      expect(store.getTrackedVar('hi')).toBeUndefined();
    });

    it('errors when preprocess store is missing from context', async () => {
      const r = await cmd.execute(ctx(['track', 'max', '@hi', '$pnl'], undefined));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/store unavailable/);
    });
  });

  describe('track list', () => {
    it('"(no trackers set)" when empty', async () => {
      const adapter = makeAdapter();
      await cmd.execute(ctx(['track', 'list'], store, adapter));
      expect(adapter.printInfo).toHaveBeenCalledWith('(no trackers set)');
    });

    it('prints registered trackers sorted alphabetically', async () => {
      store.setTrackedVar('z', 'max', '$pnl');
      store.setTrackedVar('a', 'min', '$ask');
      store.updateTrackedVar('z', 100);
      const adapter = makeAdapter();
      await cmd.execute(ctx(['track', 'list'], store, adapter));
      expect(adapter.print).toHaveBeenCalledWith([
        '@a (track min) = $ask  →  (unseeded)',
        '@z (track max) = $pnl  →  100',
      ]);
    });
  });
});

// ─── UntrackCommand ──────────────────────────────────────────────────

describe('UntrackCommand', () => {
  let cmd: UntrackCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new UntrackCommand();
    store = new MemoryPreprocessStore();
  });

  it('removes a tracker', async () => {
    store.setTrackedVar('hi', 'max', '$pnl');
    await cmd.execute(ctx(['untrack', '@hi'], store));
    expect(store.getTrackedVar('hi')).toBeUndefined();
  });

  it("warns when target isn't a tracker", async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['untrack', '@missing'], store, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printWarning).toHaveBeenCalledWith('@missing is not a tracker');
  });

  it('does NOT touch a same-name static or live var', async () => {
    // The storage mutex prevents both from coexisting in normal use,
    // but verify the command only acts on trackers regardless.
    store.setVar('x', '42');
    const r = await cmd.execute(ctx(['untrack', '@x'], store));
    expect(r.success).toBe(true);
    expect(store.getVar('x')).toBe('42');
  });

  it('"untrack all" clears every tracker (but not static / live)', async () => {
    store.setTrackedVar('a', 'max', '$pnl');
    store.setTrackedVar('b', 'min', '$ask');
    store.setVar('keep', '99');
    store.setLiveVar('keepLive', '$last');
    await cmd.execute(ctx(['untrack', 'all'], store));
    expect(store.getTrackedVars()).toEqual({});
    expect(store.getVar('keep')).toBe('99');
    expect(store.getLiveVar('keepLive')).toBe('$last');
  });

  it('errors on missing target', async () => {
    const r = await cmd.execute(ctx(['untrack'], store));
    expect(r.success).toBe(false);
  });

  it('errors when preprocess store is missing', async () => {
    const r = await cmd.execute(ctx(['untrack', '@x'], undefined));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/store unavailable/);
  });
});

// ─── UnsetCommand ────────────────────────────────────────────────────

describe('UnsetCommand', () => {
  let cmd: UnsetCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new UnsetCommand();
    store = new MemoryPreprocessStore();
  });

  it('removes a single var', async () => {
    store.setVar('x', '1');
    await cmd.execute(ctx(['unset', '@x'], store));
    expect(store.getVar('x')).toBeUndefined();
  });

  it("warns when the target doesn't exist (still success)", async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['unset', '@missing'], store, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printWarning).toHaveBeenCalledWith('@missing is not set');
  });

  it('"unset all" wipes every var', async () => {
    store.setVar('a', '1');
    store.setVar('b', '2');
    await cmd.execute(ctx(['unset', 'all'], store));
    expect(store.getVars()).toEqual({});
  });

  it('errors on missing target', async () => {
    const r = await cmd.execute(ctx(['unset'], store));
    expect(r.success).toBe(false);
  });

  it('errors on a bare word that is not "all" and not @x', async () => {
    const r = await cmd.execute(ctx(['unset', 'something'], store));
    expect(r.success).toBe(false);
  });

  describe('reactive (live) vars', () => {
    it('removes a live var', async () => {
      store.setLiveVar('spread', '$ask-$bid');
      await cmd.execute(ctx(['unset', '@spread'], store));
      expect(store.getLiveVar('spread')).toBeUndefined();
    });

    it('treats live and static uniformly — same target works for either kind', async () => {
      // Same target string, different bindings — both removed by the
      // same command without the user needing to know which kind it was.
      store.setVar('a', '1');
      store.setLiveVar('b', '$ask');
      const r1 = await cmd.execute(ctx(['unset', '@a'], store));
      const r2 = await cmd.execute(ctx(['unset', '@b'], store));
      expect(r1.success).toBe(true);
      expect(r2.success).toBe(true);
      expect(store.getVar('a')).toBeUndefined();
      expect(store.getLiveVar('b')).toBeUndefined();
    });

    it('"unset all" wipes ALL THREE kinds — static + reactive + tracked', async () => {
      store.setVar('a', '1');
      store.setLiveVar('b', '$ask');
      store.setTrackedVar('c', 'max', '$pnl');
      await cmd.execute(ctx(['unset', 'all'], store));
      expect(store.getVars()).toEqual({});
      expect(store.getLiveVars()).toEqual({});
      expect(store.getTrackedVars()).toEqual({});
    });

    it('`unset @<tracker>` removes a tracker (third kind)', async () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      await cmd.execute(ctx(['unset', '@hi'], store));
      expect(store.getTrackedVar('hi')).toBeUndefined();
    });

    it('warns when @target is bound nowhere (success result, just a warning)', async () => {
      const adapter = makeAdapter();
      const r = await cmd.execute(ctx(['unset', '@missing'], store, adapter));
      expect(r.success).toBe(true);
      expect(adapter.printWarning).toHaveBeenCalledWith('@missing is not set');
    });
  });
});

// ─── PrintCommand ────────────────────────────────────────────────────

describe('PrintCommand', () => {
  let cmd: PrintCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new PrintCommand();
    store = new MemoryPreprocessStore();
  });

  it('prints a single var value via print(adapter)', async () => {
    store.setVar('x', '100$');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['print', '@x'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['@x = 100$']);
  });

  it('warns when the target var is missing', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['print', '@nope'], store, adapter));
    expect(r.success).toBe(true); // unset/missing is still success
    expect(adapter.printWarning).toHaveBeenCalledWith('@nope is not set');
  });

  it('"print all" prints every var, sorted', async () => {
    store.setVar('z', '3');
    store.setVar('a', '1');
    store.setVar('m', '2');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['print', 'all'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['@a = 1', '@m = 2', '@z = 3']);
  });

  it('"print all" shows "(no variables set)" when store is empty', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['print', 'all'], store, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('(no variables set)');
  });

  it('errors on missing target', async () => {
    const r = await cmd.execute(ctx(['print'], store));
    expect(r.success).toBe(false);
  });

  it('"print list" is a synonym for "print all"', async () => {
    store.setVar('a', '1');
    store.setVar('b', '2');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['print', 'list'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['@a = 1', '@b = 2']);
  });

  it('"vars" (no args) lists all variables', async () => {
    store.setVar('a', '1');
    store.setVar('b', '2');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['vars'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['@a = 1', '@b = 2']);
  });

  it('"vars list" also lists all variables', async () => {
    store.setVar('x', '99');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['vars', 'list'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['@x = 99']);
  });

  it('"vars @x" prints a single var (mirror of "print @x")', async () => {
    store.setVar('y', 'hello');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['vars', '@y'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['@y = hello']);
  });

  it('exposes "vars" as an alias', () => {
    expect(cmd.aliases).toContain('vars');
  });

  describe('reactive (live) vars', () => {
    function makeCtxWithExchange(
      args: string[],
      tickers: Array<{ symbol: string; last?: number; bid?: number; ask?: number; mark?: number }>,
      store: MemoryPreprocessStore,
      adapter: ReturnType<typeof makeAdapter>,
    ): CommandContext {
      const exchange = {
        store: { positions: [], orders: [], markets: [], tickers },
      } as unknown as CommandContext['exchange'];
      return {
        raw: args.join(' '),
        args: { _: args },
        symbol: 'BTCUSDT',
        accountName: 'a',
        exchange,
        adapter,
        preprocessStore: store,
      } as unknown as CommandContext;
    }

    it('`print @x` for a live var shows source AND current value', async () => {
      store.setLiveVar('x', '$last');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', '@x'], [{ symbol: 'BTCUSDT', last: 50_000 }], store, adapter);
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith(['@x (live) = $last  →  50000']);
    });

    it('`print @x` for a math source evaluates correctly', async () => {
      store.setLiveVar('spread', '$ask-$bid');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(
        ['print', '@spread'],
        [{ symbol: 'BTCUSDT', bid: 49_990, ask: 50_010 }],
        store,
        adapter,
      );
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith(['@spread (live) = $ask-$bid  →  20']);
    });

    it('`print @x` for a live var with a broken source surfaces the error inline', async () => {
      // Source references $entry but no position exists. Evaluation
      // throws — print should show the error in the trailer, not
      // bail out.
      store.setLiveVar('above', '$entry +1%');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', '@above'], [], store, adapter);
      await cmd.execute(c);
      // The redundant "live var @above source 'X':" prefix is stripped.
      const printedLines = adapter.print.mock.calls[0]?.[0] as string[];
      expect(printedLines).toHaveLength(1);
      expect(printedLines[0]).toMatch(/^@above \(live\) = \$entry \+1%  →  \(.+no position.+\)$/i);
    });

    it('`print all` interleaves live and static, sorted alphabetically', async () => {
      store.setVar('a', '1');
      store.setLiveVar('b', '$last');
      store.setVar('c', '3');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', 'all'], [{ symbol: 'BTCUSDT', last: 50_000 }], store, adapter);
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith(['@a = 1', '@b (live) = $last  →  50000', '@c = 3']);
    });

    it('`print all` with only live vars still lists them', async () => {
      store.setLiveVar('x', '$last');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', 'all'], [{ symbol: 'BTCUSDT', last: 50_000 }], store, adapter);
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith(['@x (live) = $last  →  50000']);
    });

    it('`print all` shows "(no variables set)" only when BOTH kinds are empty', async () => {
      // Sanity-check: setting only a live var should NOT trigger the
      // empty-list message.
      store.setLiveVar('x', '$last');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', 'all'], [{ symbol: 'BTCUSDT', last: 50_000 }], store, adapter);
      await cmd.execute(c);
      expect(adapter.printInfo).not.toHaveBeenCalled();
      expect(adapter.print).toHaveBeenCalled();
    });

    it('precedence — when only a live binding exists, `print @x` shows it (no warn)', async () => {
      // The static fallback path's "not set" warning must not fire when
      // a live binding exists.
      store.setLiveVar('x', '$last');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', '@x'], [{ symbol: 'BTCUSDT', last: 50_000 }], store, adapter);
      await cmd.execute(c);
      expect(adapter.printWarning).not.toHaveBeenCalled();
      expect(adapter.print).toHaveBeenCalled();
    });
  });

  describe('tracked vars', () => {
    function makeCtxWithExchange(
      args: string[],
      tickers: Array<{ symbol: string; last?: number; bid?: number; ask?: number; mark?: number }>,
      store: MemoryPreprocessStore,
      adapter: ReturnType<typeof makeAdapter>,
    ): CommandContext {
      const exchange = {
        store: { positions: [], orders: [], markets: [], tickers },
      } as unknown as CommandContext['exchange'];
      return {
        raw: args.join(' '),
        args: { _: args },
        symbol: 'BTCUSDT',
        accountName: 'a',
        exchange,
        adapter,
        preprocessStore: store,
      } as unknown as CommandContext;
    }

    it('`print @<tracker>` shows kind, source, and CURRENT stored value (no re-eval)', async () => {
      // `print` is informational — it must NOT mutate the tracker.
      // Showing the stored value (which is undefined before any
      // resolution) is what the user wants.
      store.setTrackedVar('hi', 'max', '$last');
      store.updateTrackedVar('hi', 50_000); // simulate prior resolution
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(
        ['print', '@hi'],
        [{ symbol: 'BTCUSDT', last: 99_999 }], // doesn't matter — print doesn't re-eval
        store,
        adapter,
      );
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith(['@hi (track max) = $last  →  50000']);
      // Stored value unchanged — print didn't trigger the resolver.
      expect(store.getTrackedVar('hi')!.value).toBe(50_000);
    });

    it('`print @<tracker>` shows `(unseeded)` when no resolution has happened yet', async () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', '@hi'], [], store, adapter);
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith(['@hi (track max) = $pnl  →  (unseeded)']);
    });

    it('`print all` interleaves trackers with static + live, alphabetically', async () => {
      store.setVar('a', '1');
      store.setLiveVar('b', '$last');
      store.setTrackedVar('c', 'max', '$pnl');
      store.updateTrackedVar('c', 42);
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', 'all'], [{ symbol: 'BTCUSDT', last: 50_000 }], store, adapter);
      await cmd.execute(c);
      expect(adapter.print).toHaveBeenCalledWith([
        '@a = 1',
        '@b (live) = $last  →  50000',
        '@c (track max) = $pnl  →  42',
      ]);
    });

    it('precedence — tracker wins over live and static in `print @x`', async () => {
      // The mutex makes this impossible to construct via the public
      // API, but the precedence is locked in regardless.
      store.setTrackedVar('x', 'min', '$pnl');
      const adapter = makeAdapter();
      const c = makeCtxWithExchange(['print', '@x'], [], store, adapter);
      await cmd.execute(c);
      const printed = adapter.print.mock.calls[0][0] as string[];
      expect(printed[0]).toMatch(/^@x \(track min\)/);
    });
  });
});

// ─── AliasCommand ────────────────────────────────────────────────────

describe('AliasCommand', () => {
  let cmd: AliasCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new AliasCommand();
    store = new MemoryPreprocessStore();
  });

  it('"alias <name> <body>" stores the body verbatim', async () => {
    await cmd.execute(ctx(['alias', 'b1', 'buy', '100$'], store));
    expect(store.getAlias('b1')).toBe('buy 100$');
  });

  it('strips a single layer of outer quotes from the body', async () => {
    // The user types alias x "buy 1; sell 1" — at this point the
    // chain tokenizer has already preserved the quotes around the
    // body. AliasCommand's job is to strip the outer pair so the
    // stored body doesn't contain the literal " characters.
    await cmd.execute(ctx(['alias', 'b1', '"buy 100$; sell 100$"'], store));
    expect(store.getAlias('b1')).toBe('buy 100$; sell 100$');
  });

  it('handles single-quoted bodies symmetrically', async () => {
    await cmd.execute(ctx(['alias', 'b1', "'buy 100$'"], store));
    expect(store.getAlias('b1')).toBe('buy 100$');
  });

  it('"alias list" prints every alias sorted', async () => {
    store.setAlias('z', 'sell');
    store.setAlias('a', 'buy');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['alias', 'list'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['a = buy', 'z = sell']);
  });

  it('"alias" with no args is treated as "list"', async () => {
    store.setAlias('a', 'buy');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['alias'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['a = buy']);
  });

  it('"alias list" with empty store prints "(no aliases defined)"', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['alias', 'list'], store, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('(no aliases defined)');
  });

  it.each([['1bad'], ['has-dash'], ['has space'], ['$dollar'], ['@at']])(
    'rejects malformed alias name "%s"',
    async (name) => {
      const r = await cmd.execute(ctx(['alias', name, 'buy'], store));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Invalid alias name/);
    },
  );

  it.each([['ok'], ['_starts_underscore'], ['CamelCase'], ['mix_123']])('accepts alias name "%s"', async (name) => {
    const r = await cmd.execute(ctx(['alias', name, 'buy'], store));
    expect(r.success).toBe(true);
    expect(store.getAlias(name)).toBe('buy');
  });

  it('errors when the body is empty', async () => {
    const r = await cmd.execute(ctx(['alias', 'b1'], store));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Missing alias body/);
  });

  it('errors when preprocess store is missing', async () => {
    const r = await cmd.execute(ctx(['alias', 'b1', 'buy'], undefined));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/store unavailable/);
  });

  it('overwrites an existing alias', async () => {
    store.setAlias('b1', 'buy 1$');
    await cmd.execute(ctx(['alias', 'b1', 'buy', '2$'], store));
    expect(store.getAlias('b1')).toBe('buy 2$');
  });
});

// ─── UnaliasCommand ──────────────────────────────────────────────────

describe('UnaliasCommand', () => {
  let cmd: UnaliasCommand;
  let store: MemoryPreprocessStore;
  beforeEach(() => {
    cmd = new UnaliasCommand();
    store = new MemoryPreprocessStore();
  });

  it('removes a single alias', async () => {
    store.setAlias('b1', 'buy 1$');
    await cmd.execute(ctx(['unalias', 'b1'], store));
    expect(store.getAlias('b1')).toBeUndefined();
  });

  it('warns on a missing alias (still success)', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['unalias', 'nope'], store, adapter));
    expect(adapter.printWarning).toHaveBeenCalledWith('nope is not an alias');
  });

  it('"unalias all" wipes every alias', async () => {
    store.setAlias('a', 'buy');
    store.setAlias('b', 'sell');
    await cmd.execute(ctx(['unalias', 'all'], store));
    expect(store.getAliases()).toEqual({});
  });

  it('errors on missing target', async () => {
    const r = await cmd.execute(ctx(['unalias'], store));
    expect(r.success).toBe(false);
  });
});

// ─── SleepCommand ────────────────────────────────────────────────────

describe('SleepCommand', () => {
  let cmd: SleepCommand;
  beforeEach(() => {
    cmd = new SleepCommand();
    vi.useFakeTimers();
  });

  it('awaits the requested duration (ms)', async () => {
    const promise = cmd.execute(ctx(['sleep', '250ms']));
    let done = false;
    promise.then(() => {
      done = true;
    });

    await vi.advanceTimersByTimeAsync(249);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await promise;
    expect(done).toBe(true);
  });

  it('treats a bare number as seconds', async () => {
    const promise = cmd.execute(ctx(['sleep', '5']));
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(4_999);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await promise;
    expect(done).toBe(true);
  });

  it('errors on missing duration', async () => {
    vi.useRealTimers();
    const r = await cmd.execute(ctx(['sleep']));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Missing duration/);
  });

  it('errors on garbage', async () => {
    vi.useRealTimers();
    const r = await cmd.execute(ctx(['sleep', 'forever']));
    expect(r.success).toBe(false);
  });

  it('errors when over the 10-minute cap', async () => {
    vi.useRealTimers();
    const r = await cmd.execute(ctx(['sleep', '11m']));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/10 minute cap/);
  });
});

// ─── ClearCommand ────────────────────────────────────────────────────

describe('ClearCommand', () => {
  let cmd: ClearCommand;
  let writeSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    cmd = new ClearCommand();
    writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
  });

  it('writes the ANSI clear-screen + cursor-home sequence', async () => {
    await cmd.execute(ctx(['clear']));
    expect(writeSpy).toHaveBeenCalledWith('\x1b[2J\x1b[H');
  });

  it('aliases include "cls"', () => {
    expect(cmd.aliases).toContain('cls');
  });

  it('does not require connection', () => {
    expect(cmd.requiresConnection).toBe(false);
  });
});

// ─── WhitelistCommand / UnwhitelistCommand ───────────────────────────

describe('WhitelistCommand', () => {
  let cmd: WhitelistCommand;
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    cmd = new WhitelistCommand();
    store = new MemoryPreprocessStore();
  });

  it('"whitelist" with no args lists nothing when empty', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['whitelist'], store, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('(whitelist is empty)');
  });

  it('"whitelist BTCUSDT" adds the symbol (uppercased)', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['whitelist', 'btcusdt'], store, adapter));
    expect(store.getWhitelist()).toEqual(['BTCUSDT']);
    expect(adapter.printSuccess).toHaveBeenCalledWith('Whitelisted BTCUSDT');
  });

  it('"whitelist <existing>" reports already-whitelisted softly', async () => {
    store.addToWhitelist('BTCUSDT');
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['whitelist', 'btcusdt'], store, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printInfo).toHaveBeenCalledWith('BTCUSDT is already whitelisted');
  });

  it('"whitelist" with no args lists each symbol on its own row', async () => {
    store.addToWhitelist('BTCUSDT');
    store.addToWhitelist('ETHUSDT');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['whitelist'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['  BTCUSDT', '  ETHUSDT']);
  });

  it('errors when preprocessStore is missing', async () => {
    const adapter = makeAdapter();
    const c = ctx(['whitelist', 'BTCUSDT'], undefined, adapter);
    const r = await cmd.execute(c);
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/preprocess store/);
  });
});

describe('UnwhitelistCommand', () => {
  let cmd: UnwhitelistCommand;
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    cmd = new UnwhitelistCommand();
    store = new MemoryPreprocessStore();
  });

  it('removes a symbol', async () => {
    store.addToWhitelist('BTCUSDT');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['unwhitelist', 'btcusdt'], store, adapter));
    expect(store.getWhitelist()).toEqual([]);
    expect(adapter.printSuccess).toHaveBeenCalledWith('Removed BTCUSDT from whitelist');
  });

  it('warns softly when removing a non-whitelisted symbol', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['unwhitelist', 'BTCUSDT'], store, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printWarning).toHaveBeenCalledWith('BTCUSDT was not whitelisted');
  });

  it('"unwhitelist all" clears the whole list', async () => {
    store.addToWhitelist('BTCUSDT');
    store.addToWhitelist('ETHUSDT');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['unwhitelist', 'all'], store, adapter));
    expect(store.getWhitelist()).toEqual([]);
    expect(adapter.printSuccess).toHaveBeenCalledWith('Cleared whitelist (2 symbols)');
  });

  it('errors with usage hint when no target', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['unwhitelist'], store, adapter));
    expect(r.success).toBe(false);
    expect(adapter.printError).toHaveBeenCalledWith(expect.stringMatching(/Usage: unwhitelist/));
  });
});

// ─── FatfingerCommand / UnfatfingerCommand ──────────────────────────

describe('FatfingerCommand', () => {
  let cmd: FatfingerCommand;
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    cmd = new FatfingerCommand();
    store = new MemoryPreprocessStore();
  });

  it('"fatfinger" with no args lists nothing when empty', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['fatfinger'], store, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('(no fatfinger limits set)');
  });

  it('"fatfinger 10" sets the focused symbol', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['fatfinger', '10'], store, adapter));
    expect(store.getFatfinger('BTCUSDT')).toBe(10);
    expect(adapter.printSuccess).toHaveBeenCalledWith('Fatfinger BTCUSDT: 10');
  });

  it('"fatfinger 0.5 ETHUSDT" sets a specific symbol', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['fatfinger', '0.5', 'ethusdt'], store, adapter));
    expect(store.getFatfinger('ETHUSDT')).toBe(0.5);
  });

  it('"fatfinger" with no args lists set limits sorted', async () => {
    store.setFatfinger('ETHUSDT', 5);
    store.setFatfinger('BTCUSDT', 0.1);
    const adapter = makeAdapter();
    await cmd.execute(ctx(['fatfinger'], store, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['  BTCUSDT: 0.1', '  ETHUSDT: 5']);
  });

  it.each(['0', '-5', 'abc'])('rejects invalid contract size %s', async (n) => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['fatfinger', n], store, adapter));
    expect(r.success).toBe(false);
    expect(store.getFatfinger('BTCUSDT')).toBeUndefined();
  });
});

describe('UnfatfingerCommand', () => {
  let cmd: UnfatfingerCommand;
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    cmd = new UnfatfingerCommand();
    store = new MemoryPreprocessStore();
  });

  it('removes a specific limit', async () => {
    store.setFatfinger('BTCUSDT', 10);
    const adapter = makeAdapter();
    await cmd.execute(ctx(['unfatfinger', 'btcusdt'], store, adapter));
    expect(store.getFatfinger('BTCUSDT')).toBeUndefined();
  });

  it('"unfatfinger all" clears every limit', async () => {
    store.setFatfinger('BTCUSDT', 10);
    store.setFatfinger('ETHUSDT', 5);
    const adapter = makeAdapter();
    await cmd.execute(ctx(['unfatfinger', 'all'], store, adapter));
    expect(store.getFatfingers()).toEqual({});
    expect(adapter.printSuccess).toHaveBeenCalledWith('Cleared 2 fatfinger limits');
  });

  it('warns softly when removing a symbol with no limit', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['unfatfinger', 'BTCUSDT'], store, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printWarning).toHaveBeenCalledWith('BTCUSDT has no fatfinger limit');
  });
});

// ─── SimulationCommand ──────────────────────────────────────────────

describe('SimulationCommand', () => {
  let cmd: SimulationCommand;
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    cmd = new SimulationCommand();
    store = new MemoryPreprocessStore();
  });

  it('default state is OFF', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['simulation'], store, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('simulation: OFF (live)');
  });

  it('"simulation on" flips and persists the flag', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['simulation', 'on'], store, adapter));
    expect(store.getSimulation()).toBe(true);
    expect(adapter.printSuccess).toHaveBeenCalledWith('simulation ON — write commands will log + skip');
  });

  it('"simulation off" flips back', async () => {
    store.setSimulation(true);
    const adapter = makeAdapter();
    await cmd.execute(ctx(['simulation', 'off'], store, adapter));
    expect(store.getSimulation()).toBe(false);
  });

  it.each(['on', 'true', '1'])('accepts "%s" as truthy', async (val) => {
    await cmd.execute(ctx(['simulation', val], store));
    expect(store.getSimulation()).toBe(true);
  });

  it.each(['off', 'false', '0'])('accepts "%s" as falsy', async (val) => {
    store.setSimulation(true);
    await cmd.execute(ctx(['simulation', val], store));
    expect(store.getSimulation()).toBe(false);
  });

  it('errors on unknown argument', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['simulation', 'maybe'], store, adapter));
    expect(r.success).toBe(false);
    expect(adapter.printError).toHaveBeenCalledWith(expect.stringMatching(/Usage: simulation/));
  });

  it('"sim status" reports current state', async () => {
    store.setSimulation(true);
    const adapter = makeAdapter();
    await cmd.execute(ctx(['sim', 'status'], store, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('simulation: ON (dry-run)');
  });

  it('exposes "sim" and "dry-run" as aliases', () => {
    expect(cmd.aliases).toEqual(expect.arrayContaining(['sim', 'dry-run']));
  });
});

// ─── HistoryCommand ──────────────────────────────────────────────────

describe('HistoryCommand', () => {
  let cmd: HistoryCommand;
  beforeEach(() => {
    cmd = new HistoryCommand();
  });

  it('reports "history not available" when the adapter does not implement it', async () => {
    const adapter = makeAdapter();
    // Default adapter has no getCommandHistory.
    expect((adapter as Record<string, unknown>).getCommandHistory).toBeUndefined();
    const r = await cmd.execute(ctx(['history'], undefined, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printInfo).toHaveBeenCalledWith('history not available in this runtime');
  });

  it('prints the adapter-provided list (newest first)', async () => {
    const adapter = makeAdapter();
    (adapter as unknown as { getCommandHistory: (n?: number) => string[] }).getCommandHistory = () => [
      'buy 100$',
      'cancel',
      'positions',
    ];
    await cmd.execute(ctx(['history'], undefined, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['buy 100$', 'cancel', 'positions']);
  });

  it('passes the user-supplied N through to the adapter', async () => {
    const adapter = makeAdapter();
    const spy = vi.fn((limit?: number) => Array(limit ?? 50).fill('cmd'));
    (adapter as unknown as { getCommandHistory: (n?: number) => string[] }).getCommandHistory = spy;
    await cmd.execute(ctx(['history', '5'], undefined, adapter));
    expect(spy).toHaveBeenCalledWith(5);
  });

  it('caps N at 200', async () => {
    const adapter = makeAdapter();
    const spy = vi.fn((limit?: number) => Array(limit ?? 50).fill('cmd'));
    (adapter as unknown as { getCommandHistory: (n?: number) => string[] }).getCommandHistory = spy;
    await cmd.execute(ctx(['history', '999'], undefined, adapter));
    expect(spy).toHaveBeenCalledWith(200);
  });

  it('errors on non-positive N', async () => {
    const adapter = makeAdapter();
    (adapter as unknown as { getCommandHistory: (n?: number) => string[] }).getCommandHistory = () => [];
    const r1 = await cmd.execute(ctx(['history', '0'], undefined, adapter));
    expect(r1.success).toBe(false);
    const r2 = await cmd.execute(ctx(['history', '-5'], undefined, adapter));
    expect(r2.success).toBe(false);
    const r3 = await cmd.execute(ctx(['history', 'banana'], undefined, adapter));
    expect(r3.success).toBe(false);
  });

  it('prints "(no history)" when the source is empty', async () => {
    const adapter = makeAdapter();
    (adapter as unknown as { getCommandHistory: (n?: number) => string[] }).getCommandHistory = () => [];
    await cmd.execute(ctx(['history'], undefined, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('(no history)');
  });

  it('does not require a connection', () => {
    expect(cmd.requiresConnection).toBe(false);
  });
});

// ─── BudgetCommand ───────────────────────────────────────────────────
describe('BudgetCommand', () => {
  let cmd: BudgetCommand;
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    cmd = new BudgetCommand();
    store = new MemoryPreprocessStore();
  });

  it('sets a cap from a $-prefixed amount', async () => {
    const r = await cmd.execute(ctx(['budget', '$5000'], store));
    expect(r.success).toBe(true);
    expect(store.getBudget()).toBe(5000);
  });

  it('sets a cap from a bare number', async () => {
    await cmd.execute(ctx(['budget', '2500'], store));
    expect(store.getBudget()).toBe(2500);
  });

  it('`budget off` disables the cap', async () => {
    store.setBudget(1000);
    await cmd.execute(ctx(['budget', 'off'], store));
    expect(store.getBudget()).toBeUndefined();
  });

  it('`budget reset` zeroes the spent accumulator but keeps the cap', async () => {
    store.setBudget(1000);
    store.addBudgetSpent(400);
    await cmd.execute(ctx(['budget', 'reset'], store));
    expect(store.getBudgetSpent()).toBe(0);
    expect(store.getBudget()).toBe(1000);
  });

  it('rejects a malformed amount', async () => {
    const r = await cmd.execute(ctx(['budget', 'lots'], store));
    expect(r.success).toBe(false);
    expect(store.getBudget()).toBeUndefined();
  });

  it('rejects a partially-numeric amount (no silent parseFloat)', async () => {
    const r = await cmd.execute(ctx(['budget', '5000abc'], store));
    expect(r.success).toBe(false);
    expect(store.getBudget()).toBeUndefined();
  });

  it('bare `budget` reports status without changing it', async () => {
    store.setBudget(1000);
    const r = await cmd.execute(ctx(['budget'], store));
    expect(r.success).toBe(true);
    expect(store.getBudget()).toBe(1000);
  });

  it('does not require a connection', () => {
    expect(cmd.requiresConnection).toBe(false);
  });
});
