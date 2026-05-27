import type { ExchangeLike } from '../types';
import type { TrackedVarEntry, TrackedVarKind } from './trackedVars';

/**
 * Persistent storage for user-defined variables (`set @x v`) and aliases
 * (`alias <name> "<cmd>"`). The standalone CLI provides a file-backed
 * implementation; the web CLI can use an in-memory or atom-backed one.
 */
export interface PreprocessStore {
  getVars(): Record<string, string>;
  getVar(name: string): string | undefined;
  /**
   * Set a static (snapshot) variable. Implementations MUST clear any
   * same-name reactive (`sets @name …`) OR tracked (`track …`)
   * binding — `set`, `sets`, and `track` are mutually exclusive on
   * the same name. This keeps `@name` resolution unambiguous: at
   * most one path produces a value.
   */
  setVar(name: string, value: string): void;
  unsetVar(name: string): boolean;
  unsetAllVars(): void;

  /**
   * Reactive ("live") variables — `sets @name <source>`. The stored
   * value is the SOURCE expression, not a snapshot; consumers
   * evaluate against current context on each read. Always session-
   * scoped (no persistence) — persisting a live source would require
   * either a subscription engine to rehydrate or saving a stale
   * snapshot, both worse than just re-typing the `sets` next session.
   *
   * Mutex with `set` (static) AND `track` (tracked): setLiveVar
   * MUST clear any same-name static OR tracked binding. The
   * `@name` resolution path is tracked → live → static, with the
   * mutex guaranteeing at most one defined.
   */
  getLiveVars(): Record<string, string>;
  getLiveVar(name: string): string | undefined;
  setLiveVar(name: string, source: string): void;
  unsetLiveVar(name: string): boolean;
  unsetAllLiveVars(): void;

  /**
   * Tracked variables — `track max @name <source>` /
   * `track min @name <source>`. The stored entry is a (kind, source,
   * value) tuple; the value is the high-water / low-water mark of
   * the source over the lifetime of the tracker. Each `@name`
   * substitution re-evaluates the source and updates the extreme
   * if it's more extreme.
   *
   * Session-scoped. Mutex with `set` (static) AND `sets` (live):
   * setTrackedVar MUST clear any same-name static or live binding.
   *
   * `updateTrackedVar` is the in-place value-only mutation that the
   * resolver uses on every read to refresh the extreme. It does NOT
   * touch the kind / source — those are set once by setTrackedVar.
   */
  getTrackedVars(): Record<string, TrackedVarEntry>;
  getTrackedVar(name: string): TrackedVarEntry | undefined;
  setTrackedVar(name: string, kind: TrackedVarKind, source: string): void;
  updateTrackedVar(name: string, value: number): void;
  unsetTrackedVar(name: string): boolean;
  unsetAllTrackedVars(): void;

  getAliases(): Record<string, string>;
  getAlias(name: string): string | undefined;
  setAlias(name: string, command: string): void;
  unsetAlias(name: string): boolean;
  unsetAllAliases(): void;

  /**
   * Whitelist of symbols that the global / "all positions" commands
   * (`close allpos`, `close longs`, `close shorts`, `nuke`) refuse to
   * touch. Persistent like vars and aliases.
   *
   * Storage layer holds normalized symbol strings — callers should
   * pass the same form they'd type in a command (uppercased).
   */
  getWhitelist(): string[];
  isWhitelisted(symbol: string): boolean;
  addToWhitelist(symbol: string): void;
  removeFromWhitelist(symbol: string): boolean;
  clearWhitelist(): void;

  /**
   * Per-symbol fatfinger — the maximum allowed order size (in contracts)
   * for a given symbol. Sized commands (buy / sell / scale / swarm /
   * twap / chase) reject orders whose computed amount exceeds the
   * limit. Limits are checked per-individual-order — `buy 1, repeat 11`
   * is allowed even if the total exceeds the limit, matching ichibot.
   *
   * Symbol keys are normalized to uppercase, matching the whitelist
   * convention. A value of `undefined` from `getFatfinger` means "no
   * limit set"; setting `0` is treated as "no limit" (cleaner than
   * special-casing in callers).
   */
  getFatfingers(): Record<string, number>;
  getFatfinger(symbol: string): number | undefined;
  setFatfinger(symbol: string, contracts: number): void;
  unsetFatfinger(symbol: string): boolean;
  clearFatfingers(): void;

