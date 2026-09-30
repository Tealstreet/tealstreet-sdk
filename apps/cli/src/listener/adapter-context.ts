// Per-call adapter override threaded via AsyncLocalStorage. Used by the
// loopback listener's cmd:exec handler to redirect print* output to a
// streaming WS frame emitter without touching every adapter.printX call
// site in repl.ts.
//
// The real adapter's print* methods consult als.getStore() first; if a
// `printSink` is set, calls route there instead. Outside an ALS scope
// (the REPL prompt path, async event listeners, exchange fill events)
// als.getStore() is undefined and the real adapter behaves normally.

import { AsyncLocalStorage } from 'node:async_hooks';

export type PrintKind = 'print' | 'success' | 'warn' | 'error' | 'info';

export interface AdapterContext {
  /**
   * OPTIONAL. A context may exist purely to scope a run to an account/symbol
   * without redirecting output — `when`'s armed trigger does exactly that, and
   * its fired command's output belongs on the terminal like any other.
   * Consumers must therefore guard on `ctx?.printSink`, not on `ctx`.
   */
  printSink?: (kind: PrintKind, text: string) => void;
  // Per-call overrides for `cmd:exec`. When present, `processCommand`
  // uses these instead of the REPL's `session.currentAccountName` /
  // `currentSymbol` so a web tab's selected account/symbol scopes the
  // run without mutating the long-lived REPL session. Resolved exchange
  // is looked up via `exchangeManager.getExchange(accountOverride)`.
  accountOverride?: string;
  symbolOverride?: string;
  /**
   * Discriminates the invocation source for downstream consumers that
   * need to distinguish a web tab's `cmd:exec` (which from the user's
   * perspective is an inline/UI-initiated command) from a true
   * external webhook POST. Both run under the same ALS context shape,
   * so without this field downstream code can't tell them apart.
   *
   * Phase 5's `script:<name>` dispatcher reads this to populate
   * `api.meta.invokedFrom` correctly per SPEC §6 (`'inline'` for web
   * tabs, `'webhook'` for HTTP hook fires). Leaving the field
   * undefined means "treat as REPL / direct invocation".
   */
  source?: 'cmd-exec' | 'webhook';
}

const als = new AsyncLocalStorage<AdapterContext>();

export function getAdapterContext(): AdapterContext | undefined {
  return als.getStore();
}

export function runWithAdapterContext<T>(ctx: AdapterContext, fn: () => Promise<T>): Promise<T> {
  return als.run(ctx, fn);
}
