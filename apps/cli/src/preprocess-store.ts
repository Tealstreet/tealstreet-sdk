import { getStorageDir, onStorageOwnerChange, isRecord, assertStorageSession } from './storage-context';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { join } from 'path';
import { MemoryPreprocessStore } from '@tealstreet/cli-core';
import { enqueueAliasChange, type AliasChange, type AliasSyncState, type PreprocessStore } from '@tealstreet/cli-core';
import { logger } from './logger';

const CONFIG_DIR = () => getStorageDir();
const VARS_FILE = () => join(CONFIG_DIR(), 'vars.json');
const ALIASES_FILE = () => join(CONFIG_DIR(), 'aliases.json');
const WHITELIST_FILE = () => join(CONFIG_DIR(), 'whitelist.json');
const FATFINGER_FILE = () => join(CONFIG_DIR(), 'fatfingers.json');
const SIMULATION_FILE = () => join(CONFIG_DIR(), 'simulation.json');
const BUDGET_FILE = () => join(CONFIG_DIR(), 'budget.json');
// Alias changes not yet accepted by the server, and its cursor (alias-sync.ts).
const ALIAS_SYNC_FILE = () => join(CONFIG_DIR(), 'aliases-sync.json');

/**
 * File-backed `PreprocessStore` for the standalone CLI. Vars persist to
 * `~/.tealstreet/vars.json`, aliases to `~/.tealstreet/aliases.json`,
 * whitelisted symbols to `~/.tealstreet/whitelist.json`. The first
 * two files are `Record<string, string>` JSON; the whitelist is a
 * plain `string[]`. Writes happen synchronously after every mutation
 * so a crash mid-session doesn't lose state. Reads happen once at
 * construction.
 *
 * Subclassing `MemoryPreprocessStore` lets us reuse the in-memory
 * semantics and just persist after each mutation.
 */
export class FilePreprocessStore extends MemoryPreprocessStore implements PreprocessStore {
  constructor() {
    assertStorageSession();
    super();
    this.reloadOwner();
    onStorageOwnerChange(() => this.reloadOwner());
  }

  private reloadOwner(): void {
    super.unsetAllVars();
    super.unsetAllAliases();
    super.unsetAllLiveVars();
    super.unsetAllTrackedVars();
    super.clearWhitelist();
    super.clearFatfingers();
    super.setSimulation(false);
    super.setBudget(0);
    ensureDir();
    loadInto(this, VARS_FILE(), (k, v) => super.setVar(k, v));
    loadInto(this, ALIASES_FILE(), (k, v) => super.setAlias(k, v));
    loadWhitelist((symbol) => super.addToWhitelist(symbol));
    loadFatfingers((symbol, contracts) => super.setFatfinger(symbol, contracts));
    loadSimulation((on) => super.setSimulation(on));
    loadBudget((usd) => super.setBudget(usd));
  }

  override getVars() {
    assertStorageSession();
    return super.getVars();
  }
  override getVar(name: string) {
    assertStorageSession();
    return super.getVar(name);
  }
  override getAliases() {
    assertStorageSession();
    return super.getAliases();
  }
  override getAlias(name: string) {
    assertStorageSession();
    return super.getAlias(name);
  }
  override getLiveVars() {
    assertStorageSession();
    return super.getLiveVars();
  }
  override getLiveVar(name: string) {
    assertStorageSession();
    return super.getLiveVar(name);
  }
  override getTrackedVars() {
    assertStorageSession();
    return super.getTrackedVars();
  }
  override getTrackedVar(name: string) {
    assertStorageSession();
    return super.getTrackedVar(name);
  }
  override getWhitelist() {
    assertStorageSession();
    return super.getWhitelist();
  }
  override isWhitelisted(symbol: string) {
    assertStorageSession();
    return super.isWhitelisted(symbol);
  }
  override getFatfingers() {
    assertStorageSession();
    return super.getFatfingers();
  }
  override getFatfinger(symbol: string) {
    assertStorageSession();
    return super.getFatfinger(symbol);
  }
  override getSimulation() {
    assertStorageSession();
    return super.getSimulation();
  }
  override getBudget() {
    assertStorageSession();
    return super.getBudget();
  }
  override getBudgetSpent() {
    assertStorageSession();
    return super.getBudgetSpent();
  }

