import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { randomUUID } from 'crypto';

import type {
  CrossTradeConfig,
  CrossTradeCopier,
  CrossTradeExecution,
} from '@tealstreet/safe-cex/src/crossTrade/types';
import { BRAND, brandEnv } from './brand';

const CONFIG_DIR = brandEnv('CONFIG_DIR') || join(homedir(), BRAND.configDirName);
const CROSS_TRADE_FILE = join(CONFIG_DIR, 'cross-trade.json');

export const TOMBSTONE_TTL_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * Stored shape extends safe-cex's CrossTradeConfig with id + LWW fields. The
 * file lives at ~/.tealstreet/cross-trade.json (override via
 * TEALSTREET_CONFIG_DIR for Electron / tests). Tombstones (`deletedAt`) carry
 * the soft delete forward so the listener bridge can resolve cross-device
 * sync against the web side without losing intent.
 */
export type StoredCrossTradeConfig = CrossTradeConfig & {
  id: string;
  updatedAt: number;
  deletedAt?: number;
};

export interface CrossTradeFile {
  configs: StoredCrossTradeConfig[];
}

export function getCrossTradeFilePath(): string {
  return CROSS_TRADE_FILE;
}

function isLive(c: StoredCrossTradeConfig): boolean {
  return !c.deletedAt;
}

function ensureDir(): void {
  if (!existsSync(CONFIG_DIR)) {
    mkdirSync(CONFIG_DIR, { recursive: true });
  }
}

export function loadCrossTradeFile(): CrossTradeFile {
  ensureDir();
  if (!existsSync(CROSS_TRADE_FILE)) {
    return { configs: [] };
  }
  try {
    const parsed = JSON.parse(readFileSync(CROSS_TRADE_FILE, 'utf-8'));
    if (!parsed || !Array.isArray(parsed.configs)) return { configs: [] };
    return parsed as CrossTradeFile;
  } catch {
    return { configs: [] };
  }
}

export function saveCrossTradeFile(file: CrossTradeFile): void {
  ensureDir();
  writeFileSync(CROSS_TRADE_FILE, JSON.stringify(file, null, 2));
}

export function stampUpdatedAt<T extends { updatedAt: number }>(row: T): T {
  row.updatedAt = Date.now();
  return row;
}

/**
 * List live configs (tombstones filtered).
 */
export function listCrossTradeConfigs(): StoredCrossTradeConfig[] {
  return loadCrossTradeFile().configs.filter(isLive);
}

/**
 * List ALL configs including tombstones. Listener snapshot path uses this.
 */
export function listAllCrossTradeConfigs(): StoredCrossTradeConfig[] {
  return loadCrossTradeFile().configs;
}

export function getCrossTradeConfig(id: string): StoredCrossTradeConfig | undefined {
  return loadCrossTradeFile().configs.find((c) => c.id === id && isLive(c));
}

export interface NewCrossTradeArgs {
  masterAccount: string;
  masterSymbol?: string;
  enabled?: boolean;
  copierAccount: string;
  copierExecution?: CrossTradeExecution;
  copierWeight?: number;
  copierSymbolMappings?: { [key: string]: string };
}

/**
 * Build a single-copier config row. Most CLI usage starts with one master
 * and one copier; callers can mutate `copiers[]` further if they want more.
 */
export function buildSingleCopierConfig(args: NewCrossTradeArgs): StoredCrossTradeConfig {
  const copier: CrossTradeCopier = {
    enabled: true,
    account: args.copierAccount,
    weight: args.copierWeight ?? 1,
    execution: args.copierExecution ?? 'LimitSlippage',
    weightMode: 'Absolute',
    symbolMappings: args.copierSymbolMappings ?? {},
  };
  return {
    id: randomUUID(),
    enabled: args.enabled ?? false,
    masterAccount: args.masterAccount,
    masterSymbol: args.masterSymbol,
    masterWeight: 1,
    copiers: [copier],
    updatedAt: Date.now(),
  };
}

/**
 * Local upsert — stamps updatedAt so the change participates in cross-device
 * LWW. For listener-side raw upserts (where the inbound timestamp already
 * won the LWW comparison), use upsertCrossTradeConfigRaw.
 */
