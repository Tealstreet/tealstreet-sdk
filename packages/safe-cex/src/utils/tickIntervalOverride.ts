import type { ExchangeName } from '../types';

/**
 * Server-pushed per-exchange polling slowdown — the break-glass lever for a
 * venue that has started rate-limiting us.
 *
 * The host app derives this from per-user feature flags and pushes it through
 * `__workerConfig` as a typed setter. safe-cex never
 * sees a flag name; it only sees a number.
 *
 * **A multiplier below 1 is refused.** This exists to reduce request pressure
 * during an incident, and a lever that can also speed polling up is one bad
 * value away from causing the outage it was reached for. Values below 1 are
 * clamped to 1 (no change) rather than rejected loudly, because a misconfigured
 * flag must not break polling entirely.
 */

const multipliers = new Map<string, number>();
const listeners = new Set<() => void>();

/** Engine pollers consume changes; callers remove the listener on disposal. */
export const subscribeTickIntervalMultipliers = (listener: () => void): (() => void) => {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
};

/**
 * Reserved key meaning "every exchange".
 *
 * The flag engine folds rules to ONE boolean per user, so the lever is
 * per-user, not per-venue — a wildcard says exactly that. It also keeps the
 * producer from having to enumerate the user's accounts, which would mean
 * importing the exchange controller into the worker factory and closing an
 * import cycle around it.
 */
export const ALL_EXCHANGES = '*';

const MIN_MULTIPLIER = 1;

/** Ceiling so a fat-fingered flag cannot effectively stop an account polling. */
const MAX_MULTIPLIER = 60;

export const setTickIntervalMultipliers = (next: Readonly<Record<string, number>> | undefined): void => {
  multipliers.clear();
  for (const [exchange, raw] of Object.entries(next ?? {})) {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) continue;
    const clamped = Math.min(Math.max(raw, MIN_MULTIPLIER), MAX_MULTIPLIER);
    if (clamped === MIN_MULTIPLIER) continue;
    multipliers.set(exchange, clamped);
  }
  for (const listener of listeners) listener();
};

/** 1 when nothing is set for this exchange, so callers can multiply unconditionally. */
export const getTickIntervalMultiplier = (exchange: ExchangeName | string): number =>
  multipliers.get(exchange) ?? multipliers.get(ALL_EXCHANGES) ?? MIN_MULTIPLIER;

/** Test/diagnostic accessor — not part of the consumer contract. */
export const getTickIntervalMultipliersForTests = (): Record<string, number> => Object.fromEntries(multipliers);