  override setLiveVar(name: string, source: string) {
    assertStorageSession();
    super.setLiveVar(name, source);
  }
  override unsetLiveVar(name: string) {
    assertStorageSession();
    return super.unsetLiveVar(name);
  }
  override unsetAllLiveVars() {
    assertStorageSession();
    super.unsetAllLiveVars();
  }
  override setTrackedVar(...args: Parameters<MemoryPreprocessStore['setTrackedVar']>) {
    assertStorageSession();
    super.setTrackedVar(...args);
  }
  override updateTrackedVar(name: string, value: number) {
    assertStorageSession();
    super.updateTrackedVar(name, value);
  }
  override unsetTrackedVar(name: string) {
    assertStorageSession();
    return super.unsetTrackedVar(name);
  }
  override unsetAllTrackedVars() {
    assertStorageSession();
    super.unsetAllTrackedVars();
  }
  override addBudgetSpent(usd: number) {
    assertStorageSession();
    super.addBudgetSpent(usd);
  }
  override resetBudgetSpent() {
    assertStorageSession();
    super.resetBudgetSpent();
  }

  override setVar(name: string, value: string): void {
    assertStorageSession();
    super.setVar(name, value);
    save(VARS_FILE(), super.getVars());
  }

  override unsetVar(name: string): boolean {
    assertStorageSession();
    const removed = super.unsetVar(name);
    if (removed) save(VARS_FILE(), super.getVars());
    return removed;
  }

  override unsetAllVars(): void {
    assertStorageSession();
    super.unsetAllVars();
    save(VARS_FILE(), super.getVars());
  }

  // Every user-made alias change is queued for sync (alias-sync.ts) so it
  // reaches the server and every other listener. Loading at construction goes
  // through super directly and is NOT queued; nor is applyRemoteAliases.
  private aliasChangeListener: (() => void) | null = null;

  setAliasChangeListener(listener: (() => void) | null): void {
    assertStorageSession();
    this.aliasChangeListener = listener;
  }

  private queueAliasChange(change: AliasChange): void {
    this.setAliasSyncState(enqueueAliasChange(this.getAliasSyncState(), change));
    this.aliasChangeListener?.();
  }

  override setAlias(name: string, command: string): void {
    assertStorageSession();
    super.setAlias(name, command);
    save(ALIASES_FILE(), super.getAliases());
    this.queueAliasChange({ name, command });
  }

  override unsetAlias(name: string): boolean {
    assertStorageSession();
    const removed = super.unsetAlias(name);
    if (removed) {
      save(ALIASES_FILE(), super.getAliases());
      this.queueAliasChange({ name, command: null });
    }
    return removed;
  }

  override unsetAllAliases(): void {
    assertStorageSession();
    const names = Object.keys(super.getAliases());
    super.unsetAllAliases();
    save(ALIASES_FILE(), super.getAliases());
    for (const name of names) this.queueAliasChange({ name, command: null });
  }

  getAliasSyncState(): AliasSyncState {
    assertStorageSession();
    try {
      if (!existsSync(ALIAS_SYNC_FILE())) return { cursor: null, pending: [] };
      const parsed: unknown = JSON.parse(readFileSync(ALIAS_SYNC_FILE(), 'utf-8'));
      if (!isRecord(parsed)) return { cursor: null, pending: [] };
      return {
        cursor: typeof parsed.cursor === 'number' ? parsed.cursor : null,
        pending: Array.isArray(parsed.pending)
          ? parsed.pending.filter(
              (entry): entry is AliasChange =>
                isRecord(entry) &&
                typeof entry.name === 'string' &&
                (entry.command === null || typeof entry.command === 'string'),
            )
          : [],
        owner: typeof parsed.owner === 'string' ? parsed.owner : null,
        ...(parsed.uploadLocal === false ? { uploadLocal: false } : {}),
      };
    } catch (err) {
      logger.warn(
        `alias sync: unreadable ${ALIAS_SYNC_FILE()}, starting over: ${err instanceof Error ? err.message : String(err)}`,
      );
      return { cursor: null, pending: [] };
    }
  }

