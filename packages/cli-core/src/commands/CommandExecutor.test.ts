import { describe, it, expect, vi } from 'vitest';
import { markErrorReported } from '@tealstreet/safe-cex/src/commandOutcome';
import { executeCommand } from './CommandExecutor';
import { preprocessCommand } from '../preprocess/preprocess';
import { MemoryPreprocessStore } from '../preprocess/memoryStore';
import type { ExchangeLike, CLIPlatformAdapter } from '../types';

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

function makeExchange(): ExchangeLike {
  return {
    name: 'test-exchange',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
      ],
      tickers: [],
      orders: [],
      positions: [],
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
  } as unknown as ExchangeLike;
}

describe('executeCommand — exchange-reported order failures', () => {
  it.each(['reported', 'frozen', 'unreported'] as const)('prints the Buy failure result even when the exchange reported it (%s)', async (mode) => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [{ symbol: 'BTCUSDT', last: 50_000 }];
    const error = Object.assign(new Error('Reconcile before retrying'), {
      code: 'COMMAND_TIMEOUT', command: 'placeOrders', outcome: 'unknown', clientOrderId: 'requested',
      actualClientOrderIds: ['actual'], partialOrderIds: ['accepted'],
    });
    if (mode === 'frozen') Object.freeze(error);
    const placeOrder = vi.fn().mockImplementation(async () => {
      if (mode !== 'unreported') {
        // The standalone exchange error listener or worker error event prints
        // before the awaited Buy rejection reaches the executor boundary.
        markErrorReported(error);
        adapter.printError(error.message);
      }
      throw error;
    });
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = placeOrder;
    const result = await executeCommand('buy $100', 'BTCUSDT', undefined, exchange, adapter);
    expect(placeOrder).toHaveBeenCalledTimes(1);
    // The exchange error listener filters and deduplicates (suppressed categories,
    // once-per-account auth, 10s console dedupe), so the result line always prints.
    expect(adapter.printError).toHaveBeenCalledWith(`Command failed: ${error.message}`);
    expect(adapter.printError).toHaveBeenCalledTimes(mode === 'unreported' ? 1 : 2);
    expect(result).toMatchObject({ handled: true, success: false, error: error.message, errorReported: true, errorDetails: {
      code: 'COMMAND_TIMEOUT', command: 'placeOrders', outcome: 'unknown', clientOrderId: 'requested',
      actualClientOrderIds: ['actual'], partialOrderIds: ['accepted'],
    } });
    if (mode !== 'unreported') expect(result.errorDetails?.errorReported).toBe(true);
  });
});

describe('executeCommand — bare-instrument shorthand', () => {
  it('treats `BTCUSDT` as `switch BTCUSDT` when the market exists', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();

    const result = await executeCommand('BTCUSDT', 'ETHUSDT', undefined, exchange, adapter);
    expect(result.handled).toBe(true);
    expect(result.sessionChanged).toBe(true);
    expect(result.newSession?.currentSymbol).toBe('BTCUSDT');
  });

  it('matches case-insensitively', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    const result = await executeCommand('btcusdt', 'ETHUSDT', undefined, exchange, adapter);
    expect(result.newSession?.currentSymbol).toBe('BTCUSDT');
  });

  it('does NOT match unknown markets — falls through to legacy router', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    const result = await executeCommand('FAKEUSDT', 'BTCUSDT', undefined, exchange, adapter);
    expect(result.handled).toBe(false);
  });

  it('does NOT swallow multi-token input that happens to start with a market', async () => {
    // `BTCUSDT something` should NOT be a bare-instrument switch.
    // It currently goes to legacy router as unknown — we want that.
    const adapter = makeAdapter();
    const exchange = makeExchange();
    const result = await executeCommand('BTCUSDT 100', 'ETHUSDT', undefined, exchange, adapter);
    expect(result.handled).toBe(false);
  });

  it('does not match when not connected (no exchange)', async () => {
    const adapter = makeAdapter();
    const result = await executeCommand('BTCUSDT', 'ETHUSDT', undefined, null, adapter);
    expect(result.handled).toBe(false);
  });

  it('still resolves real commands ahead of bare-instrument', async () => {
    // If somehow a market exists with the same name as a registered
    // command, the registry wins. (Hypothetical — no real market is
    // named `buy`, but the precedence matters.)
    const adapter = makeAdapter();
    const exchange = makeExchange();
    // Put a fake market named `clear` (a registered utility command).
    exchange.store!.markets!.push({
      symbol: 'clear',
      base: 'CLEAR',
      quote: 'X',
      precision: { amount: 1, price: 1 },
    } as never);
    const result = await executeCommand('clear', 'BTCUSDT', undefined, exchange, adapter);
    // ClearCommand wins; no session change.
    expect(result.handled).toBe(true);
    expect(result.sessionChanged).toBeFalsy();
  });
});

