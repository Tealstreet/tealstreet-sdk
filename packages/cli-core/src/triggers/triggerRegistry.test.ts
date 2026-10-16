import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  armTrigger,
  currentTriggerDepth,
  cancelTrigger,
  clearTriggers,
  listTriggers,
  MAX_ARMED_MS,
  TriggerDepthExceededError,
} from './triggerRegistry';
import type { ExchangeLike } from '../types';
import type { ParsedCondition } from '../preprocess/conditions';

const exchange = { name: 'bybit', store: {} } as unknown as ExchangeLike;

const condition = (satisfied: () => boolean): ParsedCondition => ({
  check: () => satisfied(),
  describe: 'price >= 100',
});

const arm = (over: Partial<Parameters<typeof armTrigger>[0]> = {}) =>
  armTrigger({
    describe: 'price >= 100',
    action: 'be +3%',
    symbol: 'BTCUSDT',
    account: 'main',
    resolveExchange: () => exchange,
    condition: condition(() => false),
    run: vi.fn().mockResolvedValue(undefined),
    ...over,
  });

describe('triggerRegistry', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    clearTriggers();
  });
  afterEach(() => {
    clearTriggers();
    vi.useRealTimers();
  });

  it('does not run the command while the condition is false', async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    arm({ run, condition: condition(() => false) });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(run).not.toHaveBeenCalled();
    expect(listTriggers()[0].status).toBe('armed');
  });

  it('runs the command once the condition becomes true', async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    let ready = false;
    arm({ run, condition: condition(() => ready) });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(run).not.toHaveBeenCalled();
    ready = true;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(run).toHaveBeenCalledWith('be +3%');
  });

  /**
   * The command may take longer than a poll interval. A second tick that could
   * start it again would place the order twice, which is why the status flips
   * to `fired` BEFORE `run` is awaited rather than after.
   */
  it('fires exactly once even when the command is slower than the poll', async () => {
    let release: (() => void) | undefined;
    const run = vi.fn().mockImplementation(() => new Promise<void>((resolve) => (release = resolve)));
    arm({ run, condition: condition(() => true) });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(run).toHaveBeenCalledTimes(1);
    release?.();
  });

  it('stops polling once it has fired', async () => {
    const check = vi.fn().mockReturnValue(true);
    arm({ run: vi.fn().mockResolvedValue(undefined), condition: { check, describe: 'x' } });
    await vi.advanceTimersByTimeAsync(1_000);
    const callsAfterFiring = check.mock.calls.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(check.mock.calls.length).toBe(callsAfterFiring);
  });

  it('reports a failing command instead of swallowing it', async () => {
    const onSettled = vi.fn();
    arm({
      run: vi.fn().mockRejectedValue(new Error('insufficient margin')),
      condition: condition(() => true),
      onSettled,
    });
    await vi.advanceTimersByTimeAsync(1_000);
    const trigger = listTriggers()[0];
    expect(trigger.status).toBe('failed');
    expect(trigger.error).toBe('insufficient margin');
    expect(onSettled).toHaveBeenCalled();
  });

  /**
   * A disconnected account is not a reason to give up — it reconnects. A
   * trigger that settled here would silently stop protecting the position.
   */
  it('keeps polling while the exchange is unavailable', async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    let live: ExchangeLike | null = null;
    arm({ run, condition: condition(() => true), resolveExchange: () => live });
    await vi.advanceTimersByTimeAsync(2_000);
    expect(run).not.toHaveBeenCalled();
    expect(listTriggers()[0].status).toBe('armed');
    live = exchange;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('expires rather than staying armed forever', async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    arm({ run, condition: condition(() => false) });
    // Jump the clock past the deadline, then let one poll observe it. Walking
    // every 250ms poll across 12h took ~3s and timed out under a loaded suite.
    vi.setSystemTime(Date.now() + MAX_ARMED_MS);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(listTriggers()[0].status).toBe('expired');
    expect(run).not.toHaveBeenCalled();
  });

  it('cancels an armed trigger and never fires it', async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    let ready = false;
    const t = arm({ run, condition: condition(() => ready) });
    expect(cancelTrigger(t.id)).toBe(true);
    ready = true;
    await vi.advanceTimersByTimeAsync(5_000);
    expect(run).not.toHaveBeenCalled();
    expect(listTriggers()[0].status).toBe('cancelled');
  });

  it('refuses to cancel one that already fired', async () => {
    const t = arm({ run: vi.fn().mockResolvedValue(undefined), condition: condition(() => true) });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(cancelTrigger(t.id)).toBe(false);
  });

  /**
   * A trigger whose command arms another is legitimate — a ladder arms its next
   * rung. A trigger that arms itself is a loop that places orders.
   */
  it('refuses to arm past the re-entrancy ceiling', () => {
    expect(() => arm({ depth: 5 })).toThrow(TriggerDepthExceededError);
  });

  /**
   * `firingDepth` is module state. Two triggers firing in the same tick used
   * to interleave across the `await` and reset each other to 0, so a trigger
   * whose command armed another could slip past the cap — the self-arming
   * order loop the cap exists to stop. Firing is serialized, and the depth is
   * RESTORED rather than zeroed.
   */
  it('keeps each firing trigger\'s depth to itself when two fire at once', async () => {
    const seen: number[] = [];
    // Reads the depth on BOTH sides of an await. The second read is the one
    // that matters: it lands after the other trigger would have settled, so a
    // concurrent fire that zeroed the shared counter shows up as a 0 here.
    const slow = () => async () => {
      seen.push(currentTriggerDepth());
      await new Promise<void>((resolve) => setTimeout(resolve, 1_000));
      seen.push(currentTriggerDepth());
    };
    const quick = () => async () => {
      seen.push(currentTriggerDepth());
    };
    arm({ run: slow(), condition: condition(() => true), depth: 0 });
    arm({ run: quick(), condition: condition(() => true), depth: 2 });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(seen).toEqual([1, 1, 3]);
    expect(currentTriggerDepth()).toBe(0);
  });

  it('does not let a failing trigger stop the next one from firing', async () => {
    const second = vi.fn().mockResolvedValue(undefined);
    arm({ run: vi.fn().mockRejectedValue(new Error('boom')), condition: condition(() => true) });
    arm({ run: second, condition: condition(() => true) });
    await vi.advanceTimersByTimeAsync(2_000);
    expect(second).toHaveBeenCalledTimes(1);
  });

  /**
   * An armed trigger must never be the reason the process stays alive — that
   * is the `setInterval` in `logger.ts` that users reported as "the CLI never
   * exits".
   */
  it('arms on an unref-ed timer', () => {
    const unref = vi.fn();
    const spy = vi.spyOn(global, 'setTimeout').mockReturnValue({ unref } as never);
    try {
      arm();
      expect(unref).toHaveBeenCalled();
    } finally {
      spy.mockRestore();
    }
  });
});
