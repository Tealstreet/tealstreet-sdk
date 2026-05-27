import { evaluateLiveVarSource } from './reactiveVars';
import type { PreprocessContext } from './types';

/**
 * Tracked variables — `track max @<name> <source>` /
 * `track min @<name> <source>`. The stored value is the high-water
 * (or low-water) mark of the source expression seen so far. Each
 * `@<name>` substitution re-evaluates the source, compares against
 * the stored extreme, and updates the extreme if the new value is
 * more extreme.
 *
 * Use case: poor-man's trailing stop without a chaser.
 *   track max @hi $pnl
 *   sets @stop @hi-100
 *   wait pnl < @stop          # release when pnl drops $100 below the
 *                              high-water unrealised PnL.
 *   close
 *
 * Storage: session-scoped (per-PreprocessStore-instance), NOT
 * persisted. A high-water mark from yesterday's session isn't
 * meaningful to carry into a new one — the user re-registers the
 * track with the same `track max` command.
 *
 * Mutex with `set` (static) and `sets` (live) on the same name —
 * the storage layer enforces that all three setters clear the
 * other two when called.
 */

export type TrackedVarKind = 'min' | 'max';

export interface TrackedVarEntry {
  kind: TrackedVarKind;
  /** The source expression — same vocabulary as live-var sources. */
  source: string;
  /** Current high/low-water value. `undefined` until first resolution. */
  value: number | undefined;
}

/**
 * Resolve `@<name>` when it's bound as a tracked var. Evaluates the
 * source against current ctx, compares against the stored extreme,
 * mutates the store with the new extreme if it's more extreme, and
 * returns the resulting extreme.
 *
 * The first resolution (when `entry.value === undefined`) seeds the
 * tracker with whatever the source evaluates to right now.
 *
 * Throws wrapped errors when:
 *   - The source evaluation throws (no position, unknown constant,
 *     etc.) — re-thrown with the same prefix-stripping logic as
 *     evaluateLiveVarSource so the error chain stays readable.
 *   - A cycle is detected (a tracker's source references itself
 *     transitively). Detection threads through `seen` the same way
 *     live vars do — pass the same Set down.
 */
export function resolveTrackedVar(
  name: string,
  entry: TrackedVarEntry,
  ctx: PreprocessContext,
  seen: Set<string>,
): number {
  // Reuse evaluateLiveVarSource for the source evaluation — it
  // handles tokenization, substitution, cycle detection, and error
  // wrapping. The aggregator runs OUTSIDE the live-var machinery so
  // we don't conflate "live evaluation" with "tracked accumulation".
  const current = evaluateLiveVarSource(name, entry.source, ctx, seen);
  const next = entry.value === undefined
    ? current
    : entry.kind === 'max'
      ? Math.max(entry.value, current)
      : Math.min(entry.value, current);
  if (next !== entry.value) {
    ctx.store.updateTrackedVar(name, next);
  }
  return next;
}
