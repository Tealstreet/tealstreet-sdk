import type { CommandOutcome, SerializedCommandError } from '@tealstreet/safe-cex/src/commandOutcome';

import { markErrorReported, serializeCommandError } from '@tealstreet/safe-cex/src/commandOutcome';

/** A failed batch with IDs reported by the SDK, never a successful batch result. */
export class ScriptBatchOrderError extends Error {
  readonly partialOrderIds: readonly string[];
  declare readonly outcome?: CommandOutcome;
  declare readonly partialOrderCount?: number;
  declare readonly totalOrderCount?: number;

  private constructor(details: SerializedCommandError, ids: string[], cause: unknown) {
    super(details.message, { cause });
    const { name: _name, message: _message, partialOrderIds: _ids, ...fields } = details;
    Object.assign(this, fields);
    this.name = 'ScriptBatchOrderError';
    this.partialOrderIds = Object.freeze(ids);
    if (details.errorReported === true) markErrorReported(this);
  }

  /** Missing/malformed partial facts leave the original failure untouched. */
  static from(error: unknown): ScriptBatchOrderError | undefined {
    if (error === null || typeof error !== 'object') return undefined;
    try {
      if (error instanceof ScriptBatchOrderError) return error;
      const details = serializeCommandError(error);
      const ids = details.partialOrderIds;
      if (!Array.isArray(ids)) return undefined;
      if (ids.length === 0 || !ids.every((id): id is string => typeof id === 'string')) {
        return undefined;
      }
      return new ScriptBatchOrderError(details, ids, error);
    } catch {
      // An unreadable error property cannot replace the original failure.
      return undefined;
    }
  }
}