export function upsertCrossTradeConfig(row: StoredCrossTradeConfig): void {
  const file = loadCrossTradeFile();
  stampUpdatedAt(row);
  const idx = file.configs.findIndex((c) => c.id === row.id);
  if (idx >= 0) {
    file.configs[idx] = { ...row, deletedAt: undefined };
  } else {
    file.configs.push(row);
  }
  saveCrossTradeFile(file);
}

/**
 * Listener-side raw upsert. Does NOT stamp updatedAt — trusts the wire
 * timestamp. Mirrors `upsertAccountRaw` in config.ts.
 */
export function upsertCrossTradeConfigRaw(row: StoredCrossTradeConfig): void {
  const file = loadCrossTradeFile();
  const idx = file.configs.findIndex((c) => c.id === row.id);
  if (idx >= 0) {
    file.configs[idx] = row;
  } else {
    file.configs.push(row);
  }
  saveCrossTradeFile(file);
}

/**
 * Soft-delete a config by id. Sets deletedAt + updatedAt. Returns true iff
 * a live row was tombstoned. Mirrors `removeAccount` semantics.
 */
export function removeCrossTradeConfig(id: string, opts?: { createTombstoneIfMissing?: boolean }): boolean {
  const file = loadCrossTradeFile();
  const now = Date.now();
  const idx = file.configs.findIndex((c) => c.id === id);
  if (idx >= 0) {
    file.configs[idx] = { ...file.configs[idx]!, deletedAt: now, updatedAt: now };
    saveCrossTradeFile(file);
    return true;
  }
  if (!opts?.createTombstoneIfMissing) return false;
  file.configs.push({
    id,
    enabled: false,
    masterAccount: undefined,
    masterSymbol: undefined,
    masterWeight: 1,
    copiers: [],
    updatedAt: now,
    deletedAt: now,
  });
  saveCrossTradeFile(file);
  return true;
}

/**
 * Listener-side tombstone. Caller supplies the inbound updatedAt — same shape
 * as `tombstoneAccount` in config.ts.
 */
export function tombstoneCrossTradeConfig(id: string, updatedAt: number): void {
  const file = loadCrossTradeFile();
  const now = Date.now();
  const idx = file.configs.findIndex((c) => c.id === id);
  if (idx >= 0) {
    file.configs[idx] = { ...file.configs[idx]!, deletedAt: now, updatedAt };
  } else {
    file.configs.push({
      id,
      enabled: false,
      masterAccount: undefined,
      masterSymbol: undefined,
      masterWeight: 1,
      copiers: [],
      updatedAt,
      deletedAt: now,
    });
  }
  saveCrossTradeFile(file);
}

/**
 * Set the top-level enabled flag on a live config. Returns true iff updated.
 */
export function setCrossTradeConfigEnabled(id: string, enabled: boolean): boolean {
  const file = loadCrossTradeFile();
  const idx = file.configs.findIndex((c) => c.id === id && isLive(c));
  if (idx < 0) return false;
  file.configs[idx] = stampUpdatedAt({ ...file.configs[idx]!, enabled });
  saveCrossTradeFile(file);
  return true;
}

/**
 * Purge tombstones older than maxAgeMs. Returns count removed.
 */
export function gcTombstonedCrossTradeConfigs(maxAgeMs: number = TOMBSTONE_TTL_MS): number {
  const file = loadCrossTradeFile();
  const cutoff = Date.now() - maxAgeMs;
  const before = file.configs.length;
  file.configs = file.configs.filter((c) => !c.deletedAt || c.deletedAt > cutoff);
  const removed = before - file.configs.length;
  if (removed > 0) saveCrossTradeFile(file);
  return removed;
}

/**
 * Filter to configs currently runnable. Used by the daemon: enabled + at
 * least one enabled copier + not tombstoned. Chase mode is rejected by the
 * daemon for v1, but it's not filtered here — the daemon prints a clear
 * warning so the user knows which configs were dropped and why.
 */
export function getRunnableCrossTradeConfigs(): StoredCrossTradeConfig[] {
  return listCrossTradeConfigs().filter(
    (c) => c.enabled && c.masterAccount && c.copiers.some((cp) => cp?.enabled && cp.account),
  );
}
