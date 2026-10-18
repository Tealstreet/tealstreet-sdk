/**
 * File-based script loader for the standalone CLI. See SPEC §5.1 (Phase 3).
 *
 * Scripts live in `~/.tealstreet/scripts/` — one `.ts` file per script.
 * The directory IS the registry; no JSON manifest, no sqlite. On first
 * launch the dir is seeded with a `tsconfig.json`, an ambient
 * `types.d.ts`, and an example `hello.ts` so the user can open the
 * directory in their IDE of choice and get full type completion
 * without any node_modules install (see scripts-templates.ts).
 *
 * **Why dynamic `import()`** — Bun runs `.ts` natively, so we can let
 * Bun handle the transpile + module evaluation and just take the
 * `meta` const + `default` function ref off the module. That avoids
 * the source-string + brace-scanner path entirely, which the in-house
 * Phase 3 plan review flagged as fragile.
 *
 * Phase 3 is explicitly throwaway / migrate-later — the marketplace,
 * cross-device sync, and any per-script `api.storage.*` blobs land in
 * Phase 10. Keep this module small.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { pathToFileURL } from 'url';

import type { ScriptFn, ScriptMeta, ScriptRecord } from '@tealstreet/script-api-core';

import { logger } from './logger';
import { HELLO_TEMPLATE, TSCONFIG_TEMPLATE, TYPES_TEMPLATE } from '@tealstreet/script-authoring/src/scripts-templates';
import { BRAND } from './brand';

const CONFIG_DIR = path.join(os.homedir(), BRAND.configDirName);
export const SCRIPTS_DIR = path.join(CONFIG_DIR, 'scripts');
const TYPES_FILE = path.join(SCRIPTS_DIR, 'types.d.ts');
const TSCONFIG_FILE = path.join(SCRIPTS_DIR, 'tsconfig.json');
const HELLO_FILE = path.join(SCRIPTS_DIR, 'hello.ts');

/**
 * Letters, digits, underscores, hyphens. Must start with a letter or
 * underscore. Phase 5's dispatcher resolves the `script:<name>` prefix
 * exclusively, so this regex only needs to keep filenames as
 * structurally clean tokens — it does NOT need to dodge REPL builtins,
 * because the prefix sidesteps every conflict (SPEC §6.1).
 *
 * Earlier phases included a `RESERVED_SCRIPT_NAMES` set that refused
 * to load `buy.ts`, `tasks.ts`, etc. — that guard is gone since the
 * prefix-only model means `script:buy` is always unambiguous.
 */
const VALID_SCRIPT_NAME = /^[a-zA-Z_][a-zA-Z0-9_-]*$/;

/**
 * A `ScriptRecord` plus the on-disk file path. The host keeps this
 * shape internally; the runtime only needs the `ScriptRecord` slice.
 */
export interface LoadedScript extends ScriptRecord {
  /** Absolute path to the source file. */
  readonly filePath: string;
}

let cache: LoadedScript[] = [];

/**
 * Module-scope monotonic counter for the import cache-buster. Replaces
 * the earlier `Date.now()` token — two `loadScripts()` calls within
 * the same millisecond would otherwise collide and Bun/Node would
 * return the previously memoized module URL, silently missing edits.
 * A counter is collision-free under any timing.
 */
let reloadCounter = 0;

/**
 * Create `~/.tealstreet/scripts/` if missing, refresh `types.d.ts` on
 * every launch (the ambient surface must match this CLI version), and
 * write `tsconfig.json` + `hello.ts` only-if-missing so user edits
 * survive.
 */
export function ensureScriptsDir(): void {
  if (!fs.existsSync(SCRIPTS_DIR)) {
    fs.mkdirSync(SCRIPTS_DIR, { recursive: true });
  }
  // Always refresh the ambient types — they're tied to the running
  // CLI's script-API surface, so stale types would silently mislead
  // the user's editor.
  fs.writeFileSync(TYPES_FILE, TYPES_TEMPLATE);
  if (!fs.existsSync(TSCONFIG_FILE)) {
    fs.writeFileSync(TSCONFIG_FILE, TSCONFIG_TEMPLATE);
  }
  if (!fs.existsSync(HELLO_FILE)) {
    fs.writeFileSync(HELLO_FILE, HELLO_TEMPLATE);
  }
}

interface ScannableEntry {
  readonly name: string;
  readonly file: string;
  readonly absPath: string;
}

/**
 * Filter the scripts directory down to real script files: top-level
 * `.ts` (not `.d.ts`), filename matching the valid-name regex, not
 * hidden. Subdirectories are ignored.
 */
