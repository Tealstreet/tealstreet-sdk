/** All CLI persistence belongs to one authenticated subject within one brand/profile. */
import { AsyncLocalStorage } from 'node:async_hooks';
import { createHash, randomUUID } from 'node:crypto';
import {
  copyFileSync,
  chmodSync,
  linkSync,
  unlinkSync,
  constants,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readlinkSync,
  readdirSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { BRAND, brandEnv } from './brand';

const homeDir = join(homedir(), BRAND.configDirName);
const legacyConfig = resolve(brandEnv('CONFIG_FILE') || join(homeDir, 'config.json'));
const root = resolve(brandEnv('CONFIG_DIR') || dirname(legacyConfig));
const profile = createHash('sha256')
  .update(JSON.stringify([BRAND.id, legacyConfig, process.env.TEALSTREET_API || 'http://localhost:3000']))
  .digest('hex');
const profiles = join(root, 'profiles', profile);
const selector = join(profiles, 'session.json');
const claimFile = join(profiles, 'legacy-owner.json');
// Local-only state persists across invocations, but is never adopted by login.
const guestId = 'local';
let owner: string | null;
let epoch = 0;
const context = new AsyncLocalStorage<number>();
const listeners = new Set<() => void>();

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function readJson(file: string): unknown {
  try {
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
}
function selectedLegacyConfig() {
  let present = false;
  let linked = false;
  try {
    const entry = lstatSync(legacyConfig);
    present = true;
    linked = entry.isSymbolicLink();
    const file = linked
      ? root === dirname(legacyConfig)
        ? resolveLegacyEntry(realpathSync(root), basename(legacyConfig))
        : null
      : entry.isFile()
        ? legacyConfig
        : null;
    return { present: true, linked, snapshot: file ? readJson(file) : null };
  } catch {
    return { present, linked, snapshot: null };
  }
}
// Freeze the explicitly selected config after resolving any in-root link. The
// same snapshot establishes ownership and supplies the eventual private copy.
const selectedConfig = selectedLegacyConfig();
export function authOwner(auth: unknown): string | null {
  if (!isRecord(auth)) return null;
  if (typeof auth.userId === 'string' && auth.userId) return auth.userId;
  if (typeof auth.accessToken !== 'string') return null;
  try {
    const payloadPart = auth.accessToken.split('.')[1];
    if (!payloadPart) return null;
    const payload: unknown = JSON.parse(Buffer.from(payloadPart, 'base64url').toString());
    if (!isRecord(payload)) return null;
    const subject = payload.userId ?? payload.sub;
    return typeof subject === 'string' && subject ? subject : null;
  } catch {
    return null;
  }
}
function privateDir(dir: string) {
  mkdirSync(dir, { recursive: true, mode: 0o700 });
}
function writeJson(file: string, value: unknown) {
  privateDir(dirname(file));
  const temp = `${file}.${randomUUID()}.tmp`;
  writeFileSync(temp, JSON.stringify(value), { mode: 0o600 });
  renameSync(temp, file);
}

function writeExclusive(file: string, write: (temp: string) => void): void {
  const temp = `${file}.${randomUUID()}.tmp`;
  try {
    write(temp);
    linkSync(temp, file);
  } catch (error) {
    if (!isRecord(error) || error.code !== 'EEXIST') throw error;
  } finally {
    if (existsSync(temp)) unlinkSync(temp);
  }
}

// Freeze provenance before any new login can overwrite it. An exclusive claim
// makes two concurrently launched profiles agree; unknown ownership stays unknown.
function legacyOwner(): string | null {
  if (!existsSync(claimFile)) {
    const config = selectedConfig.snapshot;
    let candidate = isRecord(config) ? authOwner(config.auth) : null;
    const aliases = readJson(join(root, 'aliases-sync.json'));
    if (isRecord(aliases) && typeof aliases.owner === 'string' && aliases.owner !== candidate) candidate = null;
    privateDir(profiles);
    writeExclusive(claimFile, (temp) =>
      writeFileSync(
        temp,
        JSON.stringify({ owner: candidate, hadConfig: isRecord(config) && Array.isArray(config.accounts) }),
        { mode: 0o600 },
      ),
    );
  }
  const claim = readJson(claimFile);
  return isRecord(claim) && typeof claim.owner === 'string' ? claim.owner : null;
}
const originalOwner = legacyOwner();

// Walk link targets one component at a time. Never stat/read a target outside
// the physical source, even through an intermediate directory symlink.
function resolveLegacyEntry(physicalRoot: string, name: string): string | null {
  let directory = physicalRoot;
  let pending = [name];
  let links = 0;
  try {
    while (pending.length) {
      const part = pending.shift();
      if (!part || part === '.') continue;
      if (part === '..') {
        if (directory === physicalRoot) return null;
        directory = dirname(directory);
        continue;
      }
      const candidate = join(directory, part);
      const stat = lstatSync(candidate);
      if (stat.isSymbolicLink()) {
        if (++links > 40) return null;
        const target = readlinkSync(candidate);
        if (isAbsolute(target)) {
          // Conservatively refuse non-canonical absolute targets. Relative
          // targets with '..' are checked component by component above.
          if (target.split(sep).includes('..')) return null;
          const within = relative(physicalRoot, target);
          if (isAbsolute(within) || within === '..' || within.startsWith(`..${sep}`)) return null;
          directory = physicalRoot;
          pending = [...within.split(sep), ...pending];
        } else {
          pending = [...target.split(sep), ...pending];
        }
      } else {
        if (pending.length && !stat.isDirectory()) return null;
        directory = candidate;
      }
    }
    return lstatSync(directory).isFile() ? directory : null;
  } catch {
    return null;
  }
}

// A config file proves ownership of that file, not every sibling file. Freeze
// one claim for the physical shared source, across all config profiles/brands.
// Conflicting or unknown config owners leave the shared files quarantined.
function sharedLegacyOwner(): string | null {
  if (root !== dirname(legacyConfig)) return null;
  const physicalRoot = realpathSync(root);
  const sharedClaim = join(physicalRoot, '.legacy-state-owner.json');
  const authority = JSON.stringify([BRAND.id, process.env.TEALSTREET_API || 'http://localhost:3000']);
  if (!existsSync(sharedClaim)) {
    const owners = new Set<string>();
    let ambiguous = false;
    for (const entry of readdirSync(physicalRoot, { withFileTypes: true })) {
      if (!entry.isFile() && !entry.isSymbolicLink()) continue;
      const file = resolveLegacyEntry(physicalRoot, entry.name);
      if (!file) {
        ambiguous = true;
        continue;
      }
      const value = readJson(file);
      if (!isRecord(value) || !Array.isArray(value.accounts)) continue;
      const id = authOwner(value.auth);
      if (id) owners.add(id);
      else ambiguous = true;
    }
    // Honor earlier releases' claims even if their config was since replaced.
    const oldProfiles = join(physicalRoot, 'profiles');
    if (existsSync(oldProfiles)) {
      for (const entry of readdirSync(oldProfiles, { withFileTypes: true })) {
        if (!entry.isDirectory()) continue;
        const prior = readJson(join(oldProfiles, entry.name, 'legacy-owner.json'));
        if (!isRecord(prior)) continue;
        if (typeof prior.owner === 'string') owners.add(prior.owner);
        else if (prior.hadConfig !== false) ambiguous = true;
      }
    }
    const aliases = readJson(join(physicalRoot, 'aliases-sync.json'));
    if (isRecord(aliases) && typeof aliases.owner === 'string') owners.add(aliases.owner);
    const onlyOwner = owners.size === 1 && !ambiguous ? [...owners][0] : null;
    writeExclusive(sharedClaim, (temp) =>
      writeFileSync(temp, JSON.stringify({ owner: onlyOwner, authority }), { mode: 0o600 }),
    );
  }
  const claim = readJson(sharedClaim);
  return isRecord(claim) && claim.authority === authority && typeof claim.owner === 'string' ? claim.owner : null;
}
const originalSharedOwner = sharedLegacyOwner();
const originalConfigOwned =
  originalOwner !== null &&
  isRecord(selectedConfig.snapshot) &&
  Array.isArray(selectedConfig.snapshot.accounts) &&
  authOwner(selectedConfig.snapshot.auth) === originalOwner &&
  (!selectedConfig.linked || originalSharedOwner === originalOwner);
let migrationIncomplete = false;

const selected = readJson(selector);
owner = isRecord(selected) ? (typeof selected.owner === 'string' ? selected.owner : null) : originalOwner;

function directory(subject: string | null): string {
  const key = subject ? createHash('sha256').update(subject).digest('hex') : guestId;
  return join(profiles, 'users', key);
}
function copyOwnedFile(from: string, to: string) {
  if (!existsSync(from) || !lstatSync(from).isFile()) return;
  writeExclusive(to, (temp) => {
    copyFileSync(from, temp, constants.COPYFILE_EXCL);
    chmodSync(temp, 0o600);
  });
}
function migrate(subject: string | null) {
  const target = directory(subject);
  privateDir(target);
  if (!subject || subject !== originalOwner) return;
  // The config's recorded owner establishes provenance for its directory, not
  // an unrelated default home directory when CONFIG_FILE selects another one.
  const marker = join(target, '.legacy-migrated');
  const configFile = join(target, 'config.json');
  const hasOwnedConfig = () => {
    const config = readJson(configFile);
    return isRecord(config) && Array.isArray(config.accounts) && authOwner(config.auth) === subject;
  };
  if (existsSync(marker)) {
    // Earlier versions could publish this marker after skipping a selected
    // symlink. Warn about that empty profile without overwriting its contents.
    if (selectedConfig.linked && !hasOwnedConfig()) migrationIncomplete = true;
    return;
  }
  if (!originalConfigOwned) {
    migrationIncomplete = true;
    return;
  }
  writeExclusive(configFile, (temp) => writeFileSync(temp, JSON.stringify(selectedConfig.snapshot), { mode: 0o600 }));
  if (!hasOwnedConfig()) {
    migrationIncomplete = true;
    return;
  }
  if (root !== dirname(legacyConfig) || subject !== originalSharedOwner) {
    writeJson(marker, { owner: subject });
    return;
  }
  for (const name of [
    'vars.json',
    'aliases.json',
    'aliases-sync.json',
    'whitelist.json',
    'fatfingers.json',
    'simulation.json',
    'budget.json',
    'tasks.json',
    'cross-trade.json',
    'listener.json',
    'trusted-clients.json',
    'hooks.json',
    'audit.json',
    'audit.log',
    'audit.log.1',
    'cli.log',
    'cli_history',
    'cli-export.json',
    'update-check.json',
  ]) {
    copyOwnedFile(join(root, name), join(target, name));
  }
  const scripts = join(root, 'scripts');
  if (existsSync(scripts) && lstatSync(scripts).isDirectory()) {
    privateDir(join(target, 'scripts'));
    for (const entry of readdirSync(scripts, { withFileTypes: true })) {
      if (entry.isFile()) copyOwnedFile(join(scripts, entry.name), join(target, 'scripts', entry.name));
    }
  }
  writeJson(marker, { owner: subject });
}
migrate(owner);

export class StorageSessionCancelledError extends Error {
  constructor(message = 'CLI login changed; previous-session work was cancelled.') {
    super(message);
    this.name = 'StorageSessionCancelledError';
  }
}
export function assertStorageSession(expected = context.getStore() ?? epoch): void {
  if (expected !== epoch) throw new StorageSessionCancelledError();
}
export function isStorageSessionCurrent(expected = context.getStore() ?? epoch): boolean {
  return expected === epoch && (context.getStore() === undefined || context.getStore() === epoch);
}
export function getStorageEpoch(): number {
  assertStorageSession();
  return epoch;
}
export function getStorageOwner(): string | null {
  assertStorageSession();
  return owner;
}
export function getStorageDir(): string {
  assertStorageSession();
  return directory(owner);
}
export function getStoragePath(name: string): string {
  return join(getStorageDir(), name);
}
export function getOwnedLegacyDatabase(): string | null {
  return !migrationIncomplete &&
    (!selectedConfig.linked || originalConfigOwned) &&
    owner &&
    owner === originalSharedOwner &&
    root === dirname(legacyConfig)
    ? join(root, 'market-data.db')
    : null;
}
export function onStorageOwnerChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
export function runInStorageSession<T>(fn: () => T): T {
  return context.run(getStorageEpoch(), fn);
}
/** Bind callbacks, including ones invoked outside ALS, to their creating login. */
export function captureStorageSession() {
  const capturedEpoch = getStorageEpoch();
  const capturedOwner = getStorageOwner();
  const assertCurrent = () => {
    assertStorageSession(capturedEpoch);
    if (owner !== capturedOwner) throw new StorageSessionCancelledError();
  };
  return {
    owner: capturedOwner,
    epoch: capturedEpoch,
    isCurrent: () => owner === capturedOwner && isStorageSessionCurrent(capturedEpoch),
    assertCurrent,
    run<T>(fn: () => T): T {
      assertCurrent();
      return context.run(capturedEpoch, fn);
    },
  };
}
export function selectStorageOwner(subject: string | null): void {
  assertStorageSession();
  if (subject === owner) return;
  migrate(subject);
  writeJson(selector, { owner: subject });
  owner = subject;
  epoch++;
  // Transition callbacks operate in the new context, including when login was
  // invoked inside a command belonging to the old session.
  context.run(epoch, () => {
    let failed = false;
    for (const listener of listeners) {
      try {
        listener();
      } catch {
        failed = true;
      }
    }
    if (failed)
      console.error(
        'CLI login changed; some session resources could not stop cleanly. Restart the CLI before continuing.',
      );
  });
  if (context.getStore() !== undefined) context.enterWith(epoch);
}

export function legacyDataRequiresReentry(): boolean {
  return (
    selectedConfig.present &&
    (originalOwner === null ||
      originalSharedOwner !== originalOwner ||
      (selectedConfig.linked && !originalConfigOwned) ||
      migrationIncomplete)
  );
}

export class QueuedSessionCancelledError extends StorageSessionCancelledError {
  constructor() {
    super('Cancelled queued command because the CLI login changed.');
    this.name = 'QueuedSessionCancelledError';
  }
}

/** Capture authority when work is enqueued, before any wait behind another job. */
export function enqueueSessionWork(
  previous: Promise<void>,
  run: () => Promise<void> | void,
  reportCancellation: (message: string) => void,
): Promise<void> {
  const epoch = getStorageEpoch();
  return new Promise<void>((resolve, reject) => {
    let waiting = true;
    const unsubscribe = onStorageOwnerChange(() => {
      if (!waiting || isStorageSessionCurrent(epoch)) return;
      waiting = false;
      unsubscribe();
      const error = new QueuedSessionCancelledError();
      // Transition listeners run in the NEW session so the cancellation notice
      // does not try to write a log through the invalidated old async context.
      try {
        reportCancellation(error.message);
      } finally {
        reject(error);
      }
    });
    void previous.then(
      () => {
        if (!waiting) return;
        waiting = false;
        unsubscribe();
        try {
          assertStorageSession(epoch);
          Promise.resolve(runInStorageSession(run)).then(resolve, reject);
        } catch (error) {
          reject(error);
        }
      },
      (error: unknown) => {
        if (!waiting) return;
        waiting = false;
        unsubscribe();
        reject(error);
      },
    );
  });
}
