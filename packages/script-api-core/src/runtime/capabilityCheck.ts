/**
 * Dispatcher-side capability validators per SPEC §4.
 *
 * The dispatcher fires every gate BEFORE the adapter is called. Denial paths
 * never reach safe-cex (or the host's CLI executor in the runCommand case).
 *
 * Surface:
 *   - validateReadCapability(method, capabilities)
 *   - validateWriteCapability(method, capabilities)
 *   - validateRunCommandCapability(verb, capabilities, depth)
 *   - validateExchangeCapability(exchangeName, capabilities)
 *   - assertWithinStorageQuota(currentBytes, addedBytes, capabilities)
 *   - assertIdempotentForRetry(scriptName, capabilities)
 *
 * Each function throws on denial with a message that surfaces directly to the
 * user via the existing `printError` path. No swallowing, no soft-fail.
 *
 * Account allowlist is enforced separately in `accountResolver.ts` so a
 * single resolution pass covers both "which account?" and "is this account
 * allowed?".
 */
import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

import type { Capabilities, ReadMethodName, WriteMethodName } from '../api/capabilities.types';

/**
 * runCommand recursion cap (§4.2). Tighter than the alias-expansion cap
 * (MAX_ALIAS_DEPTH=8 in cli-core) because runCommand may dispatch to a script
 * which itself calls runCommand — the deeper budget would let an author
 * easily build a recursive bomb.
 */
export const MAX_RUN_COMMAND_DEPTH = 4;

export const validateReadCapability = (method: ReadMethodName, capabilities: Pick<Capabilities, 'reads'>): void => {
  // SPEC §4.1: undefined `reads` means all reads permitted (v1 author-only default).
  if (!capabilities.reads) return;
  if (!capabilities.reads.includes(method)) {
    throw new Error(`capability denied: read '${method}' not in script capabilities.reads`);
  }
};

export const validateWriteCapability = (method: WriteMethodName, capabilities: Pick<Capabilities, 'writes'>): void => {
  // SPEC §4.1: writes are always explicit; nothing implicit.
  if (!capabilities.writes.includes(method)) {
    throw new Error(`capability denied: write '${method}' not in script capabilities.writes`);
  }
};

/**
 * Validates a runCommand invocation per SPEC §4.2:
 *   - runCommand block must be present + enabled.
 *   - The verb (first whitespace-separated token of the command) must be in
 *     `allowedVerbs`.
 *   - Recursion depth must be ≤ MAX_RUN_COMMAND_DEPTH.
 *
 * `depth` is 1-based: the outermost runCommand call is depth=1; depth=5 is
 * the first denial.
 */
export const validateRunCommandCapability = (
  command: string,
  capabilities: Pick<Capabilities, 'runCommand'>,
  depth: number,
): void => {
  const cap = capabilities.runCommand;
  if (!cap || !cap.enabled) {
    throw new Error('capability denied: runCommand is not enabled for this script');
  }
  if (depth > MAX_RUN_COMMAND_DEPTH) {
    throw new Error(`capability denied: runCommand recursion depth ${depth} exceeds cap ${MAX_RUN_COMMAND_DEPTH}`);
  }
  // Extract the verb — first whitespace-separated token of the user-supplied
  // command. Empty / whitespace-only commands fail loud.
  const verb = command.trim().split(/\s+/)[0] ?? '';
  if (verb.length === 0) {
    throw new Error('capability denied: runCommand requires a verb (got empty command)');
  }
  if (!cap.allowedVerbs.includes(verb)) {
    // Exact-string contract per SPEC §4.2 / team-lead Phase 2 brief.
    throw new Error(`capability denied: runCommand verb '${verb}' not in allowlist`);
  }
};

/**
 * Optional broader gate (§4 capability schema). When `capabilities.exchanges`
 * is set, every action's target exchange must be in the allowlist. When
 * omitted, the per-account allowlist is the only constraint.
 */
export const validateExchangeCapability = (
  exchange: ExchangeName,
  capabilities: Pick<Capabilities, 'exchanges'>,
): void => {
  if (!capabilities.exchanges) return;
  if (!capabilities.exchanges.includes(exchange)) {
    throw new Error(`capability denied: exchange '${exchange}' not in script capabilities.exchanges`);
  }
};

/**
 * Per-script storage quota gate (§4.5). Called by the storage adapter on
 * every `api.storage.set` before write. `currentBytes` is the total size of
 * the script's storage namespace today; `addedBytes` is the delta the
 * incoming write would introduce. Throws when over-quota.
 */
export const assertWithinStorageQuota = (
  currentBytes: number,
  addedBytes: number,
  capabilities: Pick<Capabilities, 'storage'>,
): void => {
  if (!capabilities.storage.enabled) {
    throw new Error('capability denied: storage is not enabled for this script');
  }
  const projected = currentBytes + addedBytes;
  if (projected > capabilities.storage.quotaBytes) {
    throw new Error(
      `storage quota exceeded: write would bring total to ${projected} bytes; cap ${capabilities.storage.quotaBytes}`,
    );
  }
};

/**
 * Idempotent retry guard (§4.3 / §7.5). The cli-core chain layer calls this
 * before re-running a script step under `retry N`. Throws the exact spec
 * error string when the script isn't marked idempotent so the runner can
 * surface it via the existing `handled && success === false` path.
 */
export const assertIdempotentForRetry = (scriptName: string, capabilities: Pick<Capabilities, 'idempotent'>): void => {
  if (capabilities.idempotent !== true) {
    throw new Error(`script '${scriptName}' is not marked idempotent; cannot retry`);
  }
};
