/**
 * accountResolver — implements SPEC §2.6 account-resolution precedence.
 *
 * Pure function. Resolves and validates the target account for any CoreApi
 * call that accepts `opts?.account`. Three rules, top-down:
 *
 *   1. Explicit `opts.account` wins.
 *   2. Otherwise, if `capabilities.accounts` has exactly one entry, use it
 *      (unambiguous case).
 *   3. Otherwise, fall back to the host's active account.
 *
 * The resolved account is then validated against `capabilities.accounts` —
 * `Error: account 'X' not in script capabilities` on mismatch.
 *
 * EXCEPTION — empty allowlist means "no restriction". An empty
 * `capabilities.accounts: []` is the author-only default: the script
 * operates on whatever account the host has active, with no allowlist
 * check. This mirrors `reads: undefined` ("all reads permitted") and is
 * what makes the shipped `hello.ts` template runnable out of the box. To
 * pin a script to specific accounts, list them — a non-empty list is
 * enforced strictly. (There is no `'*'` wildcard; that was deleted from
 * the SPEC — empty-list-means-active is the sanctioned escape hatch.)
 *
 * The dispatcher (Phase 2 runScript) runs this once per call and hands the
 * resolved account to the adapter so the adapter doesn't need to know about
 * resolution at all.
 */
import type { Capabilities } from '../api/capabilities.types';

export const resolveAccount = (
  opts: { account?: string } | undefined,
  capabilities: Pick<Capabilities, 'accounts'>,
  activeAccount: string,
): string => {
  let resolved: string | undefined = opts?.account;

  if (!resolved && capabilities.accounts.length === 1) {
    resolved = capabilities.accounts[0];
  }

  if (!resolved) {
    resolved = activeAccount;
  }

  if (!resolved) {
    throw new Error('account: no explicit, single-capability, or active account available');
  }

  // Empty allowlist = author-only default: no restriction, use the resolved
  // (active) account as-is. See the header comment.
  if (capabilities.accounts.length === 0) {
    return resolved;
  }

  if (!capabilities.accounts.includes(resolved)) {
    throw new Error(`account '${resolved}' not in script capabilities`);
  }

  return resolved;
};
