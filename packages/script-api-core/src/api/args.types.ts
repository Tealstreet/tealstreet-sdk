/**
 * ScriptArgs — the parsed-argument shape every script receives.
 * See docs/custom-scripts/SPEC.md §7.1.
 *
 * Types only in Phase 1. Tokenizer impl (argParser.ts) lands in Phase 2.
 */

export interface ScriptArgs {
  /** Unmodified raw tokens (post-substitution but pre-flag-extraction). */
  readonly positional: readonly string[];
  /** Long-form flags. Boolean flags carry `true`. */
  readonly flags: Readonly<Record<string, string | boolean>>;
  /** Helper: positional[0] split on comma + trimmed. Empty when no positional[0]. */
  readonly symbols: readonly string[];
  /** Original command line post-substitution. Scripts can re-parse if needed. */
  readonly raw: string;
}
