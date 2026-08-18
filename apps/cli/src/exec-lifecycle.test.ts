import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  FIRST_SIGINT_DEADLINE_MS,
  createExecLifecycle,
  drainUntilInterrupted,
  raceWithDeadline,
  waitForOutcomesThenFence,
} from './exec-lifecycle';

afterEach(() => {
  vi.useRealTimers();
});

describe('createExecLifecycle', () => {
  it('establishes one 15s deadline on first SIGINT and makes the second immediate', () => {
    let now = 1_000;
    const onSecondInterrupt = vi.fn();
    const life = createExecLifecycle({ nowMs: () => now, onSecondInterrupt });

    const first = life.requestShutdown();
    expect(first).toEqual({ first: true, deadlineAtMs: 1_000 + FIRST_SIGINT_DEADLINE_MS });
    expect(life.isShutdownRequested()).toBe(true);

    now = 9_000;
    const second = life.requestShutdown();
    expect(second).toEqual({ first: false, deadlineAtMs: first.deadlineAtMs });
    expect(life.isSecondInterrupt()).toBe(true);
    expect(onSecondInterrupt).toHaveBeenCalledOnce();
  });

  it('drains every tracked command that entered before shutdown', async () => {
    const entered = Promise.withResolvers<void>();
    const life = createExecLifecycle({ onSecondInterrupt: vi.fn() });
    life.track(entered.promise);

    let drained = false;
    const draining = life.drainAll().then(() => {
      drained = true;
    });
    await Promise.resolve();
    expect(drained).toBe(false);

    entered.resolve();
    await draining;
    expect(drained).toBe(true);
  });

  it('stops waiting for a never-settling entered command at the shared deadline', async () => {
    vi.useFakeTimers();
    const life = createExecLifecycle({ nowMs: () => 1_000, onSecondInterrupt: vi.fn() });
    life.track(new Promise<void>(() => undefined));
    const deadlineAtMs = life.requestShutdown().deadlineAtMs;

    const draining = life.drain(deadlineAtMs);
    await vi.advanceTimersByTimeAsync(FIRST_SIGINT_DEADLINE_MS);
    await expect(draining).resolves.toBe(false);
  });
});

describe('drainUntilInterrupted', () => {
  it('switches a pending unbounded drain to the shared interrupt deadline', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const interrupted = Promise.withResolvers<number>();
    const life = createExecLifecycle({ onSecondInterrupt: vi.fn() });
    life.track(new Promise<void>(() => undefined));

    const draining = drainUntilInterrupted(life, interrupted.promise);
    interrupted.resolve(2_000);
    await vi.advanceTimersByTimeAsync(999);
    let settled = false;
    void draining.then(() => {
      settled = true;
    });
    await Promise.resolve();
    expect(settled).toBe(false);

    await vi.advanceTimersByTimeAsync(1);
    await expect(draining).resolves.toBe(false);
  });
});

describe('raceWithDeadline', () => {
  it('returns a settled value before the absolute deadline', async () => {
    await expect(raceWithDeadline(Promise.resolve('ok'), Date.now() + 1_000)).resolves.toEqual({
      kind: 'settled',
      value: 'ok',
    });
  });

  it('does not wait when the absolute deadline has already expired', async () => {
    await expect(raceWithDeadline(new Promise<void>(() => undefined), 999, { nowMs: () => 1_000 })).resolves.toEqual({
      kind: 'deadline',
    });
  });
});

describe('waitForOutcomesThenFence', () => {
  it('waits for the latest absolute outcome deadline before fencing unresolved work', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const fence = vi.fn(() => ({ deadlineAtMs: 17_000 }));
    const waiting = waitForOutcomesThenFence({
      handles: [
        { outcome: new Promise<void>(() => undefined), outcomeDeadlineAtMs: 1_500 },
        { outcome: new Promise<void>(() => undefined), outcomeDeadlineAtMs: 2_000 },
      ],
      interrupted: new Promise<void>(() => undefined),
      fence,
    });

    await vi.advanceTimersByTimeAsync(999);
    expect(fence).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);

    await expect(waiting).resolves.toEqual({
      reason: 'deadline',
      fenceResult: { deadlineAtMs: 17_000 },
    });
    expect(fence).toHaveBeenCalledOnce();
  });

  it('fences immediately after every outcome settles', async () => {
    const first = Promise.withResolvers<void>();
    const second = Promise.withResolvers<void>();
    const fence = vi.fn(() => 'fenced');
    const waiting = waitForOutcomesThenFence({
      handles: [
        { outcome: first.promise, outcomeDeadlineAtMs: Date.now() + 10_000 },
        { outcome: second.promise, outcomeDeadlineAtMs: Date.now() + 20_000 },
      ],
      interrupted: new Promise<void>(() => undefined),
      fence,
    });

    first.resolve();
    await Promise.resolve();
    expect(fence).not.toHaveBeenCalled();
    second.resolve();

    await expect(waiting).resolves.toEqual({ reason: 'outcome', fenceResult: 'fenced' });
    expect(fence).toHaveBeenCalledOnce();
  });

  it('clears the deadline timer and fences after interruption', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const interrupted = Promise.withResolvers<void>();
    const fence = vi.fn(() => 'fenced');
    const waiting = waitForOutcomesThenFence({
      handles: [{ outcome: new Promise<void>(() => undefined), outcomeDeadlineAtMs: 20_000 }],
      interrupted: interrupted.promise,
      fence,
    });

    interrupted.resolve();
    await expect(waiting).resolves.toEqual({ reason: 'interrupted', fenceResult: 'fenced' });
    expect(fence).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });
});
