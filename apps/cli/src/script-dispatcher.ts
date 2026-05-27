/**
 * Pure helpers for the Phase 5 `script:<name>` dispatcher in
 * `repl.ts:runStep`. Lives in its own module so unit tests can pin
 * the parse contract without dragging in `repl.ts`'s top-level
 * adapter/exchange-manager singletons. See SPEC §6.1.
 */

export interface ScriptInvocation {
  /** The script name (filename sans `.ts`). */
  readonly name: string;
  /** Everything after the first whitespace, raw — handed to the script's `args` parser. */
  readonly rawArgs: string;
}

/**
 * Parse a `script:<name> <rest>` command into name + rawArgs. Returns
 * `null` when the prefix is malformed (no name, or `script: ` with a
 * space) — the caller surfaces a clear error message.
 *
 * SPEC §6.1 grammar: no space allowed between `:` and the name.
 * `script:` and `script: foo` are both parse errors. `script:` plus
 * a name with no args is valid (`rawArgs` is the empty string).
 *
 * Caller is responsible for the leading `script:` check — this
 * function assumes it.
 */
export function parseScriptInvocation(command: string): ScriptInvocation | null {
  const afterPrefix = command.slice('script:'.length);
  if (afterPrefix.length === 0) return null;
  // Reject `script: foo` — leading whitespace after the colon is invalid.
  if (/^\s/.test(afterPrefix)) return null;
  const spaceIdx = afterPrefix.search(/\s/);
  if (spaceIdx === -1) {
    return { name: afterPrefix, rawArgs: '' };
  }
  const name = afterPrefix.slice(0, spaceIdx);
  if (name.length === 0) return null;
  const rawArgs = afterPrefix.slice(spaceIdx + 1);
  return { name, rawArgs };
}