describe('executeCommand — result.data threading', () => {
  it('threads CommandResult success/error out to ExecuteResult', async () => {
    const adapter = makeAdapter();
    const result = await executeCommand(
      'set @x 1',
      'BTCUSDT',
      undefined,
      null,
      adapter,
    );
    expect(result.handled).toBe(true);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/preprocess store unavailable/);
  });

  it('threads CommandResult.data.orderIds out to ExecuteResult.data', async () => {
    // buy $100 = market notional buy. Needs a ticker last price to
    // convert notional→amount, and a placeOrder mock that returns
    // the ID array we want to verify reaches ExecuteResult.data.
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['mkt-7']);
    const result = await executeCommand('buy $100', 'BTCUSDT', undefined, exchange, adapter);
    expect(result.handled).toBe(true);
    expect(result.data?.orderIds).toEqual(['mkt-7']);
  });

  it('omits data when the command did not produce any', async () => {
    // `cancel` returns plain success() with no data. The data field
    // should be absent (not set to an empty object or empty array)
    // so the runner's `data?.orderIds` short-circuits cleanly.
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange as unknown as { cancelOrders: ReturnType<typeof vi.fn> }).cancelOrders = vi.fn()
      .mockResolvedValue(undefined);
    const result = await executeCommand('cancel', 'BTCUSDT', undefined, exchange, adapter);
    expect(result.handled).toBe(true);
    expect(result.data).toBeUndefined();
  });
});

