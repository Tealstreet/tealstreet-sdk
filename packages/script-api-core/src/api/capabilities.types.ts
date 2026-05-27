/**
 * Capabilities — declared per-script at save time, enforced at the dispatcher.
 * See docs/custom-scripts/SPEC.md §4.
 *
 * Types only in Phase 1. Enforcement (capabilityCheck.ts) lands in Phase 2.
 */
import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

export type TargetName = 'core' | 'cli' | 'web' | 'mobile';

/**
 * The set of exchange.* method names a script may invoke.
 * Kept as string literals (not a TS enum) so the runtime can validate
 * against a frozen allowlist without pulling in the action surface.
 */
export type WriteMethodName =
  | 'placeOrder'
  | 'placeOrders'
  | 'cancelOrder'
  | 'cancelOrders'
  | 'updateOrder'
  | 'closePosition'
  | 'setLeverage'
  | 'setPositionMode'
  | 'setMarginType'
  | 'runCommand';

/**
 * The set of getX method names a script may invoke.
 * When `Capabilities.reads` is undefined, all reads are permitted
 * (matches the v1 author-only model per SPEC §4.1 + §12 design call #10).
 */
export type ReadMethodName =
  | 'getActiveAccount'
  | 'getActiveAccounts'
  | 'getActiveSymbol'
  | 'getActiveExchange'
  | 'getAccounts'
  | 'getMarkets'
  | 'getMarket'
  | 'getTicker'
  | 'getTickers'
  | 'getPositions'
  | 'getHistoricalPosition'
  | 'getOrders'
  | 'getBalance'
  | 'getCombinedBalance'
  | 'getOrderbook'
  | 'getCandles';

export interface RunCommandCapability {
  readonly enabled: boolean;
  /** Allowlist of REPL verbs the script may pass to runCommand. */
  readonly allowedVerbs: readonly string[];
}

export interface StorageCapability {
  readonly enabled: boolean;
  /** Per-script storage cap in bytes. Default 102_400 (100 KB). */
  readonly quotaBytes: number;
}

export interface Capabilities {
  /**
   * Account allowlist for both reads and writes (required).
   * An empty array `[]` means "no restriction" — the resolved account
   * (explicit `opts.account` → single-entry → active, per SPEC §2.6) is
   * used without the membership check (author-only default, mirrors
   * `reads: undefined`). List accounts to pin the script to them; a
   * non-empty list is enforced strictly. There is no `'*'` wildcard.
   */
  readonly accounts: readonly string[];
  /** Optional broader gate. When omitted, derived from `accounts`. */
  readonly exchanges?: readonly ExchangeName[];
  /**
   * Read-side allowlist. When undefined, all reads are permitted
   * (v1 author-only default). When set, dispatcher rejects reads not in the list.
   */
  readonly reads?: readonly ReadMethodName[];
  /** Write-side allowlist. Always explicit; nothing implicit. */
  readonly writes: readonly WriteMethodName[];
  /** Escape-hatch `runCommand` config. When omitted, runCommand is disabled. */
  readonly runCommand?: RunCommandCapability;
  /** When true, the script is safe to re-run under `retry N`. Default false. */
  readonly idempotent: boolean;
  /** Per-script KV storage capability + quota. */
  readonly storage: StorageCapability;
  /**
   * Worker-termination budget. Default 30_000 ms.
   * See SPEC §12 design call #4.
   */
  readonly maxRuntimeMs?: number;
}

/**
 * Authoring-side shape — what a script's source file declares via
 * `export const meta: ScriptMeta = { ... }`. This is the public-facing
 * type users `import type` from `@tealstreet/script-api-core` when
 * editing their `~/.tealstreet/scripts/*.ts` files (SPEC §5.1).
 *
 * Distinct from `ScriptRecord` (in runScript.ts): `ScriptMeta` is what
 * the script file declares, `ScriptRecord` is what the loader builds
 * by combining `ScriptMeta` with filesystem metadata (name from
 * filename, optional `fn` reference from the default export).
 */
export interface ScriptMeta {
  readonly targets: readonly TargetName[];
  readonly capabilities: Capabilities;
  /**
   * Stamped per-target at save time so the runtime can refuse to load
   * scripts written against a future surface this host hasn't shipped
   * yet (SPEC §2.5 `targetCheck`). Optional in v1 — when omitted, the
   * validator treats it as `0.0.0` and any non-zero runtime version
   * passes.
   */
  readonly coreApiVersion?: string;
  readonly cliApiVersion?: string;
  readonly webApiVersion?: string;
  readonly mobileApiVersion?: string;
}
