// Keeps this CLI's aliases in step with the server, so an alias defined in a
// browser (or another machine) resolves here too — which is what makes a
// webhook body's alias work on whichever listener runs it
// (docs/plans/WEBHOOK_RELAY_v1.md phase 5, supabase/cli_aliases.sql).
// REPL-only: one-shot commands must not hold timers open.

import {
  syncAliases,
  type AliasChange,
  type AliasSyncRejection,
  type AliasSyncReport,
  type AliasSyncResponse,
} from '@tealstreet/cli-core';
import { getAuthState, getProxyTokenFromConfig, getTealstreetApiBaseUrl } from './auth';
import { logger } from './logger';
import type { FilePreprocessStore } from './preprocess-store';

const PERIODIC_MS = 5 * 60_000;
const AFTER_CHANGE_MS = 2_000;
const STALE_BEFORE_WEBHOOK_MS = 30_000;

let store: FilePreprocessStore | null = null;
let warn: (message: string) => void = () => {};
let periodicTimer: ReturnType<typeof setTimeout> | undefined;
let changeTimer: ReturnType<typeof setTimeout> | undefined;
let inFlight: Promise<boolean> | null = null;
let lastAttemptAt = 0;

const unref = (timer: ReturnType<typeof setTimeout>) => {
  (timer as { unref?: () => void }).unref?.();
  return timer;
};

async function call(changes: AliasChange[], since: number): Promise<AliasSyncResponse | AliasSyncRejection | null> {
  const token = getProxyTokenFromConfig();
  if (!token) return null;
  try {
    const response = await fetch(`${getTealstreetApiBaseUrl()}/api/cli/aliases`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
      body: JSON.stringify({ changes, since }),
      signal: AbortSignal.timeout(10_000),
    });
    if (response.status === 409) return { rejected: 'too_many_aliases' };
    if (!response.ok) {
      logger.warn(`alias sync: server answered ${response.status}`);
      return null;
    }
    return (await response.json()) as AliasSyncResponse;
  } catch (err) {
    logger.warn(`alias sync: ${(err as Error).message}`);
    return null;
  }
}

/** One round, never two at once. Resolves false when it could not sync. */
export function syncAliasesNow(): Promise<boolean> {
  const current = store;
  if (!current) return Promise.resolve(false);
  if (inFlight) return inFlight;
  const owner = getAuthState().userId;
  if (!owner) return Promise.resolve(false);
  inFlight = syncAliases({
    owner,
    loadState: () => current.getAliasSyncState(),
    saveState: (state) => current.setAliasSyncState(state),
    getLocalAliases: () => current.getAliases(),
    applyRemote: (entries) => current.applyRemoteAliases(entries),
    call,
    report: reportToUser,
  })
    .then((ok) => {
      lastAttemptAt = Date.now();
      return ok;
    })
    .finally(() => {
      inFlight = null;
    });
  return inFlight;
}

/** Before a webhook runs, so an alias defined elsewhere moments ago resolves here. */
export async function syncAliasesIfStale(): Promise<void> {
  // Keyed on the last attempt: an unreachable server must not delay every alert.
  if (Date.now() - lastAttemptAt > STALE_BEFORE_WEBHOOK_MS) await syncAliasesNow();
}

function schedulePeriodic(): void {
  periodicTimer = unref(
    setTimeout(async () => {
      try {
        await syncAliasesNow();
      } finally {
        if (store) schedulePeriodic();
      }
    }, PERIODIC_MS),
  );
}

function reportToUser(report: AliasSyncReport): void {
  if (report.conflicts.length > 0) {
    warn(
      `Aliases: ${report.conflicts.join(', ')} differ from your account's copy and were kept as they are here. ` +
        'Re-run `alias <name> "<command>"` to choose which one every device uses.',
    );
  }
  if (report.skipped.length > 0) warn(`Aliases not synced (invalid name or too long): ${report.skipped.join(', ')}`);
  if (report.overLimit.length > 0) {
    warn(`Aliases not synced — your account is at 500 aliases: ${report.overLimit.join(', ')}`);
  }
}

export function startAliasSync(preprocessStore: FilePreprocessStore, notify: (message: string) => void): void {
  if (store) return;
  store = preprocessStore;
  warn = notify;
  preprocessStore.setAliasChangeListener(() => {
    if (changeTimer) clearTimeout(changeTimer);
    changeTimer = unref(setTimeout(() => void syncAliasesNow(), AFTER_CHANGE_MS));
  });
  void syncAliasesNow();
  schedulePeriodic();
}

export function stopAliasSync(): void {
  store?.setAliasChangeListener(null);
  store = null;
  if (periodicTimer) clearTimeout(periodicTimer);
  if (changeTimer) clearTimeout(changeTimer);
}
