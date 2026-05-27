import type { TrackedVarEntry, TrackedVarKind } from './trackedVars';
import type { PreprocessStore } from './types';

/**
 * In-memory implementation of `PreprocessStore`. Used by tests and as the
 * default for the web CLI. The standalone CLI replaces this with a
 * file-backed store under `~/.tealstreet/`.
 */
export class MemoryPreprocessStore implements PreprocessStore {
  private vars = new Map<string, string>();
  private liveVars = new Map<string, string>();
  private aliases = new Map<string, string>();
  private whitelist = new Set<string>();

  getVars(): Record<string, string> {
    return Object.fromEntries(this.vars);
  }

  getVar(name: string): string | undefined {
    return this.vars.get(name);
  }

  setVar(name: string, value: string): void {
    // `set` wins over `sets` AND `track` — clear any same-name
    // reactive or tracked binding so the `@name` resolution path
    // can't return more than one answer. Go through the public
    // unset methods so subclass overrides (e.g. FilePreprocessStore's
    // persistence) fire as expected.
    if (this.liveVars.has(name)) this.unsetLiveVar(name);
    if (this.trackedVars.has(name)) this.unsetTrackedVar(name);
    this.vars.set(name, value);
  }

  unsetVar(name: string): boolean {
    return this.vars.delete(name);
  }

  unsetAllVars(): void {
    this.vars.clear();
  }

  getLiveVars(): Record<string, string> {
    return Object.fromEntries(this.liveVars);
  }

  getLiveVar(name: string): string | undefined {
    return this.liveVars.get(name);
  }

  setLiveVar(name: string, source: string): void {
    // `sets` wins over `set` AND `track` — clear any same-name
    // static or tracked binding. Go through the public unset
    // methods so persistence-aware subclasses' overrides fire.
    if (this.vars.has(name)) this.unsetVar(name);
    if (this.trackedVars.has(name)) this.unsetTrackedVar(name);
    this.liveVars.set(name, source);
  }

  unsetLiveVar(name: string): boolean {
    return this.liveVars.delete(name);
  }

  unsetAllLiveVars(): void {
    this.liveVars.clear();
  }

  private trackedVars = new Map<string, TrackedVarEntry>();

  getTrackedVars(): Record<string, TrackedVarEntry> {
    // Defensive copy of each entry so callers can't mutate the
    // stored state through the snapshot. `value` is a primitive so
    // a shallow spread is enough.
    const out: Record<string, TrackedVarEntry> = {};
    for (const [k, v] of this.trackedVars) out[k] = { ...v };
    return out;
  }

  getTrackedVar(name: string): TrackedVarEntry | undefined {
    return this.trackedVars.get(name);
  }

  setTrackedVar(name: string, kind: TrackedVarKind, source: string): void {
    // `track` wins over `set` AND `sets` — clear any same-name
    // static or live binding. Then create a fresh tracker with no
    // initial value (resolveTrackedVar seeds it on first read).
    if (this.vars.has(name)) this.unsetVar(name);
    if (this.liveVars.has(name)) this.unsetLiveVar(name);
    this.trackedVars.set(name, { kind, source, value: undefined });
  }

  updateTrackedVar(name: string, value: number): void {
    const entry = this.trackedVars.get(name);
    if (!entry) return;
    // The resolver enforces the extreme-update semantic; this method
    // is intentionally a dumb setter so the resolver is the single
    // source of truth on "what counts as more extreme".
    entry.value = value;
  }

  unsetTrackedVar(name: string): boolean {
    return this.trackedVars.delete(name);
  }

  unsetAllTrackedVars(): void {
    this.trackedVars.clear();
  }

  getAliases(): Record<string, string> {
    return Object.fromEntries(this.aliases);
  }

  getAlias(name: string): string | undefined {
    return this.aliases.get(name);
  }

  setAlias(name: string, command: string): void {
    this.aliases.set(name, command);
  }

  unsetAlias(name: string): boolean {
    return this.aliases.delete(name);
  }

  unsetAllAliases(): void {
    this.aliases.clear();
  }

  getWhitelist(): string[] {
    return [...this.whitelist].sort();
  }

  isWhitelisted(symbol: string): boolean {
    return this.whitelist.has(symbol.toUpperCase());
  }

  addToWhitelist(symbol: string): void {
    this.whitelist.add(symbol.toUpperCase());
  }

  removeFromWhitelist(symbol: string): boolean {
    return this.whitelist.delete(symbol.toUpperCase());
  }

  clearWhitelist(): void {
    this.whitelist.clear();
  }

  private fatfingers = new Map<string, number>();

  getFatfingers(): Record<string, number> {
    return Object.fromEntries(this.fatfingers);
  }

  getFatfinger(symbol: string): number | undefined {
    return this.fatfingers.get(symbol.toUpperCase());
  }

  setFatfinger(symbol: string, contracts: number): void {
    if (!Number.isFinite(contracts) || contracts <= 0) {
      this.fatfingers.delete(symbol.toUpperCase());
      return;
    }
    this.fatfingers.set(symbol.toUpperCase(), contracts);
  }

  unsetFatfinger(symbol: string): boolean {
    return this.fatfingers.delete(symbol.toUpperCase());
  }

  clearFatfingers(): void {
    this.fatfingers.clear();
  }

  private simulation = false;

  getSimulation(): boolean {
    return this.simulation;
  }

  setSimulation(on: boolean): void {
    this.simulation = !!on;
  }

  private budgetLimit = 0; // 0 = off
  private budgetSpent = 0; // session-only; not persisted

  getBudget(): number | undefined {
    return this.budgetLimit > 0 ? this.budgetLimit : undefined;
  }

  setBudget(usd: number): void {
    this.budgetLimit = Number.isFinite(usd) && usd > 0 ? usd : 0;
    // Setting (or clearing) a budget resets the session accumulator so
    // the new cap starts from a clean slate.
    this.budgetSpent = 0;
  }

  getBudgetSpent(): number {
    return this.budgetSpent;
  }

  addBudgetSpent(usd: number): void {
    if (Number.isFinite(usd) && usd > 0) this.budgetSpent += usd;
  }

  resetBudgetSpent(): void {
    this.budgetSpent = 0;
  }
}