describe('end-to-end capture: preprocess + executeCommand + simulated writeback', () => {
  // Models the apps/cli runner: preprocess emits a step with
  // captureTo set, runner awaits executeCommand, reads result.data.
  // orderIds, and writes the comma-joined list to the store as a
  // static var. This test asserts the full chain so a runner-side
  // regression would surface here.

  async function runChain(
    chain: string,
    exchange: ExchangeLike,
    store: MemoryPreprocessStore,
    adapter: CLIPlatformAdapter,
  ): Promise<{ warnings: string[] }> {
    const warnings: string[] = [];
    const pre = preprocessCommand(chain, {
      exchange,
      symbol: 'BTCUSDT',
      store,
    });
    for (const step of pre.steps) {
      // Mirror the apps/cli runner: resolve any deferred-substitution
      // `@<name>` tokens against the CURRENT store before dispatch.
      // This is what makes `@o1 = buy …; cancel @o1` work in one
      // chain — the cancel's @o1 was left literal at preprocess and
      // we substitute it here, after the prior step's capture wrote
      // back.
      let resolvedCommand = step.command;
      if (step.deferredSubstitution) {
        for (const name of step.deferredSubstitution) {
          const value = store.getVar(name) ?? '';
          const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
          const re = new RegExp(`@${escaped}(?![A-Za-z0-9_])`, 'g');
          resolvedCommand = resolvedCommand.replace(re, value);
        }
      }
      const result = await executeCommand(
        resolvedCommand,
        'BTCUSDT',
        undefined,
        exchange,
        adapter,
        undefined,
        store,
      );
      if (result.handled && result.success === false) {
        throw new Error(result.error ?? 'Command failed');
      }
      if (step.captureTo !== undefined && result.handled) {
        const ids = result.data?.orderIds ?? [];
        store.setVar(step.captureTo, ids.join(','));
        if (ids.length === 0) {
          warnings.push(`@${step.captureTo}: captured 0 order IDs`);
        }
      }
    }
    return { warnings };
  }

  it('`@o1 = buy $100` populates @o1 with the returned ID', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['exch-id-abc']);
    const store = new MemoryPreprocessStore();
    await runChain('@o1 = buy $100', exchange, store, adapter);
    expect(store.getVar('o1')).toBe('exch-id-abc');
  });

  it('bracket-order capture stores all IDs comma-joined', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['entry-1', 'sl-2', 'tp-3']);
    const store = new MemoryPreprocessStore();
    await runChain(
      '@bracket = buy $100 at 49000 stop 48000',
      exchange,
      store,
      adapter,
    );
    expect(store.getVar('bracket')).toBe('entry-1,sl-2,tp-3');
  });

  it('empty-array return → empty var + warning', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue([]);
    const store = new MemoryPreprocessStore();
    const { warnings } = await runChain('@o1 = buy $100', exchange, store, adapter);
    expect(store.getVar('o1')).toBe('');
    expect(warnings).toEqual(['@o1: captured 0 order IDs']);
  });

  it('subsequent step references the captured var (substitute path)', async () => {
    // `@o1 = buy …; print @o1` — the print should see the captured
    // ID, proving the writeback completed BEFORE the next step ran.
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['captured-id']);
    const store = new MemoryPreprocessStore();
    await runChain('@o1 = buy $100; print @o1', exchange, store, adapter);
    expect(store.getVar('o1')).toBe('captured-id');
    expect(adapter.print).toHaveBeenCalledWith(['@o1 = captured-id']);
  });

  it('multi-symbol buy → captured var contains ALL leg IDs', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
      { symbol: 'ETHUSDT', last: 3_000 },
    ];
    const placeOrder = vi.fn()
      .mockResolvedValueOnce(['btc-1'])
      .mockResolvedValueOnce(['eth-1']);
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = placeOrder;
    const store = new MemoryPreprocessStore();
    await runChain('@grp = buy BTCUSDT,ETHUSDT $100', exchange, store, adapter);
    expect(store.getVar('grp')).toBe('btc-1,eth-1');
  });

  it('non-capture steps in the same chain are unaffected', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['post-id']);
    const store = new MemoryPreprocessStore();
    await runChain('buy $50; @o1 = buy $100; buy $50', exchange, store, adapter);
    expect(store.getVar('o1')).toBe('post-id');
    // Three separate orders placed.
    expect(
      (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder,
    ).toHaveBeenCalledTimes(3);
  });

  it('end-to-end — capture on one prompt, `cancel @o1` on the next prompt cancels the captured orders', async () => {
    // The realistic two-prompt flow: user runs the capture, then
    // types `cancel @o1` separately. By the time the second prompt
    // preprocesses, @o1 is bound — substitution expands to the
    // comma-list and CancelCommand's comma-list branch (added in
    // this commit) matches every piece.
    //
    // The same chain in ONE prompt (`@o1 = …; cancel @o1`) doesn't
    // work today because preprocess is one-shot — at chain-
    // preprocess time @o1 isn't yet set. Documented limitation
    // pending a runner refactor to defer per-segment substitution.
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['entry-1', 'sl-2']);
    // The order IDs need to be in the orders store so CancelCommand
    // can find them. Simulate the placement by adding to orders[].
    (exchange.store as unknown as { orders: unknown[] }).orders = [
      { id: 'entry-1', status: 'open', symbol: 'BTCUSDT', side: 'buy', type: 'limit', reduceOnly: false, lastUpdate: 0 },
      { id: 'sl-2', status: 'open', symbol: 'BTCUSDT', side: 'sell', type: 'limit', reduceOnly: true, lastUpdate: 0 },
    ];
    const cancelOrders = vi.fn();
    (exchange as unknown as { cancelOrders: ReturnType<typeof vi.fn> }).cancelOrders = cancelOrders;
    const store = new MemoryPreprocessStore();

    // Prompt 1: capture
    await runChain('@o1 = buy $100 at 49000 stop 48000', exchange, store, adapter);
    expect(store.getVar('o1')).toBe('entry-1,sl-2');

    // Prompt 2: cancel — fresh preprocess, @o1 substitutes to the
    // comma-list, CancelCommand walks it.
    await runChain('cancel @o1', exchange, store, adapter);
    expect(cancelOrders).toHaveBeenCalledTimes(1);
    const cancelledIds = (cancelOrders.mock.calls[0][0] as Array<{ id: string }>).map(
      (o) => o.id,
    );
    expect(cancelledIds.sort()).toEqual(['entry-1', 'sl-2']);
  });

  it('single-chain — `@o1 = buy …; cancel @o1` resolves @o1 against the live capture', async () => {
    // Was the locked-in limitation pre-deferred-substitution; now the
    // positive shape. The preprocess leaves `@o1` in the cancel
    // segment literal (deferredSubstitution = ['o1']) and the
    // runner resolves it after the capture writes back. Tests the
    // full forward-reference flow end-to-end in one chain.
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['some-id']);
    (exchange.store as unknown as { orders: unknown[] }).orders = [
      { id: 'some-id', status: 'open', symbol: 'BTCUSDT', side: 'buy', type: 'limit', reduceOnly: false, lastUpdate: 0 },
    ];
    const cancelOrders = vi.fn();
    (exchange as unknown as { cancelOrders: ReturnType<typeof vi.fn> }).cancelOrders = cancelOrders;
    const store = new MemoryPreprocessStore();

    await runChain('@o1 = buy $100; cancel @o1', exchange, store, adapter);

    expect(store.getVar('o1')).toBe('some-id');
    expect(cancelOrders).toHaveBeenCalledTimes(1);
    const cancelledIds = (cancelOrders.mock.calls[0][0] as Array<{ id: string }>).map(
      (o) => o.id,
    );
    expect(cancelledIds).toEqual(['some-id']);
  });

  it('single-chain — bracket capture + cancel composes across segments', async () => {
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['entry-1', 'sl-2']);
    (exchange.store as unknown as { orders: unknown[] }).orders = [
      { id: 'entry-1', status: 'open', symbol: 'BTCUSDT', side: 'buy', type: 'limit', reduceOnly: false, lastUpdate: 0 },
      { id: 'sl-2', status: 'open', symbol: 'BTCUSDT', side: 'sell', type: 'limit', reduceOnly: true, lastUpdate: 0 },
    ];
    const cancelOrders = vi.fn();
    (exchange as unknown as { cancelOrders: ReturnType<typeof vi.fn> }).cancelOrders = cancelOrders;
    const store = new MemoryPreprocessStore();

    await runChain(
      '@bracket = buy $100 at 49000 stop 48000; cancel @bracket',
      exchange,
      store,
      adapter,
    );

    expect(store.getVar('bracket')).toBe('entry-1,sl-2');
    expect(cancelOrders).toHaveBeenCalledTimes(1);
    expect(
      (cancelOrders.mock.calls[0][0] as Array<{ id: string }>).map((o) => o.id).sort(),
    ).toEqual(['entry-1', 'sl-2']);
  });

  it('single-chain — captures earlier in the chain don\'t leak into later un-related @-refs', async () => {
    // If the user types `@o1 = buy $100; echo @o2`, the @o2 in the
    // echo isn't a pending capture (no earlier segment captured to
    // o2), so it must still throw "Unknown variable" as before.
    // Sanity-check that the pending-captures set is name-scoped, not
    // a blanket "any @-ref is deferred".
    const adapter = makeAdapter();
    const exchange = makeExchange();
    (exchange.store as unknown as { tickers: unknown[] }).tickers = [
      { symbol: 'BTCUSDT', last: 50_000 },
    ];
    (exchange as unknown as { placeOrder: ReturnType<typeof vi.fn> }).placeOrder = vi.fn()
      .mockResolvedValue(['some-id']);
    const store = new MemoryPreprocessStore();
    await expect(
      runChain('@o1 = buy $100; echo @o2', exchange, store, adapter),
    ).rejects.toThrow(/Unknown variable @o2/);
  });
});
