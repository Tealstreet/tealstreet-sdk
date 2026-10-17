import { AsyncLocalStorage } from 'node:async_hooks';
import { randomBytes } from 'crypto';
import {
  appendFileSync,
  closeSync,
  existsSync,
  fstatSync,
  mkdirSync,
  openSync,
  readSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { logger } from './logger';
import { BRAND } from './brand';

/**
 * Durable, append-only audit log of write-op COMMANDS for the standalone
 * CLI (TSBUG-107, first slice). When enabled, every order-placing /
 * cancel / close / leverage command dispatched through the REPL is
 * appended as one JSON line to `~/.tealstreet/audit.log` with the
 * command, account, and result — so a scripted / automated session
 * leaves an append-only trail you can reconstruct after the fact. (It is
 * append-only, not cryptographically tamper-evident — a later slice
 * could hash-chain lines if that's needed.) Script-driven writes are
 * captured too: `recordWriteOp` covers REPL commands, while
 * `recordScriptWrite` (called from the script-adapter write methods +
 * the runner's script-invocation path) covers `script:<name>` internal
 * write ops — orders, cancels, updates, closes, leverage — tagged
 * `[script]`.
 *
 * Command lineage (TSBUG-107, slice 3): every entry carries a `runId`
 * shared by all writes from a single top-level invocation. The runner
 * wraps each `processCommand` chain in `runWithAuditRunId(newRunId(),
 * …)`; the id is an AsyncLocalStorage value, so it flows automatically
 * into every `appendAuditEntry` reached during that run — REPL commands,
 * a `repeat 10`'s expanded steps, a script invocation AND its internal
 * order writes (which run inside the same async scope) — without
 * threading an id through the adapter signatures. Outside any wrapped
 * run the id is simply absent.
 *
 * Fill attribution (TSBUG-107, slice 4): write ops that return order ids
 * are remembered in a bounded in-memory map; when a `fill` event arrives
 * for one of those ids, `recordFill` appends a `[fill]` entry stamped
 * with the SAME runId + a pointer to the originating command, so the log
 * shows what actually executed grouped with what was sent. Unattributable
 * fills (placed before audit was on, or in a prior process) are skipped.
 *
 * Off by default. The enabled flag persists to `~/.tealstreet/audit.json`
 * (`{ enabled: boolean }`). CLI-only: there is no filesystem on the web
 * terminal, so this doesn't touch cli-core / web.
 *
 * Rotation + bounded read (TSBUG-107, slice 5): `audit.log` is still
 * append-only, but it's size-capped — once it passes `maxLogBytes` it is
 * rolled to `audit.log.1` (single backup, replacing any prior one) and a
 * fresh `audit.log` starts, so disk use stays bounded at ~2× the cap. The
 * size is read via `statSync` per append (not a process-local counter) so
 * concurrent CLI sessions sharing the file can't drift it past the cap.
 * `readAuditTail` reads backwards from the end of the live log (widening
 * the window if malformed tail lines hide valid entries, and dipping into
 * the rotated backup only when a recent rotation left the live file
 * short), never slurping the whole file.
 */
const CONFIG_DIR = process.env.TEALSTREET_CONFIG_DIR || join(homedir(), BRAND.configDirName);
const AUDIT_FLAG_FILE = join(CONFIG_DIR, 'audit.json');
const AUDIT_LOG_FILE = join(CONFIG_DIR, 'audit.log');
const ROTATED_LOG_FILE = AUDIT_LOG_FILE + '.1';

/** Size cap for the live log; past this it rolls to `audit.log.1`. Mutable
 *  only so tests can shrink it — production keeps the 5 MB default. */
let maxLogBytes = 5 * 1024 * 1024;

export interface AuditEntry {
  /** ISO-8601 timestamp. */
  ts: string;
  /**
   * Correlation id shared by every write from one top-level invocation
   * (a typed line / chain / `repeat` / script run). Resolved from the
   * ambient run scope at append time when not set explicitly; absent
   * for writes that happen outside any `runWithAuditRunId` scope.
   */
  runId?: string;
  account: string;
  /** The fully-substituted command string as dispatched. */
  command: string;
  /** True when the command succeeded (no error). */
  ok: boolean;
  /** Order IDs returned by the exchange, when the command produced any. */
  orderIds?: string[];
  /** Error message when the command failed. */
  error?: string;
}

let enabledCache: boolean | undefined;

/**
 * Ambient correlation-id scope for one top-level invocation. The runner
 * enters it once per `processCommand`; `appendAuditEntry` reads it so
 * every write produced during that run — including fire-and-forget `&`
 * steps that settle after the runner returns, and a script's internal
 * order writes — shares the same `runId` without explicit plumbing.
 */
const runIdStore = new AsyncLocalStorage<string>();

/** A short, log-friendly correlation id (8 hex chars). */
export function newRunId(): string {
  return randomBytes(4).toString('hex');
}

/** Run `fn` with `runId` as the ambient audit correlation id. */
export function runWithAuditRunId<T>(runId: string, fn: () => T): T {
  return runIdStore.run(runId, fn);
}

/** The correlation id of the enclosing run, or undefined outside one. */
export function currentAuditRunId(): string | undefined {
  return runIdStore.getStore();
}

/**
 * Order → lineage map for fill attribution (TSBUG-107, slice 4). When a
 * write op returns order IDs we remember each id's run/command/account;
 * when a `fill` event later arrives for one of those ids, `recordFill`
 * appends a `[fill]` entry stamped with the SAME runId + a pointer to
 * the originating command — so the audit log shows what actually
 * executed, grouped with what was sent. Bounded (oldest evicted past the
 * cap) and only populated while auditing is on, so it can't grow
 * unbounded. In-memory only: fills for orders placed in a prior process
 * simply aren't attributed (they predate the map).
 */
interface OrderLineage {
  runId?: string;
  command: string;
  account: string;
}
const MAX_LINEAGE = 5000;
const orderLineage = new Map<string, OrderLineage>();

function rememberOrderLineage(orderIds: string[], command: string, account: string): void {
  const runId = currentAuditRunId();
  for (const id of orderIds) {
    if (!id) continue;
    orderLineage.set(id, { runId, command, account });
  }
  while (orderLineage.size > MAX_LINEAGE) {
    const oldest = orderLineage.keys().next().value;
    if (oldest === undefined) break;
    orderLineage.delete(oldest);
  }
}

function ensureDir(): void {
  if (!existsSync(CONFIG_DIR)) mkdirSync(CONFIG_DIR, { recursive: true });
}

/** Whether the audit log is currently on. Loaded once from disk, then cached. */
export function isAuditEnabled(): boolean {
  if (enabledCache !== undefined) return enabledCache;
  let enabled = false;
  if (existsSync(AUDIT_FLAG_FILE)) {
    try {
      const parsed = JSON.parse(readFileSync(AUDIT_FLAG_FILE, 'utf-8'));
      enabled = typeof parsed?.enabled === 'boolean' ? parsed.enabled : false;
    } catch (e) {
      logger.warn(`Failed to load ${AUDIT_FLAG_FILE}: ${(e as Error).message}`);
    }
  }
  enabledCache = enabled;
  return enabled;
}

export function setAuditEnabled(on: boolean): void {
  enabledCache = !!on;
  try {
    ensureDir();
    writeFileSync(AUDIT_FLAG_FILE, JSON.stringify({ enabled: !!on }, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${AUDIT_FLAG_FILE}: ${(e as Error).message}`);
  }
}

/** Append one entry as a JSON line. No-op when auditing is off. The
 *  correlation id defaults from the ambient run scope only when the
 *  `runId` key is ABSENT from the entry — a caller that sets it
 *  explicitly (e.g. `recordFill`, whose lineage may legitimately have
 *  no runId) opts out of the ambient fallback, so an unattributed fill
 *  that happens to fire inside an unrelated command's run scope can't be
 *  mis-stamped with that scope's id. */
export function appendAuditEntry(entry: AuditEntry): void {
  if (!isAuditEnabled()) return;
  try {
    ensureDir();
    const runId = 'runId' in entry ? entry.runId : currentAuditRunId();
    const full = runId ? { ...entry, runId } : entry;
    appendFileSync(AUDIT_LOG_FILE, JSON.stringify(full) + '\n', 'utf-8');
    // Stat the real file (not a process-local counter) so two CLI sessions
    // sharing ~/.tealstreet/audit.log can't drift a cached size and miss
    // the cap. Write-ops are low-frequency, so a stat per append is cheap.
    if (statSync(AUDIT_LOG_FILE).size > maxLogBytes) rotate();
  } catch (e) {
    // Auditing must never break the trade path — log the failure and move on.
    logger.warn(`Failed to append audit entry: ${(e as Error).message}`);
  }
}

/** Roll the live log to `audit.log.1` once it passes the size cap. Unlinks
 *  any existing backup first — Windows `rename` won't overwrite — then a
 *  fresh `audit.log` starts on the next append. Never throws (audit must
 *  not break the trade path). */
function rotate(): void {
  try {
    if (existsSync(ROTATED_LOG_FILE)) unlinkSync(ROTATED_LOG_FILE);
    renameSync(AUDIT_LOG_FILE, ROTATED_LOG_FILE);
  } catch (e) {
    logger.warn(`Failed to rotate audit log: ${(e as Error).message}`);
  }
}

/** Read up to the last `maxLines` newline-delimited lines of `file` by
 *  scanning backwards in chunks — never reads more than it needs. Decodes
 *  once (concatenated buffers) so a multi-byte char split across a chunk
 *  boundary isn't corrupted. `atBof` is true when the scan reached the
 *  start of the file (so the caller knows there's nothing older to read).
 *  Returns the lines in file order. */
function readTailLines(file: string, maxLines: number): { lines: string[]; atBof: boolean } {
  if (!existsSync(file)) return { lines: [], atBof: true };
  const CHUNK = 16 * 1024;
  const fd = openSync(file, 'r');
  try {
    let pos = fstatSync(fd).size;
    const parts: Buffer[] = [];
    let newlines = 0;
    while (pos > 0 && newlines <= maxLines) {
      const readSize = Math.min(CHUNK, pos);
      pos -= readSize;
      const buf = Buffer.alloc(readSize);
      // Slice to bytesRead — a short read would otherwise leave trailing
      // NUL bytes that corrupt the decode + newline count.
      const bytesRead = readSync(fd, buf, 0, readSize, pos);
      const chunk = bytesRead === readSize ? buf : buf.subarray(0, bytesRead);
      parts.unshift(chunk);
      for (let i = 0; i < bytesRead; i++) if (chunk[i] === 0x0a) newlines++;
    }
    const lines = Buffer.concat(parts)
      .toString('utf-8')
      .split('\n')
      .filter((l) => l.trim());
    return { lines, atBof: pos === 0 };
  } finally {
    closeSync(fd);
  }
}

function parseEntries(lines: string[]): AuditEntry[] {
  const parsed: AuditEntry[] = [];
  for (const line of lines) {
    try {
      parsed.push(JSON.parse(line) as AuditEntry);
    } catch {
      // skip malformed line
    }
  }
  return parsed;
}

/** Read the last `n` VALID entries (newest last). Scans backwards from the
 *  end of the live log, widening the window if malformed lines in the tail
 *  leave it short of `n` valid entries (so a run of bad JSON can't hide
 *  older valid entries), until it has enough or reaches the start of the
 *  file. If a recent rotation still leaves it short, dips into
 *  `audit.log.1` for the remainder — never slurps a whole file in one go.
 *  `n` is clamped to a sane positive integer (default 20). */
export function readAuditTail(n: number): AuditEntry[] {
  const count = Number.isFinite(n) && n > 0 ? Math.floor(n) : 20;
  try {
    let parsed: AuditEntry[] = [];
    for (let want = count; ; want *= 4) {
      const { lines, atBof } = readTailLines(AUDIT_LOG_FILE, want);
      parsed = parseEntries(lines);
      if (parsed.length >= count || atBof || want >= 1_000_000) break;
    }
    if (parsed.length < count && existsSync(ROTATED_LOG_FILE)) {
      const older = parseEntries(readTailLines(ROTATED_LOG_FILE, count - parsed.length).lines);
      parsed = older.concat(parsed);
    }
    return parsed.slice(-count);
  } catch (e) {
    logger.warn(`Failed to read ${AUDIT_LOG_FILE}: ${(e as Error).message}`);
    return [];
  }
}

export const AUDIT_LOG_PATH = AUDIT_LOG_FILE;

/** Command heads recorded by the write-op audit log. Reads (positions /
 *  orders / margin / watch / help …) are intentionally excluded — only
 *  exposure-affecting writes leave a trail. Includes command aliases
 *  (`b`/`s`/`c`/`x`/`mv`/`lev`) so the same op can't slip the audit
 *  depending on which spelling the user typed. */
export const AUDIT_WRITE_HEADS = new Set([
  'buy',
  'b',
  'sell',
  's',
  'close',
  'c',
  'cancel',
  'x',
  'move',
  'mv',
  'leverage',
  'lev',
  'scale',
  'swarm',
  'chase',
  'twap',
  'nuke',
  'bump',
  'stop',
  // `be` / `trail` place real reduce-only orders and both shipped without an
  // entry here, so `audit tail` showed a user's stop moving with no record of
  // what moved it. An armed `when` makes that worse — the order is placed by
  // something the user is not watching.
  'be',
  'breakeven',
  'trail',
]);

/**
 * Record a cross-trade engine lifecycle / safety event. Tagged
 * `[cross-trade]` in the command field so `audit tail` can filter on
 * the prefix. No-op when audit is off.
 *
 * Used by the daemon for: start, stop, drift events, per-account mute
 * (via failure threshold), and the "both engines running" warning.
 */
export function recordCrossTradeEvent(
  event: string,
  detail: { account?: string; ok?: boolean; error?: string; orderIds?: string[] } = {},
): void {
  if (!isAuditEnabled()) return;
  appendAuditEntry({
    ts: new Date().toISOString(),
    account: detail.account || 'cross-trade',
    command: `[cross-trade] ${event}`,
    ok: detail.ok !== false,
    ...(detail.orderIds?.length ? { orderIds: detail.orderIds } : {}),
    ...(detail.error ? { error: detail.error } : {}),
  });
}

/**
 * Record a dispatched command as a write-op audit entry — the single
 * entry point the CLI runner calls after each command. No-ops unless
 * (a) auditing is on, (b) the command was handled, and (c) its head is
 * a write-op. Maps the result to ok / orderIds / error. Never throws.
 * The runner calls this BEFORE re-throwing a handled failure, so failed
 * writes are logged too.
 */
export function recordWriteOp(
  command: string,
  account: string,
  result: { handled?: boolean; success?: boolean; error?: string; data?: { orderIds?: string[] } },
): void {
  if (!result.handled || !isAuditEnabled()) return;
  const head = command.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
  if (!AUDIT_WRITE_HEADS.has(head)) return;
  const orderIds = result.data?.orderIds;
  if (orderIds?.length) rememberOrderLineage(orderIds, command, account || 'unknown');
  appendAuditEntry({
    ts: new Date().toISOString(),
    account: account || 'unknown',
    command,
    ok: result.success !== false,
    ...(orderIds?.length ? { orderIds } : {}),
    ...(result.success === false && result.error ? { error: result.error } : {}),
  });
}

/**
 * Record a write originating INSIDE a script (TSBUG-107 slice 2). Script
 * order calls go through the script-adapter, not `processSingleCommand`,
 * so `recordWriteOp` never sees them. The script-adapter calls this for
 * each internal order, and the runner calls it once for the script
 * invocation, so a scripted session leaves the same reconstructable
 * trail as hand-typed commands. Entries are prefixed `[script]` to
 * distinguish them. No-op while auditing is off; never throws.
 */
export function recordScriptWrite(
  account: string,
  summary: string,
  extra: { ok?: boolean; orderIds?: string[]; error?: string } = {},
): void {
  if (!isAuditEnabled()) return;
  const command = `[script] ${summary}`;
  if (extra.orderIds?.length) rememberOrderLineage(extra.orderIds, command, account || 'unknown');
  appendAuditEntry({
    ts: new Date().toISOString(),
    account: account || 'unknown',
    command,
    ok: extra.ok !== false,
    ...(extra.orderIds?.length ? { orderIds: extra.orderIds } : {}),
    ...(extra.error ? { error: extra.error } : {}),
  });
}

/**
 * Record an exchange FILL, attributed to the order that produced it
 * (TSBUG-107, slice 4). Looks the order id up in the lineage map and, if
 * found, appends a `[fill]` entry stamped with the originating run's
 * `runId` and a pointer to the command that placed the order — so the
 * log shows what actually executed alongside what was sent. Fills arrive
 * async, outside any run scope, so the `runId` comes from the remembered
 * lineage (set explicitly here — always, even when undefined — so the
 * ambient-scope fallback in `appendAuditEntry` can't mis-stamp it), not
 * the ambient scope. No-op when auditing is off or the fill can't be
 * attributed to a tracked order (e.g. placed before audit was on, or in
 * a prior process). Never throws.
 */
export function recordFill(orderId: string | undefined, summary: string): void {
  if (!isAuditEnabled() || !orderId) return;
  const lineage = orderLineage.get(orderId);
  if (!lineage) return;
  appendAuditEntry({
    ts: new Date().toISOString(),
    runId: lineage.runId, // explicit key (may be undefined) → no ambient fallback
    account: lineage.account, // always non-empty (rememberOrderLineage stores `… || 'unknown'`)
    command: `[fill] ${summary} ← ${lineage.command}`,
    ok: true,
    orderIds: [orderId],
  });
}

/** Reset the in-memory caches — test-only hook. */
export function __resetAuditCacheForTest(): void {
  enabledCache = undefined;
  orderLineage.clear();
}

/** Override the rotation size cap — test-only hook. */
export function __setMaxLogBytesForTest(n: number): void {
  maxLogBytes = n;
}