  /**
   * Global dry-run flag. When true, every write-side trading command
   * (buy / sell / close / scale / swarm / twap / chase / cancel /
   * nuke / move / bump / leverage / stop) logs what it WOULD do and
   * returns success without touching the exchange. Read commands
   * (positions / orders / margin / etc.) work normally so the user
   * can still inspect state during a sim.
   *
   * Persisted alongside vars / aliases / whitelist / fatfingers — a
   * stale `simulation on` from a previous session is one of the
   * footguns this whole feature exists to catch, so the load happens
   * eagerly (and the prompt should surface it).
   */
  getSimulation(): boolean;
  setSimulation(on: boolean): void;

  /**
   * Aggregate notional budget (TSBUG-106). An opt-in, off-by-default
   * cumulative cap on the total USD notional of exposure-increasing
   * orders placed this SESSION. `getBudget` returns the cap in USD
   * (`undefined` / 0 = off). The cap is PERSISTED (like fatfinger); the
   * `spent` accumulator is SESSION-only (in-memory) and resets on
   * restart, on `setBudget`, and on `resetBudgetSpent`. Reduce-only
   * orders don't count (they reduce exposure). Enforced per-order in
   * `TradingBaseCommand.enforceBudget`, which adds the order's notional
   * to `spent` and blocks once `spent + thisOrder > cap`.
   */
  getBudget(): number | undefined;
  setBudget(usd: number): void;
  getBudgetSpent(): number;
  addBudgetSpent(usd: number): void;
  resetBudgetSpent(): void;
}

/**
 * Resolvers for UI-only constants — `$click`, `$ordersize`, `$quick<n>`.
 *
 * These tokens only have meaning where there's UI context to read from
 * (chart click event bus, linked order-form atom, quicksize button
 * config). The standalone CLI doesn't provide them, so the substitute
 * layer falls back to throwing "only available in the web terminal".
 *
 * Each resolver returns the *substituted token string* — for prices,
 * a numeric string (e.g. `"50000"`); for sizes, a complete size token
 * (e.g. `"$100"`) so the result still satisfies Phase 0's "no bare
 * numbers" rule.
 *
 * Returning `undefined` means "no value available right now" — the
 * substitute layer treats it the same as a missing resolver and throws
 * a token-specific error.
 */
export interface UITokenResolvers {
  /** Last chart-click price. Returns a numeric string. */
  click?: () => string | undefined;
  /** Size from the linked order form. Returns a complete size token
   *  (must include `$` prefix/suffix or `%` marker). */
  orderSize?: () => string | undefined;
  /** `$quick<n>` button value, n in 1..7. Returns a complete size token. */
  quick?: (n: number) => string | undefined;
}

/**
 * Context passed into the preprocessor. Constants like `$entry` resolve
 * against `exchange.store.positions` filtered by `symbol`; user vars and
 * aliases come from `store`. UI-only constants resolve against
 * `uiResolvers` if provided.
 */
export interface PreprocessContext {
  exchange: ExchangeLike | null;
  symbol: string;
  accountName?: string;
  store: PreprocessStore;
  uiResolvers?: UITokenResolvers;
}

/**
 * One step in the preprocessed command stream. The runner is expected to
 * fire-and-don't-await each entry except where `block` is true (currently
 * only `sleep`), in which case it must await before moving on.
 */
