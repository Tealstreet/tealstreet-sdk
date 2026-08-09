import type { ExchangeAccountConfig } from '@tealstreet/safe-cex/src/types';

import { listAccounts } from '../config';
import { ExchangeManager, type RunningExchange } from '../exchange-manager';
import { createStandalonePlatformAdapter } from '../platform-adapter';

/**
 * Live read access for the MCP server, connecting lazily.
 *
 * Two properties matter here and both are easy to get wrong.
 *
 * ## 1. Never persist `active` from this process
 *
 * `ExchangeManager.enable()` writes `active: true` back to config.json, and
 * `saveConfig` rewrites the whole file with no lock. The MCP server is a
 * SECOND process that routinely runs alongside the user's REPL, so persisting
 * from here races the REPL's writes: last-writer-wins can clobber edits and
 * resurrect deleted-account tombstones, and it silently changes which accounts
 * the REPL auto-connects next launch. Every connect here passes
 * `persistActive: false`.
 *
 * ## 2. Never hand account credentials to the MCP client
 *
 * `listAccounts()` returns the full `ExchangeAccountConfig`, which carries
 * `apiKey`, `apiSecret`, `passphrase`, `secondary_apiSecret` and
 * `omniKeySeed`. Everything exposed here is an explicit allowlist projection —
 * never a spread, never the raw object.
 */

/**
 * This process owns its own manager, mirroring `cross-trade-daemon.ts`. There
 * is no shared singleton — `repl.ts` keeps a module-private one — and reusing
 * the REPL's would not be possible across processes anyway.
 */
let manager: ExchangeManager | null = null;

const getManager = (): ExchangeManager => {
  if (!manager) {
    const adapter = createStandalonePlatformAdapter();
    manager = new ExchangeManager(adapter);
    adapter.setExchangeManager(manager);
  }
  return manager;
};

/** The live manager, or null when nothing has connected yet. */
export const getExchangeManager = (): ExchangeManager => getManager();

/** Safe, credential-free view of an account. */
export interface AccountSummary {
  name: string;
  exchange: string;
  testnet: boolean;
  connected: boolean;
}

const summarize = (account: ExchangeAccountConfig): AccountSummary => ({
  // Explicit allowlist. Do NOT replace this with a spread — the source object
  // holds API secrets.
  name: account.name,
  exchange: String(account.exchange),
  testnet: Boolean((account as { testnet?: boolean }).testnet),
  connected: Boolean(getManager().get(account.name)),
});

export const listAccountSummaries = (): AccountSummary[] => listAccounts().map(summarize);

/**
 * Ensure an account is streaming, connecting on demand.
 *
 * Stores hydrate asynchronously after `start()`, so a read taken immediately
 * after connecting can legitimately be empty. Callers should say so rather
 * than presenting an empty array as "no positions".
 */
export const ensureConnected = async (accountName: string): Promise<{ ok: true } | { ok: false; error: string }> => {
  if (getManager().get(accountName)) return { ok: true };

  const account = listAccounts().find((a) => a.name === accountName);
  if (!account) {
    const names = listAccounts().map((a) => a.name);
    return { ok: false, error: `No account named '${accountName}'. Known accounts: ${names.join(', ') || '(none)'}` };
  }

  try {
    await getManager().enable(account, { persistActive: false });
    return { ok: true };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    // Proxy/tunnel accounts throw "Run login first" when unauthenticated —
    // surface that verbatim, it is the actionable part.
    return { ok: false, error: `Could not connect '${accountName}': ${message}` };
  }
};

/**
 * Disconnect everything this process brought up.
 *
 * Without it a closed MCP session leaves websockets reconnecting forever in a
 * process nobody is watching.
 */
export const disconnectAll = async (): Promise<void> => {
  if (!manager) return;
  const mgr = manager;
  await Promise.all(mgr.getAll().map((running: RunningExchange) => mgr.disable(running.accountName)));
};