function scanEntries(): ScannableEntry[] {
  const entries = fs.readdirSync(SCRIPTS_DIR, { withFileTypes: true });
  const out: ScannableEntry[] = [];
  for (const entry of entries) {
    if (!entry.isFile()) continue;
    const file = entry.name;
    if (file.startsWith('.')) continue;
    if (!file.endsWith('.ts')) continue;
    if (file.endsWith('.d.ts')) continue;
    const name = file.slice(0, -3);
    if (!VALID_SCRIPT_NAME.test(name)) {
      logger.warn(`scripts: skipping '${file}' — name must match /^[a-zA-Z_][a-zA-Z0-9_-]*$/`);
      continue;
    }
    out.push({ name, file, absPath: path.join(SCRIPTS_DIR, file) });
  }
  return out;
}

/**
 * Validate the shape of an imported module against the loader's
 * contract. Returns the typed `meta` + `fn` pair, or throws a clear
 * error the caller surfaces to the user.
 */
function validateImportedShape(file: string, mod: Record<string, unknown>): { meta: ScriptMeta; fn: ScriptFn } {
  const meta = mod['meta'];
  const fn = mod['default'];
  if (!meta || typeof meta !== 'object') {
    throw new Error(`'${file}' is missing 'export const meta: ScriptMeta = { ... }'`);
  }
  const m = meta as Record<string, unknown>;
  if (!Array.isArray(m.targets)) {
    throw new Error(`'${file}' meta.targets must be an array`);
  }
  if (!m.capabilities || typeof m.capabilities !== 'object') {
    throw new Error(`'${file}' meta.capabilities must be an object`);
  }
  if (typeof fn !== 'function') {
    throw new Error(`'${file}' is missing 'export default async function (args, api) { ... }'`);
  }
  return { meta: meta as ScriptMeta, fn: fn as ScriptFn };
}

/**
 * Promise of the most recent `loadScripts()` call, or `null` before
 * the first call. Phase 5's dispatcher arm awaits this so a
 * `script:foo` typed at cold REPL doesn't race the initial fire-and-
 * forget loader in `startRepl()`.
 *
 * Successive `loadScripts()` calls overwrite this with a fresh
 * promise — `script reload` therefore also acts as a write-barrier
 * for subsequent dispatches.
 */
let loadInFlight: Promise<LoadedScript[]> | null = null;

/**
 * Returns a promise that resolves when the registry has finished its
 * most recent load. Returns `null` if `loadScripts()` has never been
 * called — callers must treat that as "no scripts loaded yet" rather
 * than "still loading".
 *
 * Phase 5's dispatcher uses this to await the cold-REPL initial load
 * before doing `lookupScript()`.
 */
export function awaitScriptsLoaded(): Promise<LoadedScript[]> | null {
  return loadInFlight;
}

/**
 * Scan + load every script in `~/.tealstreet/scripts/`. Caches the
 * result. Call again to re-scan after the user edits a file.
 *
 * Cache-busts the dynamic `import()` via a query-string token so a
 * reload picks up edits — Bun, like Node, memoizes module URLs.
 *
 * Failures are isolated: a malformed script logs a warning and is
 * skipped; the rest of the dir still loads.
 */
export function loadScripts(): Promise<LoadedScript[]> {
  // Publish the in-flight promise SYNCHRONOUSLY so a concurrent
  // `awaitScriptsLoaded()` caller (Phase 5's dispatcher arm at cold
  // REPL) sees it before the body's first `await`.
  const promise = doLoadScripts();
  loadInFlight = promise;
  return promise;
}

async function doLoadScripts(): Promise<LoadedScript[]> {
  ensureScriptsDir();
  const entries = scanEntries();
  reloadCounter += 1;
  const reloadToken = reloadCounter.toString(36);
  const results: LoadedScript[] = [];
  for (const entry of entries) {
    try {
      const url = `${pathToFileURL(entry.absPath).href}?t=${reloadToken}`;
      const mod = (await import(url)) as Record<string, unknown>;
      const { meta, fn } = validateImportedShape(entry.file, mod);
      results.push({
        id: entry.name,
        name: entry.name,
        targets: meta.targets,
        capabilities: meta.capabilities,
        coreApiVersion: meta.coreApiVersion,
        cliApiVersion: meta.cliApiVersion,
        webApiVersion: meta.webApiVersion,
        mobileApiVersion: meta.mobileApiVersion,
        fn,
        filePath: entry.absPath,
      });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      logger.warn(`scripts: failed to load '${entry.file}' — ${message}`);
    }
  }
  cache = results;
  return results;
}

/**
 * Snapshot of the last loaded set. Cheap — does not re-scan disk. Use
 * `loadScripts()` to force a re-scan.
 */
export function listScripts(): readonly LoadedScript[] {
  return cache;
}

/**
 * Resolve a script by name (filename sans `.ts`). Returns `undefined`
 * when not found; the caller decides whether that's a hard error
 * (Phase 5 dispatcher) or a soft fallback.
 */
export function lookupScript(name: string): LoadedScript | undefined {
  return cache.find((s) => s.name === name);
}

/**
 * Test-only: clear the cache + the inflight promise. Production code
 * should call `loadScripts()` to refresh.
 */
export function clearScriptsCache(): void {
  cache = [];
  loadInFlight = null;
}
