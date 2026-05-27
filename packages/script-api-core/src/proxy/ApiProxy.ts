/**
 * ApiProxy — the seam that v2 swaps from in-process dispatch to postMessage
 * RPC without touching script code. See SPEC §1 + §3.
 *
 * Drives off `CoreApi` directly via mapped types so adding a method to the
 * contract auto-propagates through `MethodName` / `MethodArgs` / `MethodResult`.
 * Phase 8's PostMessageProxy will implement the same interface — same
 * `MethodName` + `MethodArgs<M>` shape JSON-serialized in the frame
 * (`{ t: 'rpc-req', reqId, method, args }`), per SPEC §8.3.
 *
 * Reads and writes share the same envelope but split method-name namespaces
 * so the dispatcher's capability gating (reads vs writes allowlist per
 * SPEC §4.1) can fire on a single type-level switch.
 */
import type { CoreApi, CoreExchange } from '../api/CoreApi.types';

// ───────────────────────────────────────────────────────────────────────────
// Read seam
// ───────────────────────────────────────────────────────────────────────────

export type ReadMethods = Pick<
  CoreApi,
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
  | 'getCandles'
>;

export type ReadMethodName = keyof ReadMethods;
export type ReadMethodArgs<M extends ReadMethodName> = Parameters<ReadMethods[M]>;
export type ReadMethodResult<M extends ReadMethodName> = Awaited<ReturnType<ReadMethods[M]>>;

// ───────────────────────────────────────────────────────────────────────────
// Write seam — `exchange.*` action methods. Phase 2.
// ───────────────────────────────────────────────────────────────────────────

/** Same shape as `ReadMethods` but for the action surface. */
export type WriteMethods = CoreExchange;
export type WriteMethodName = keyof WriteMethods;
export type WriteMethodArgs<M extends WriteMethodName> = M extends 'runCommand'
  ? [command: string, opts: { account?: string; symbol?: string; depth?: number }]
  : Parameters<WriteMethods[M]>;
export type WriteMethodResult<M extends WriteMethodName> = Awaited<ReturnType<WriteMethods[M]>>;

// ───────────────────────────────────────────────────────────────────────────
// ApiProxy — the unified dispatch seam.
// ───────────────────────────────────────────────────────────────────────────

/**
 * Versioned dispatch seam. Always returns Promise<T> regardless of whether
 * the underlying adapter is sync or async; scripts only ever await.
 *
 * Hosts that wire the proxy through a worker boundary (Phase 8) implement
 * this interface with a postMessage transport; everything else stays
 * identical at the call site.
 */
export interface ApiProxy {
  call<M extends ReadMethodName>(method: M, args: ReadMethodArgs<M>): Promise<ReadMethodResult<M>>;
  /**
   * Write-side dispatch. Phase 2's `InProcessProxy` validates the writes
   * allowlist + account resolution in the dispatcher, then routes to the
   * adapter's action method. Phase 8 swaps in a postMessage transport.
   */
  callWrite<M extends WriteMethodName>(method: M, args: WriteMethodArgs<M>): Promise<WriteMethodResult<M>>;
}
