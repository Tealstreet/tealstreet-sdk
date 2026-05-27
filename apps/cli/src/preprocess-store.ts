import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { MemoryPreprocessStore } from '@tealstreet/cli-core';
import type { PreprocessStore } from '@tealstreet/cli-core';
import { logger } from './logger';

const CONFIG_DIR = process.env.TEALSTREET_CONFIG_DIR || join(homedir(), '.tealstreet');
const VARS_FILE = join(CONFIG_DIR, 'vars.json');
const ALIASES_FILE = join(CONFIG_DIR, 'aliases.json');
const WHITELIST_FILE = join(CONFIG_DIR, 'whitelist.json');
const FATFINGER_FILE = join(CONFIG_DIR, 'fatfingers.json');
const SIMULATION_FILE = join(CONFIG_DIR, 'simulation.json');
const BUDGET_FILE = join(CONFIG_DIR, 'budget.json');

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
    super();
    ensureDir();
    loadInto(this, VARS_FILE, (k, v) => super.setVar(k, v));
    loadInto(this, ALIASES_FILE, (k, v) => super.setAlias(k, v));
    loadWhitelist((symbol) => super.addToWhitelist(symbol));
    loadFatfingers((symbol, contracts) => super.setFatfinger(symbol, contracts));
    loadSimulation((on) => super.setSimulation(on));
    loadBudget((usd) => super.setBudget(usd));
  }

  override setVar(name: string, value: string): void {
    super.setVar(name, value);
    save(VARS_FILE, super.getVars());
  }

  override unsetVar(name: string): boolean {
    const removed = super.unsetVar(name);
    if (removed) save(VARS_FILE, super.getVars());
    return removed;
  }

  override unsetAllVars(): void {
    super.unsetAllVars();
    save(VARS_FILE, super.getVars());
  }

  override setAlias(name: string, command: string): void {
    super.setAlias(name, command);
    save(ALIASES_FILE, super.getAliases());
  }

  override unsetAlias(name: string): boolean {
    const removed = super.unsetAlias(name);
    if (removed) save(ALIASES_FILE, super.getAliases());
    return removed;
  }

  override unsetAllAliases(): void {
    super.unsetAllAliases();
    save(ALIASES_FILE, super.getAliases());
  }

  override addToWhitelist(symbol: string): void {
    super.addToWhitelist(symbol);
    saveWhitelist(super.getWhitelist());
  }

  override removeFromWhitelist(symbol: string): boolean {
    const removed = super.removeFromWhitelist(symbol);
    if (removed) saveWhitelist(super.getWhitelist());
    return removed;
  }

  override clearWhitelist(): void {
    super.clearWhitelist();
    saveWhitelist([]);
  }

  override setFatfinger(symbol: string, contracts: number): void {
    super.setFatfinger(symbol, contracts);
    saveFatfingers(super.getFatfingers());
  }

  override unsetFatfinger(symbol: string): boolean {
    const removed = super.unsetFatfinger(symbol);
    if (removed) saveFatfingers(super.getFatfingers());
    return removed;
  }

  override clearFatfingers(): void {
    super.clearFatfingers();
    saveFatfingers({});
  }

  override setSimulation(on: boolean): void {
    super.setSimulation(on);
    saveSimulation(on);
  }

  // Persist only the budget CAP. The spent accumulator is session-only
  // (lives in MemoryPreprocessStore) and intentionally resets on restart.
  override setBudget(usd: number): void {
    super.setBudget(usd);
    saveBudget(super.getBudget() ?? 0);
  }
}

function ensureDir(): void {
  if (!existsSync(CONFIG_DIR)) {
    mkdirSync(CONFIG_DIR, { recursive: true });
  }
}

function loadInto(_self: MemoryPreprocessStore, file: string, apply: (k: string, v: string) => void): void {
  if (!existsSync(file)) return;
  try {
    const raw = readFileSync(file, 'utf-8');
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    for (const [k, v] of Object.entries(parsed)) {
      if (typeof v === 'string') apply(k, v);
    }
  } catch (e) {
    logger.warn(`Failed to load ${file}: ${(e as Error).message}`);
  }
}

function loadWhitelist(apply: (symbol: string) => void): void {
  if (!existsSync(WHITELIST_FILE)) return;
  try {
    const raw = readFileSync(WHITELIST_FILE, 'utf-8');
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return;
    for (const sym of parsed) {
      if (typeof sym === 'string') apply(sym);
    }
  } catch (e) {
    logger.warn(`Failed to load ${WHITELIST_FILE}: ${(e as Error).message}`);
  }
}

function loadFatfingers(apply: (symbol: string, contracts: number) => void): void {
  if (!existsSync(FATFINGER_FILE)) return;
  try {
    const raw = readFileSync(FATFINGER_FILE, 'utf-8');
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    for (const [k, v] of Object.entries(parsed)) {
      const n = typeof v === 'number' ? v : parseFloat(String(v));
      if (Number.isFinite(n) && n > 0) apply(k, n);
    }
  } catch (e) {
    logger.warn(`Failed to load ${FATFINGER_FILE}: ${(e as Error).message}`);
  }
}

function save(file: string, data: Record<string, string>): void {
  try {
    writeFileSync(file, JSON.stringify(data, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${file}: ${(e as Error).message}`);
  }
}

function saveWhitelist(symbols: string[]): void {
  try {
    writeFileSync(WHITELIST_FILE, JSON.stringify(symbols, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${WHITELIST_FILE}: ${(e as Error).message}`);
  }
}

function saveFatfingers(map: Record<string, number>): void {
  try {
    writeFileSync(FATFINGER_FILE, JSON.stringify(map, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${FATFINGER_FILE}: ${(e as Error).message}`);
  }
}

function loadSimulation(apply: (on: boolean) => void): void {
  if (!existsSync(SIMULATION_FILE)) return;
  try {
    const raw = readFileSync(SIMULATION_FILE, 'utf-8');
    const parsed = JSON.parse(raw);
    if (typeof parsed === 'object' && parsed !== null && typeof parsed.on === 'boolean') {
      apply(parsed.on);
    } else if (typeof parsed === 'boolean') {
      apply(parsed);
    }
  } catch (e) {
    logger.warn(`Failed to load ${SIMULATION_FILE}: ${(e as Error).message}`);
  }
}

function saveSimulation(on: boolean): void {
  try {
    writeFileSync(SIMULATION_FILE, JSON.stringify({ on }, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${SIMULATION_FILE}: ${(e as Error).message}`);
  }
}

function loadBudget(apply: (usd: number) => void): void {
  if (!existsSync(BUDGET_FILE)) return;
  try {
    const raw = readFileSync(BUDGET_FILE, 'utf-8');
    const parsed = JSON.parse(raw);
    const usd = typeof parsed === 'object' && parsed !== null ? parsed.usd : parsed;
    const n = typeof usd === 'number' ? usd : parseFloat(String(usd));
    if (Number.isFinite(n) && n > 0) apply(n);
  } catch (e) {
    logger.warn(`Failed to load ${BUDGET_FILE}: ${(e as Error).message}`);
  }
}

function saveBudget(usd: number): void {
  try {
    writeFileSync(BUDGET_FILE, JSON.stringify({ usd }, null, 2) + '\n', 'utf-8');
  } catch (e) {
    logger.warn(`Failed to save ${BUDGET_FILE}: ${(e as Error).message}`);
  }
}