export interface PreprocessedStep {
  /** The command to execute, post-substitution. */
  command: string;
  /** True if the runner must await this step before firing the next. */
  block: boolean;
  /**
   * If set to a non-negative integer, the runner must AWAIT this step
   * and retry up to N times on error. `retry: 0` means "await but
   * don't retry" — useful for synchronising error reporting with
   * subsequent chain steps without actually re-firing. `undefined`
   * means fire-and-don't-await.
   *
   * Cap is 10 retries — beyond that the parser rejects the prefix
   * rather than emit a step that could loop forever on a persistent
   * exchange error.
   */
  retry?: number;
  /**
   * `'continue'` if `set ec` was active at the time this step was
   * preprocessed. The runner converts errors on this step to warnings
   * (or no-ops) instead of error prints. `'fail'` (or undefined) is
   * the default — runner prints the error and moves on.
   *
   * Note: this only changes the *reporting* of errors. Our chain
   * model is fire-and-don't-await by default, so a failing step
   * already doesn't break subsequent steps. The flag matters for
   * retry-flagged steps and for sleep (which currently blocks).
   */
  errorMode?: 'fail' | 'continue';
  /**
   * Capture the command's `result.data.orderIds` into this user var
   * after the step completes. Set by the preprocess layer when the
   * user prefixes a command with `@<name> = ` (e.g. `@o1 = buy 1$`).
   *
   * The runner MUST `await` the step (not fire-and-forget) when this
   * is set, then write `result.data.orderIds.join(',')` into the
   * named static var via `store.setVar`. The comma-list shape
   * composes with the existing `cancel <id1>,<id2>,…` and symbol-
   * group fan-out paths, so `cancel @o1` works without further
   * special-casing.
   *
   * Empty arrays (e.g. Bybit's position-stop endpoint returns no IDs)
   * become an empty string. The runner warns when this happens so the
   * user knows the capture didn't bind anything actionable.
   */
  captureTo?: string;
  /**
   * Names of `@<var>` tokens that an EARLIER step in the same chain
   * will populate via its own `captureTo`. The preprocess layer
   * leaves these tokens literal in `command` (rather than throwing
   * "Unknown variable") and the runner MUST resolve them at
   * dispatch time, after the prior step's writeback has fired.
   *
   * Each name in the list corresponds to one or more `@<name>` (and
   * `@<name>,` etc.) occurrences in `command`. Resolution is a
   * straight string replacement: read the static var, substitute
   * every `@<name>` token in the command with the read value, then
   * dispatch the resolved command.
   *
   * Math operands referencing capture targets are rejected at
   * preprocess (capture vars hold comma-lists, which aren't
   * numeric), so the runner only has to handle bare-@ substitution.
   */
  deferredSubstitution?: string[];
  /**
   * True when this step was separated from the prior step by `;`
   * (sequential), as opposed to `&` (parallel). The runner MUST drain
   * all in-flight fire-and-forget steps from earlier in the chain
   * before dispatching this step — a barrier. This is what makes
   * `close all; buy $1000` wait for the close(s) to finish before
   * buying, while `close all & buy $1000` fires both without waiting.
   *
   * Only set (to `true`) on the FIRST step a `;`-preceded segment
   * emits; omitted (falsy) for `&`-preceded steps, the first step of
   * the chain, and interior steps of an expansion (loop/if fan-outs
   * stay parallel among themselves unless their source used `;`).
   */
  awaitPrior?: boolean;
}

export interface PreprocessWarning {
  /** Stable warning code for callers to deduplicate. */
  code: 'deprecated-at-prefix';
  message: string;
}

export interface PreprocessResult {
  steps: PreprocessedStep[];
  warnings: PreprocessWarning[];
  /**
   * True when an `end` marker fired during preprocessing. Subsequent
   * segments / sub-segments after that point were dropped. Propagates
   * upward through nested `preprocessCommand` calls (notably from
   * `if`-branch recursion) so an `end` inside a branch terminates the
   * whole chain, not just the branch body.
   *
   * Most callers can ignore this — the `steps` array already reflects
   * the early cut. The flag exists so the recursive `if` consumer can
   * propagate the termination upward, and so tests can assert the
   * marker actually fired.
   */
  endEarly?: boolean;
}
