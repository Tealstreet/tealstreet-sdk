import type { ParsedCondition } from '../preprocess/conditions';
import type { ExchangeLike } from '../types';

/**
 * Armed triggers — a condition that fires a command ONCE, later, without
 * holding the prompt.
 *
 * `wait` already blocks a chain on a condition, and cannot do this job: it
 * occupies the line it is typed on and is hard-capped at ten minutes
 * (`WaitCommand.MAX_TIMEOUT_MS`). "Move my stop when TP1 fills" is a wait of
 * hours on a prompt the user wants to keep typing at, which is a different
 * thing and needs a different primitive.
 *
 * This module owns only the ARMING. It never imports the command executor —
 * the caller supplies `run` as a closure, which is what keeps
 * `CommandExecutor -> WhenCommand -> CommandExecutor` from closing into an
 * import cycle, and what lets the host inject its own chain runner so a fired
 * command goes through the same preprocessing and write-op audit trail a typed
 * one does.
 */

/** How often an armed condition is re-checked. Matches `wait`'s poll. */
const POLL_INTERVAL_MS = 250;

/**
 * Hard ceiling on how long a trigger stays armed. A trigger is invisible by
 * design — it fires when the user is not looking — so one that can live
 * forever is one that fires a market order a week after it was forgotten.
 * Twelve hours covers a trading session and expires loudly.
 */
export const MAX_ARMED_MS = 12 * 60 * 60 * 1000;

/**
 * Re-entrancy ceiling. A trigger's command may arm another trigger, which is
 * legitimate (a ladder arms its next rung), but a trigger that arms itself is
 * a loop that places orders.
 */
const MAX_CHAIN_DEPTH = 5;

export type TriggerStatus = 'armed' | 'fired' | 'failed' | 'expired' | 'cancelled';

export interface ArmedTrigger {
  id: string;
  /** Human-readable condition, for `tasks`. */
  describe: string;
  /** The command text this fires. Stored verbatim, run through the host. */
  action: string;
  symbol: string;
  account: string | undefined;
  status: TriggerStatus;
  armedAt: number;
  /** Set once the trigger leaves `armed`. */
  settledAt?: number;
  /** Populated when `status` is `failed`. */
  error?: string;
  depth: number;
}

export interface ArmTriggerOptions {
  describe: string;
  action: string;
  symbol: string;
  account: string | undefined;
  /**
   * Re-resolve the exchange at FIRE time rather than closing over it. An
   * account switch calls `stopExchange` on the old instance, so a trigger
   * armed before the switch would poll a dead store and a fired order would go
   * to an exchange the user has left.
   */
  resolveExchange: () => ExchangeLike | null | undefined;
  condition: ParsedCondition;
  /** Supplied by the caller so this module never imports the executor. */
  run: (action: string) => Promise<void>;
  /** Inherited depth when a trigger's own command arms another. */
  depth?: number;
  onSettled?: (trigger: ArmedTrigger) => void;
}

const triggers = new Map<string, ArmedTrigger>();
const timers = new Map<string, ReturnType<typeof setTimeout>>();

let counter = 0;
const nextId = (): string => {
  counter += 1;
  return `when-${Date.now().toString(36)}-${counter.toString(36)}`;
};

/** Depth of the trigger currently firing, so a nested `when` can inherit it. */
let firingDepth = 0;
export const currentTriggerDepth = (): number => firingDepth;

/**
 * Only one trigger fires at a time.
 *
 * Two reasons, and the first is a correctness bug rather than a preference.
 * `firingDepth` is module state read by a nested `when` while its parent's
 * command runs; two triggers firing concurrently would interleave across the
 * `await` and one would reset the other's depth, so a self-arming trigger
 * could slip past the cap that exists to stop it placing orders in a loop.
 * The second is plainer: two commands placing orders against the same store at
 * the same moment is not something a user asked for by arming two conditions.
 */
let fireChain: Promise<void> = Promise.resolve();

