export const FIRST_SIGINT_DEADLINE_MS = 15_000;

export type ShutdownRequest = {
  readonly first: boolean;
  readonly deadlineAtMs: number;
};

export type DeadlineResult<T> = { readonly kind: 'settled'; readonly value: T } | { readonly kind: 'deadline' };

type DeadlineOptions = {
  readonly nowMs?: () => number;
  readonly setTimer?: (callback: () => void, delayMs: number) => ReturnType<typeof setTimeout>;
  readonly clearTimer?: (timer: ReturnType<typeof setTimeout>) => void;
};

export type OutcomeHandle<T> = {
  readonly outcome: Promise<T>;
  readonly outcomeDeadlineAtMs: number;
};

export type OutcomeFenceReason = 'outcome' | 'deadline' | 'interrupted';

export type ExecLifecycle = {
  requestShutdown(): ShutdownRequest;
  isShutdownRequested(): boolean;
  isSecondInterrupt(): boolean;
  deadlineAtMs(): number | undefined;
  track<T>(work: Promise<T>): Promise<T>;
  drain(deadlineAtMs: number): Promise<boolean>;
  drainAll(): Promise<void>;
  markFailure(): void;
  hasFailures(): boolean;
};

export const raceWithDeadline = async <T>(
  work: Promise<T>,
  deadlineAtMs: number,
  opts: DeadlineOptions = {},
): Promise<DeadlineResult<T>> => {
  const nowMs = opts.nowMs ?? (() => Date.now());
  const setTimer = opts.setTimer ?? ((callback, delayMs) => setTimeout(callback, delayMs));
  const clearTimer = opts.clearTimer ?? ((timer) => clearTimeout(timer));
  const remainingMs = deadlineAtMs - nowMs();
  if (remainingMs <= 0) {
    void work.catch(() => undefined);
    return { kind: 'deadline' };
  }

  const timeout = Promise.withResolvers<DeadlineResult<T>>();
  const timer = setTimer(() => timeout.resolve({ kind: 'deadline' }), remainingMs);
  const settled = work.then<DeadlineResult<T>>(
    (value) => ({ kind: 'settled', value }),
    (error: unknown) => {
      throw error;
    },
  );
  try {
    return await Promise.race([settled, timeout.promise]);
  } finally {
    clearTimer(timer);
  }
};

export const waitForOutcomesThenFence = async <T, TFence>(opts: {
  readonly handles: readonly OutcomeHandle<T>[];
  readonly interrupted: Promise<unknown>;
  readonly fence: () => TFence;
  readonly clock?: DeadlineOptions;
}): Promise<{ readonly reason: OutcomeFenceReason; readonly fenceResult: TFence }> => {
  if (opts.handles.length === 0) {
    return { reason: 'outcome', fenceResult: opts.fence() };
  }

  const outcomeDeadlineAtMs = Math.max(...opts.handles.map((handle) => handle.outcomeDeadlineAtMs));
  const signaled = Promise.race([
    Promise.all(opts.handles.map((handle) => handle.outcome)).then(() => 'outcome' as const),
    opts.interrupted.then(() => 'interrupted' as const),
  ]);
  const result = await raceWithDeadline(signaled, outcomeDeadlineAtMs, opts.clock);
  const reason = result.kind === 'deadline' ? 'deadline' : result.value;
  return { reason, fenceResult: opts.fence() };
};

/**
 * Drain commands without a deadline until shutdown is requested, then switch
 * the same wait to the shared absolute shutdown deadline.
 */
export const drainUntilInterrupted = async (
  lifecycle: Pick<ExecLifecycle, 'drain' | 'drainAll'>,
  interrupted: Promise<number>,
): Promise<boolean> => {
  const first = await Promise.race([
    lifecycle.drainAll().then(() => ({ kind: 'settled' as const })),
    interrupted.then((deadlineAtMs) => ({ kind: 'interrupted' as const, deadlineAtMs })),
  ]);
  return first.kind === 'settled' ? true : lifecycle.drain(first.deadlineAtMs);
};

export const createExecLifecycle = (opts: {
  readonly nowMs?: () => number;
  readonly onSecondInterrupt: () => void;
}): ExecLifecycle => {
  const nowMs = opts.nowMs ?? (() => Date.now());
  const inFlight = new Set<Promise<unknown>>();
  let shutdownDeadlineAtMs: number | undefined;
  let secondInterrupt = false;
  let failures = 0;

  const track = <T>(work: Promise<T>): Promise<T> => {
    inFlight.add(work);
    void work.then(
      () => inFlight.delete(work),
      () => inFlight.delete(work),
    );
    return work;
  };

  const drain = async (deadlineAtMs: number): Promise<boolean> => {
    while (inFlight.size > 0) {
      const batch = Promise.allSettled([...inFlight]);
      const result = await raceWithDeadline(batch, deadlineAtMs, { nowMs });
      if (result.kind === 'deadline') return false;
    }
    return true;
  };
  const drainAll = async (): Promise<void> => {
    while (inFlight.size > 0) {
      await Promise.allSettled([...inFlight]);
    }
  };

  return {
    requestShutdown() {
      if (shutdownDeadlineAtMs === undefined) {
        shutdownDeadlineAtMs = nowMs() + FIRST_SIGINT_DEADLINE_MS;
        return { first: true, deadlineAtMs: shutdownDeadlineAtMs };
      }
      secondInterrupt = true;
      opts.onSecondInterrupt();
      return { first: false, deadlineAtMs: shutdownDeadlineAtMs };
    },
    isShutdownRequested: () => shutdownDeadlineAtMs !== undefined,
    isSecondInterrupt: () => secondInterrupt,
    deadlineAtMs: () => shutdownDeadlineAtMs,
    track,
    drain,
    drainAll,
    markFailure() {
      failures += 1;
    },
    hasFailures: () => failures > 0,
  };
};
