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
  readdirSync,
  renameSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
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
const guestId = `guest-${randomUUID()}`;
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
    const config = readJson(legacyConfig);
    let candidate = isRecord(config) ? authOwner(config.auth) : null;
    const aliases = readJson(join(root, 'aliases-sync.json'));
    if (isRecord(aliases) && typeof aliases.owner === 'string' && aliases.owner !== candidate) candidate = null;
    privateDir(profiles);
    writeExclusive(claimFile, (temp) => writeFileSync(temp, JSON.stringify({ owner: candidate }), { mode: 0o600 }));
  }
  const claim = readJson(claimFile);
  return isRecord(claim) && typeof claim.owner === 'string' ? claim.owner : null;
}
const originalOwner = legacyOwner();
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
  if (existsSync(marker)) return;
  copyOwnedFile(legacyConfig, join(target, 'config.json'));
  if (root !== dirname(legacyConfig)) {
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

export function assertStorageSession(expected = context.getStore() ?? epoch): void {
  if (expected !== epoch) throw new Error('CLI login changed; previous-session work was cancelled.');
}
export function isStorageSessionCurrent(expected: number): boolean {
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
  return owner && owner === originalOwner && root === dirname(legacyConfig) ? join(root, 'market-data.db') : null;
}
export function onStorageOwnerChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
export function runInStorageSession<T>(fn: () => T): T {
  return context.run(getStorageEpoch(), fn);
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
  return originalOwner === null && existsSync(legacyConfig);
}