export class TriggerDepthExceededError extends Error {
  constructor(depth: number) {
    super(`when: refusing to arm — already ${depth} triggers deep, which is a loop that places orders.`);
    this.name = 'TriggerDepthExceededError';
  }
}

export const armTrigger = (opts: ArmTriggerOptions): ArmedTrigger => {
  const depth = opts.depth ?? 0;
  if (depth >= MAX_CHAIN_DEPTH) throw new TriggerDepthExceededError(depth);

  const trigger: ArmedTrigger = {
    id: nextId(),
    describe: opts.describe,
    action: opts.action,
    symbol: opts.symbol,
    account: opts.account,
    status: 'armed',
    armedAt: Date.now(),
    depth,
  };
  triggers.set(trigger.id, trigger);

  const settle = (status: TriggerStatus, error?: string) => {
    if (trigger.status !== 'armed') return;
    trigger.status = status;
    trigger.settledAt = Date.now();
    if (error) trigger.error = error;
    const timer = timers.get(trigger.id);
    if (timer) clearTimeout(timer);
    timers.delete(trigger.id);
    opts.onSettled?.(trigger);
  };

  const deadline = trigger.armedAt + MAX_ARMED_MS;

  // A self-rescheduling timeout, never `setInterval` (root CLAUDE.md). An
  // overlapping tick here would fire the same command twice.
  const tick = async () => {
    timers.delete(trigger.id);
    if (trigger.status !== 'armed') return;

    if (Date.now() >= deadline) {
      settle('expired');
      return;
    }

    let satisfied = false;
    try {
      const exchange = opts.resolveExchange();
      // Not an error: the account may simply be reconnecting. Keep polling.
      if (exchange) satisfied = opts.condition.check(exchange, opts.symbol);
    } catch (err) {
      settle('failed', err instanceof Error ? err.message : String(err));
      return;
    }

    if (!satisfied) {
      schedule();
      return;
    }

    // Mark fired BEFORE running. The command may itself take a while, and a
    // second tick must not be able to start it again.
    trigger.status = 'fired';
    trigger.settledAt = Date.now();
    timers.delete(trigger.id);

    const queued = fireChain.then(async () => {
      const previousDepth = firingDepth;
      firingDepth = depth + 1;
      try {
        await opts.run(opts.action);
        opts.onSettled?.(trigger);
      } catch (err) {
        // Never silent: a trigger fires while the user is looking elsewhere,
        // so a failure that printed nothing would read as "it never fired".
        trigger.status = 'failed';
        trigger.error = err instanceof Error ? err.message : String(err);
        opts.onSettled?.(trigger);
      } finally {
        // Restored, not zeroed. Zeroing is only correct for the outermost
        // fire, and assuming that is what let a concurrent one clobber it.
        firingDepth = previousDepth;
      }
    });
    // A failing trigger must not poison the chain for the next one.
    fireChain = queued.then(
      () => undefined,
      () => undefined,
    );
    await queued;
  };

  const schedule = () => {
    const timer = setTimeout(() => void tick(), POLL_INTERVAL_MS);
    // Without this a single armed trigger keeps the Bun process alive after a
    // one-shot command finishes — the exact shape of the `setInterval` in
    // `logger.ts` that users reported as "the CLI never exits".
    timer.unref?.();
    timers.set(trigger.id, timer);
  };

  schedule();
  return trigger;
};

export const listTriggers = (): ArmedTrigger[] => Array.from(triggers.values());

export const cancelTrigger = (id: string): boolean => {
  const trigger = triggers.get(id);
  if (!trigger || trigger.status !== 'armed') return false;
  trigger.status = 'cancelled';
  trigger.settledAt = Date.now();
  const timer = timers.get(id);
  if (timer) clearTimeout(timer);
  timers.delete(id);
  return true;
};

/** Test seam, and the teardown a host should call before exiting. */
export const clearTriggers = (): void => {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  triggers.clear();
  firingDepth = 0;
  fireChain = Promise.resolve();
};
