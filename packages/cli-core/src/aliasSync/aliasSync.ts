// One alias sync round, shared by the standalone CLI and the web app
// (supabase/cli_aliases.sql, `/api/cli/aliases`). The server orders changes,
// so a host only keeps a cursor and a queue of the changes it made since its
// last successful round — never device timestamps.

/** A change made on this device, or a row from the server. `null` = deleted. */
export interface AliasChange {
  name: string;
  command: string | null;
}

export interface AliasSyncState {
  /** Server revision this device has seen up to; null before its first sync. */
  cursor: number | null;
  /** Local changes not yet accepted by the server, oldest first, one per name. */
  pending: AliasChange[];
  /**
   * The user this state belongs to. A different user signing in on the same
   * device must start over: the old cursor would skip the new account's rows,
   * and the old queue is someone else's changes.
   */
  owner?: string | null;
  /**
   * False after a user switch: aliases already on this device belong to the
   * previous user, so the first round must not upload them into the new account.
   */
  uploadLocal?: boolean;
}

export interface AliasSyncResponse {
  revision: number;
  entries: AliasChange[];
  /** Names the server rejected (invalid name or body); never stored. */
  skipped?: string[];
}

/** The account already holds the maximum number of aliases. */
export type AliasSyncRejection = { rejected: 'too_many_aliases' };

export interface AliasSyncReport {
  /** First sync only: local aliases that differ from the account's copy, kept as they are here. */
  conflicts: string[];
  /** Names the server would not store. */
  skipped: string[];
  /** Additions dropped because the account is at its alias limit. */
  overLimit: string[];
}

export interface AliasSyncHost {
  /** The signed-in user; sync state recorded for anyone else is discarded. */
  owner: string;
  loadState(): AliasSyncState;
  saveState(state: AliasSyncState): void;
  getLocalAliases(): Record<string, string>;
  /** Apply server rows locally WITHOUT queueing them as new changes. */
  applyRemote(entries: AliasChange[]): void;
  /** One `/api/cli/aliases` round; null when it failed (logged by the host). */
  call(changes: AliasChange[], since: number): Promise<AliasSyncResponse | AliasSyncRejection | null>;
  /** Told about anything the user should know; called only when there is something. */
  report?(report: AliasSyncReport): void;
}

/** Record a local change, replacing any earlier pending change to the same name. */
export function enqueueAliasChange(state: AliasSyncState, change: AliasChange): AliasSyncState {
  return { ...state, pending: [...state.pending.filter((c) => c.name !== change.name), change] };
}

const isRejection = (r: AliasSyncResponse | AliasSyncRejection): r is AliasSyncRejection => 'rejected' in r;

/**
 * One sync round.
 *
 * A device's FIRST round never overwrites anything it already has: an alias
 * that differs from the account's copy keeps its local value and is reported,
 * because until now the CLI and each browser held separate alias sets and the
 * same name could mean different order sizes. Local names the account has never
 * seen are uploaded. Every change made after that is sent and wins, because the
 * server stamps it after everything it already holds.
 */
export async function syncAliases(host: AliasSyncHost): Promise<boolean> {
  const loaded = host.loadState();
  if (loaded.owner && loaded.owner !== host.owner) {
    host.saveState({ cursor: null, pending: [], owner: host.owner, uploadLocal: false });
  }
  const state = host.loadState();
  const sent = state.pending;
  let since = state.cursor ?? 0;
  let changes = sent;
  const received: AliasChange[] = [];
  const conflicts = new Set<string>();

  if (state.cursor === null) {
    const snapshot = await host.call([], 0);
    if (!snapshot) return false;
    if (isRejection(snapshot)) return false;
    const local = host.getLocalAliases();
    const pendingNames = new Set(sent.map((c) => c.name));
    const server = new Map(snapshot.entries.map((e) => [e.name, e.command]));
    for (const [name, command] of Object.entries(local)) {
      if (pendingNames.has(name) || !server.has(name)) continue;
      if (server.get(name) !== command) conflicts.add(name);
    }
    const uploads =
      state.uploadLocal === false
        ? []
        : Object.entries(local)
            .filter(([name]) => !server.has(name) && !pendingNames.has(name))
            .map(([name, command]) => ({ name, command }));
    received.push(...snapshot.entries.filter((e) => !conflicts.has(e.name)));
    since = snapshot.revision;
    changes = [...uploads, ...sent];
  }

  const result = await host.call(changes, since);
  if (!result) return false;
  if (isRejection(result)) {
    // Retrying the same additions can never succeed: drop them (deletions are
    // kept, they only shrink the set) and say which, rather than re-sending
    // the same refused round forever.
    const overLimit = changes.filter((c) => c.command !== null).map((c) => c.name);
    const latest = host.loadState();
    host.saveState({
      ...latest,
      owner: host.owner,
      // Only what THIS round sent is dropped; changes queued while it was in
      // flight wait for the next round.
      pending: latest.pending.filter(
        (c) => c.command === null || !changes.some((s) => s.name === c.name && s.command === c.command),
      ),
      // A refused first round must not re-upload every local alias each time.
      ...(state.cursor === null ? { uploadLocal: false } : {}),
    });
    host.report?.({ conflicts: [], skipped: [], overLimit });
    return false;
  }
  // Only the first snapshot is filtered for conflicts: anything in THIS
  // response changed after it, on purpose, somewhere, and applies.
  received.push(...result.entries);

  // Changes queued while the round was in flight stay queued for the next one,
  // and the server's older value for those names must not overwrite them.
  const latest = host.loadState();
  const stillPending = latest.pending.filter((c) => !sent.some((s) => s.name === c.name && s.command === c.command));
  const newerHere = new Set(stillPending.map((c) => c.name));
  host.applyRemote(received.filter((e) => !newerHere.has(e.name)));
  host.saveState({ cursor: result.revision, pending: stillPending, owner: host.owner });

  const skipped = result.skipped ?? [];
  if (conflicts.size > 0 || skipped.length > 0) {
    host.report?.({ conflicts: [...conflicts], skipped, overLimit: [] });
  }
  return true;
}
