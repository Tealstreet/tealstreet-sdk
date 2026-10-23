import type { CommandOutcome } from '@tealstreet/safe-cex/src/commandOutcome';

/** A failed batch with IDs reported by the SDK, never a successful batch result. */
export class ScriptBatchOrderError extends Error {
  readonly partialOrderIds: readonly string[];
  declare readonly outcome?: CommandOutcome;
  declare readonly partialOrderCount?: number;
  declare readonly totalOrderCount?: number;

  private constructor(message: string, ids: string[], source: Record<string, unknown>, cause: unknown) {
    super(message, { cause });
    this.name = 'ScriptBatchOrderError';
    this.partialOrderIds = Object.freeze(ids);
    const outcome = source.outcome;
    if (outcome === 'notSent' || outcome === 'rejected' || outcome === 'unknown') {
      this.outcome = outcome;
    }
    for (const key of ['partialOrderCount', 'totalOrderCount'] as const) {
      const count = source[key];
      if (typeof count === 'number' && Number.isSafeInteger(count) && count >= 0) this[key] = count;
    }
  }

  /** Missing/malformed partial facts leave the original failure untouched. */
  static from(error: unknown): ScriptBatchOrderError | undefined {
    if (error === null || typeof error !== 'object') return undefined;
    try {
      if (error instanceof ScriptBatchOrderError) return error;
      const source = error as Record<string, unknown>;
      const reportedIds = source.partialOrderIds;
      if (!Array.isArray(reportedIds)) return undefined;
      const ids = Array.from(reportedIds);
      if (ids.length === 0 || !ids.every((id): id is string => typeof id === 'string')) {
        return undefined;
      }
      const reportedMessage = source.message;
      const message = typeof reportedMessage === 'string' ? reportedMessage : 'Unknown error';
      return new ScriptBatchOrderError(message, ids, source, error);
    } catch {
      // An unreadable error property cannot replace the original failure.
      return undefined;
    }
  }
}
