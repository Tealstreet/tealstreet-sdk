import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { WhenCommand } from './WhenCommand';
import { clearTriggers, listTriggers } from '../../triggers/triggerRegistry';
import type { CommandContext } from '../Command';
import type { ExchangeLike } from '../../types';

const makeExchange = (last = 100): ExchangeLike =>
  ({
    name: 'bybit',
    store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } }],
      tickers: [{ symbol: 'BTCUSDT', last, bid: last, ask: last, mark: last }],
      orders: [],
      positions: [],
    },
  }) as unknown as ExchangeLike;

const ctx = (raw: string, exchange: ExchangeLike | null, runChain?: unknown): CommandContext =>
  ({
    raw,
    args: { _: raw.split(/\s+/) },
    symbol: 'BTCUSDT',
    accountName: 'main',
    exchange,
    adapter: {
      printInfo: vi.fn(),
      print: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
      runChain: runChain ?? vi.fn().mockResolvedValue(undefined),
    },
  }) as unknown as CommandContext;

describe('WhenCommand', () => {
  let cmd: WhenCommand;
  beforeEach(() => {
    vi.useFakeTimers();
    clearTriggers();
    cmd = new WhenCommand();
  });
  afterEach(() => {
    clearTriggers();
    vi.useRealTimers();
  });

  it('arms and returns immediately rather than blocking', async () => {
    const r = await cmd.execute(ctx('when (price >= 200) (be +3%)', makeExchange(100)));
    expect(r.success).toBe(true);
    expect(listTriggers()).toHaveLength(1);
    expect(listTriggers()[0].action).toBe('be +3%');
  });

  it('runs the command through the host once the condition holds', async () => {
    const runChain = vi.fn().mockResolvedValue(undefined);
    const exchange = makeExchange(100);
    await cmd.execute(ctx('when (price >= 150) (trail 20%)', exchange, runChain));
    await vi.advanceTimersByTimeAsync(1_000);
    expect(runChain).not.toHaveBeenCalled();

    (exchange.store as { tickers: Array<Record<string, number>> }).tickers[0].last = 200;
    (exchange.store as { tickers: Array<Record<string, number>> }).tickers[0].ask = 200;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(runChain).toHaveBeenCalledWith('trail 20%', { symbol: 'BTCUSDT', account: 'main' });
  });

  /**
   * The parens are load-bearing. A condition is variable-length and so is a
   * command, so there is no delimiter a heuristic could find — this mirrors
   * `if (<cond>) (<then>)`, which solved the same problem first.
   */
  it('refuses a form with no condition parens', async () => {
    const r = await cmd.execute(ctx('when price >= 200 be +3%', makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/expected '\(<condition>\)'/);
    expect(listTriggers()).toHaveLength(0);
  });

  it('refuses a form with no command parens', async () => {
    const r = await cmd.execute(ctx('when (price >= 200)', makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/expected '\(<command>\)'/);
  });

  it('refuses trailing tokens rather than quietly dropping them', async () => {
    const r = await cmd.execute(ctx('when (price >= 200) (be) then something', makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/trailing tokens/);
    expect(listTriggers()).toHaveLength(0);
  });

  it('refuses an unrecognised condition', async () => {
    const r = await cmd.execute(ctx('when (banana) (be)', makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/unrecognized condition/);
  });

  it('refuses an empty command', async () => {
    const r = await cmd.execute(ctx('when (price >= 200) ()', makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/empty/);
  });

  /**
   * A one-shot invocation exits as soon as its command returns, so a trigger
   * armed there could never fire. The host signals that by not supplying
   * `runChain`, and refusing is the honest answer — arming something that can
   * never fire is worse than saying so.
   */
  it('refuses to arm on a runtime that cannot run a command later', async () => {
    const c = ctx('when (price >= 200) (be)', makeExchange());
    delete (c.adapter as { runChain?: unknown }).runChain;
    const r = await cmd.execute(c);
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/needs a session that stays open/);
    expect(listTriggers()).toHaveLength(0);
  });

  it('refuses when there is no exchange', async () => {
    const r = await cmd.execute(ctx('when (price >= 200) (be)', null));
    expect(r.success).toBe(false);
  });

  /**
   * A trigger armed on BTCUSDT must fire on BTCUSDT even if the user has since
   * switched the session to something else — otherwise `be +3%` moves the
   * wrong position's stop. The host runs it under the same per-call override
   * the listener's `cmd:exec` uses.
   */
  it('fires scoped to the symbol and account it was armed on', async () => {
    const runChain = vi.fn().mockResolvedValue(undefined);
    const exchange = makeExchange(300);
    await cmd.execute(ctx('when (price >= 150) (be +3%)', exchange, runChain));
    await vi.advanceTimersByTimeAsync(1_000);
    expect(runChain).toHaveBeenCalledWith('be +3%', { symbol: 'BTCUSDT', account: 'main' });
  });

  /** `splitChain` is paren-aware, so a `;` inside the action belongs to it. */
  it('keeps a chained command whole inside the action parens', async () => {
    await cmd.execute(ctx('when (price >= 200) (be +3%; trail 20%)', makeExchange()));
    expect(listTriggers()[0].action).toBe('be +3%; trail 20%');
  });

  it('accepts a combinator condition', async () => {
    const r = await cmd.execute(ctx('when (both (price > 50) and (price < 500)) (close 50%)', makeExchange(100)));
    expect(r.success).toBe(true);
    expect(listTriggers()[0].action).toBe('close 50%');
  });
});
