import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import { join, dirname } from 'path';
import type { ExchangeAccountConfig } from '@tealstreet/cli-core';

import type { FooterSegment } from './footer-segments';
import { BRAND, brandEnv } from './brand';

// Re-export the canonical type for convenience
export type { ExchangeAccountConfig };
// Alias for backwards compatibility
export type AccountConfig = ExchangeAccountConfig;

const CONFIG_DIR = join(homedir(), BRAND.configDirName);
// Support custom config file via environment variable (used by Electron desktop app)
const CONFIG_FILE = brandEnv('CONFIG_FILE') || join(CONFIG_DIR, 'config.json');

export function getConfigFilePath(): string {
  return CONFIG_FILE;
}

export function getConfigDirForFile(): string {
  return dirname(CONFIG_FILE);
}

export interface AuthConfig {
  accessToken?: string;
  expiresAt?: number;
  userId?: string;
}

export interface ListenerConfig {
  enabled?: boolean;
  accountSync?: boolean;
  remoteExec?: boolean;
}

export interface FooterConfig {
  /** Master switch for the persistent bottom status footer. On unless explicitly false. */
  enabled?: boolean;
  /** Redraw cadence in ms (live data refresh). Clamped to a sane floor. */
  intervalMs?: number;
  /** Ordered segment layout. When omitted, DEFAULT_SEGMENTS is used. */
  segments?: FooterSegment[];
}

export interface Config {
  defaultAccount?: string;
  defaultSymbol?: string;
  accounts: ExchangeAccountConfig[];
  auth?: AuthConfig;
  listener?: ListenerConfig;
  footer?: FooterConfig;
}

export function getConfigDir(): string {
  return CONFIG_DIR;
}

// config.json holds exchange API keys and the auth token: owner-only.
const PRIVATE_DIR_MODE = 0o700;
const PRIVATE_FILE_MODE = 0o600;

export function loadConfig(): Config {
  const configDir = getConfigDirForFile();
  if (!existsSync(configDir)) {
    mkdirSync(configDir, { recursive: true, mode: PRIVATE_DIR_MODE });
  }

  if (!existsSync(CONFIG_FILE)) {
    return { accounts: [] };
  }

  try {
    return JSON.parse(readFileSync(CONFIG_FILE, 'utf-8'));
  } catch {
    return { accounts: [] };
  }
}

export function saveConfig(config: Config): void {
  const configDir = getConfigDirForFile();
  if (!existsSync(configDir)) {
    mkdirSync(configDir, { recursive: true, mode: PRIVATE_DIR_MODE });
  }
  writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2), { mode: PRIVATE_FILE_MODE });
  // `mode` applies only when the file is created; tighten configs written before.
  chmodSync(CONFIG_FILE, PRIVATE_FILE_MODE);
}

// Tombstone retention: accounts deleted via the listener mutate path keep
// a `deletedAt` marker for this long so the cross-device sync (web ↔ CLI)
// can resolve last-write-wins. GC runs once at listener startup.
export const TOMBSTONE_TTL_MS = 30 * 24 * 60 * 60 * 1000;

function isLive(a: ExchangeAccountConfig): boolean {
  return !a.deletedAt;
}

/**
 * Stamp updatedAt on a mutated account. Callers invoke this when they
 * intend the change to participate in CLI ↔ web LWW sync. Pure local
 * reads (loadConfig, listAccounts) do NOT stamp.
 */
export function stampAccountUpdatedAt(account: ExchangeAccountConfig): ExchangeAccountConfig {
  account.updatedAt = Date.now();
  return account;
}

export function getAccount(name?: string): ExchangeAccountConfig | undefined {
  const config = loadConfig();
  const live = config.accounts.filter(isLive);
  if (name) {
    return live.find((a) => a.name === name);
  }
  return live.find((a) => a.name === config.defaultAccount) || live[0];
}

export function addAccount(account: ExchangeAccountConfig): void {
  const config = loadConfig();
  stampAccountUpdatedAt(account);
  // If a tombstone exists for this name, we resurrect (clear deletedAt).
  const existingIndex = config.accounts.findIndex((a) => a.name === account.name);
  if (existingIndex >= 0) {
    config.accounts[existingIndex] = { ...account, deletedAt: undefined };
  } else {
    config.accounts.push(account);
  }
  // Set as default if it's the first live account
  const liveCount = config.accounts.filter(isLive).length;
  if (liveCount === 1) {
    config.defaultAccount = account.name;
  }
  saveConfig(config);
}

/**
 * Tombstone an account by name. Sets deletedAt + updatedAt so cross-device
 * sync can propagate the delete. Local UX (listAccounts/getActiveAccounts)
 * filters tombstones out. Tombstones are GC'd after TOMBSTONE_TTL_MS.
 *
 * Returns `true` if a local row was tombstoned (or a synthetic tombstone
 * was created when `opts.createTombstoneIfMissing` is set), `false` if
 * the name doesn't exist locally and the synthetic path wasn't opted in.
 *
 * User-facing callers (`tealstreet account remove`) should leave
 * `opts.createTombstoneIfMissing` unset so a typo (`account remove
 * bybitx`) reports "not found" instead of silently creating a junk
 * tombstone row. Listener-side sync that needs to propagate a remote
 * delete for an account the CLI never had should use the dedicated
 * `tombstoneAccount(name, updatedAt)` instead — it preserves the
 * inbound LWW timestamp.
 */