  setAliasSyncState(state: AliasSyncState): void {
    assertStorageSession();
    try {
      writeFileSync(ALIAS_SYNC_FILE(), JSON.stringify(state, null, 2) + '\n', 'utf-8');
    } catch (err) {
      logger.warn(
        `alias sync: could not save ${ALIAS_SYNC_FILE()}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  /** Server rows applied locally without being queued back as new changes. */
  applyRemoteAliases(entries: AliasChange[]): void {
    assertStorageSession();
    if (entries.length === 0) return;
    for (const entry of entries) {
      if (entry.command === null) super.unsetAlias(entry.name);
      else super.setAlias(entry.name, entry.command);
    }
    save(ALIASES_FILE(), super.getAliases());
  }

  override addToWhitelist(symbol: string): void {
    assertStorageSession();
    super.addToWhitelist(symbol);
    saveWhitelist(super.getWhitelist());
  }

  override removeFromWhitelist(symbol: string): boolean {
    assertStorageSession();
    const removed = super.removeFromWhitelist(symbol);
    if (removed) saveWhitelist(super.getWhitelist());
    return removed;
  }

  override clearWhitelist(): void {
    assertStorageSession();
    super.clearWhitelist();
    saveWhitelist([]);
  }

  override setFatfinger(symbol: string, contracts: number): void {
    assertStorageSession();
    super.setFatfinger(symbol, contracts);
    saveFatfingers(super.getFatfingers());
  }

  override unsetFatfinger(symbol: string): boolean {
    assertStorageSession();
    const removed = super.unsetFatfinger(symbol);
    if (removed) saveFatfingers(super.getFatfingers());
    return removed;
  }

  override clearFatfingers(): void {
    assertStorageSession();
    super.clearFatfingers();
    saveFatfingers({});
  }

  override setSimulation(on: boolean): void {
    assertStorageSession();
    super.setSimulation(on);
    saveSimulation(on);
  }

  // Persist only the budget CAP. The spent accumulator is session-only
  // (lives in MemoryPreprocessStore) and intentionally resets on restart.
  override setBudget(usd: number): void {
    assertStorageSession();
    super.setBudget(usd);
    saveBudget(super.getBudget() ?? 0);
  }
}

function ensureDir(): void {
  if (!existsSync(CONFIG_DIR())) {
    mkdirSync(CONFIG_DIR(), { recursive: true });
  }
}

function loadInto(_self: MemoryPreprocessStore, file: string, apply: (k: string, v: string) => void): void {
  if (!existsSync(file)) return;
  try {
    const raw = readFileSync(file, 'utf-8');
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed)) return;
    for (const [k, v] of Object.entries(parsed)) {
      if (typeof v === 'string') apply(k, v);
    }
  } catch (e) {
    logger.warn(`Failed to load ${file}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function loadWhitelist(apply: (symbol: string) => void): void {
  if (!existsSync(WHITELIST_FILE())) return;
  try {
    const raw = readFileSync(WHITELIST_FILE(), 'utf-8');
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return;
    for (const sym of parsed) {
      if (typeof sym === 'string') apply(sym);
    }
  } catch (e) {
    logger.warn(`Failed to load ${WHITELIST_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function loadFatfingers(apply: (symbol: string, contracts: number) => void): void {
  if (!existsSync(FATFINGER_FILE())) return;
  try {
    const raw = readFileSync(FATFINGER_FILE(), 'utf-8');
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed)) return;
    for (const [k, v] of Object.entries(parsed)) {
      const n = typeof v === 'number' ? v : parseFloat(String(v));
      if (Number.isFinite(n) && n > 0) apply(k, n);
    }
  } catch (e) {
    logger.warn(`Failed to load ${FATFINGER_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function save(file: string, data: Record<string, string>): void {
  try {
    writeFileSync(file, JSON.stringify(data, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${file}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function saveWhitelist(symbols: string[]): void {
  try {
    writeFileSync(WHITELIST_FILE(), JSON.stringify(symbols, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${WHITELIST_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function saveFatfingers(map: Record<string, number>): void {
  try {
    writeFileSync(FATFINGER_FILE(), JSON.stringify(map, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${FATFINGER_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function loadSimulation(apply: (on: boolean) => void): void {
  if (!existsSync(SIMULATION_FILE())) return;
  try {
    const raw = readFileSync(SIMULATION_FILE(), 'utf-8');
    const parsed = JSON.parse(raw);
    if (typeof parsed === 'object' && parsed !== null && typeof parsed.on === 'boolean') {
      apply(parsed.on);
    } else if (typeof parsed === 'boolean') {
      apply(parsed);
    }
  } catch (e) {
    logger.warn(`Failed to load ${SIMULATION_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function saveSimulation(on: boolean): void {
  try {
    writeFileSync(SIMULATION_FILE(), JSON.stringify({ on }, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${SIMULATION_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function loadBudget(apply: (usd: number) => void): void {
  if (!existsSync(BUDGET_FILE())) return;
  try {
    const raw = readFileSync(BUDGET_FILE(), 'utf-8');
    const parsed = JSON.parse(raw);
    const usd = typeof parsed === 'object' && parsed !== null ? parsed.usd : parsed;
    const n = typeof usd === 'number' ? usd : parseFloat(String(usd));
    if (Number.isFinite(n) && n > 0) apply(n);
  } catch (e) {
    logger.warn(`Failed to load ${BUDGET_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

function saveBudget(usd: number): void {
  try {
    writeFileSync(BUDGET_FILE(), JSON.stringify({ usd }, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${BUDGET_FILE()}: ${e instanceof Error ? e.message : String(e)}`);
  }
}