export function removeAccount(name: string, opts?: { createTombstoneIfMissing?: boolean }): boolean {
  const config = loadConfig();
  const now = Date.now();
  const index = config.accounts.findIndex((a) => a.name === name);
  if (index >= 0) {
    config.accounts[index] = {
      ...config.accounts[index],
      deletedAt: now,
      updatedAt: now,
    };
    if (config.defaultAccount === name) {
      const nextLive = config.accounts.find((a) => isLive(a) && a.name !== name);
      config.defaultAccount = nextLive?.name;
    }
    saveConfig(config);
    return true;
  }
  if (!opts?.createTombstoneIfMissing) {
    return false;
  }
  // Synthetic tombstone — only on explicit opt-in.
  config.accounts.push({
    name,
    // Required-by-type fields kept as empty strings — never read because
    // tombstoned rows are filtered out of the live-account paths.
    exchange: '' as ExchangeAccountConfig['exchange'],
    apiKey: '',
    apiSecret: '',
    updatedAt: now,
    deletedAt: now,
  });
  saveConfig(config);
  return true;
}

/**
 * Listener-side raw upsert. Does NOT stamp updatedAt — the listener
 * trusts the timestamp from the wire (after LWW comparison). Use
 * addAccount for local edits where the CLI should own the stamp.
 */
export function upsertAccountRaw(account: ExchangeAccountConfig): void {
  const config = loadConfig();
  const existingIndex = config.accounts.findIndex((a) => a.name === account.name);
  if (existingIndex >= 0) {
    config.accounts[existingIndex] = account;
  } else {
    config.accounts.push(account);
  }
  const liveCount = config.accounts.filter(isLive).length;
  if (liveCount === 1 && !config.defaultAccount) {
    config.defaultAccount = account.name;
  }
  saveConfig(config);
}

/**
 * Listener-side tombstone. Caller supplies the inbound updatedAt (the
 * wire frame's value), which already won the LWW comparison upstream.
 * Creates a tombstone-only row if the name isn't local.
 */
export function tombstoneAccount(name: string, updatedAt: number): void {
  const config = loadConfig();
  const now = Date.now();
  const index = config.accounts.findIndex((a) => a.name === name);
  if (index >= 0) {
    config.accounts[index] = {
      ...config.accounts[index],
      deletedAt: now,
      updatedAt,
    };
    if (config.defaultAccount === name) {
      const nextLive = config.accounts.find((a) => isLive(a) && a.name !== name);
      config.defaultAccount = nextLive?.name;
    }
  } else {
    config.accounts.push({
      name,
      exchange: '' as ExchangeAccountConfig['exchange'],
      apiKey: '',
      apiSecret: '',
      updatedAt,
      deletedAt: now,
    });
  }
  saveConfig(config);
}

/**
 * Purge tombstones older than `maxAgeMs`. Returns count removed.
 */
export function gcTombstonedAccounts(maxAgeMs: number = TOMBSTONE_TTL_MS): number {
  const config = loadConfig();
  const cutoff = Date.now() - maxAgeMs;
  const before = config.accounts.length;
  config.accounts = config.accounts.filter((a) => !a.deletedAt || a.deletedAt > cutoff);
  const removed = before - config.accounts.length;
  if (removed > 0) saveConfig(config);
  return removed;
}

export function setDefaultAccount(name: string): boolean {
  const config = loadConfig();
  const account = config.accounts.find((a) => a.name === name && isLive(a));
  if (account) {
    config.defaultAccount = name;
    saveConfig(config);
    return true;
  }
  return false;
}

export function setDefaultSymbol(symbol: string): void {
  const config = loadConfig();
  config.defaultSymbol = symbol;
  saveConfig(config);
}

/** Live accounts only (tombstones filtered). Use listAllAccounts() to
 *  include tombstones — currently only the listener's state:accounts
 *  snapshot path needs that. */
export function listAccounts(): ExchangeAccountConfig[] {
  return loadConfig().accounts.filter(isLive);
}

/** All accounts including tombstones. Used by listener snapshot path. */
export function listAllAccounts(): ExchangeAccountConfig[] {
  return loadConfig().accounts;
}

/**
 * Update an account's configuration (e.g., active state). Bumps updatedAt
 * so the change participates in cross-device LWW sync.
 */
export function updateAccount(name: string, updates: Partial<ExchangeAccountConfig>): boolean {
  const config = loadConfig();
  const index = config.accounts.findIndex((a) => a.name === name && isLive(a));
  if (index >= 0) {
    config.accounts[index] = stampAccountUpdatedAt({
      ...config.accounts[index],
      ...updates,
    });
    saveConfig(config);
    return true;
  }
  return false;
}

/**
 * Get all accounts that are marked as active (tombstones excluded).
 */
export function getActiveAccounts(): ExchangeAccountConfig[] {
  return loadConfig().accounts.filter((a) => a.active === true && isLive(a));
}
