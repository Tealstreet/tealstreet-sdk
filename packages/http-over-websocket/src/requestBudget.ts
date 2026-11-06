/** Closed budget labels; these never contain a URL, credential or request body. */
export type RequestClass = 'discovery' | 'orderRead' | 'orderWrite' | 'history' | 'read';

export const REQUEST_BUDGET_MS: Readonly<Record<RequestClass, number>> = Object.freeze({
  discovery: 60_000,
  orderRead: 20_000,
  orderWrite: 20_000,
  history: 30_000,
  read: 20_000,
});
export const MAX_REQUEST_BUDGET_MS = 2_147_483_647;

/** Legacy GET classification only. Posted reads need the engine's semantic label.
 * Exact path components keep queries and arbitrary IDs out of the policy.
 * This selects a wait budget only, never retry or execution semantics.
 */
export const requestClass = (method: string, url: string, hint?: unknown): RequestClass => {
  if (typeof hint === 'string' && Object.hasOwn(REQUEST_BUDGET_MS, hint)) return hint as RequestClass;
  if (method.toUpperCase() !== 'GET') return 'orderWrite';
  // Deliberately raw path components, with the same parser in Rust. Do not
  // normalize or decode IDs into policy vocabulary.
  const match = /^https?:\/\/[^/?#]+/i.exec(url);
  if (!match) return 'read';
  const path = url.slice(match[0].length).split(/[?#]/, 1)[0].replace(/[A-Z]/g, c => c.toLowerCase());
  const parts = path.split('/');
  if (
    parts.some((p) => ['history', 'historical', 'candles', 'kline', 'klines', 'fills', 'trades', 'income', 'allorders'].includes(p))
  )
    return 'history';
  if (parts.some((p) => ['order', 'orders', 'openorders', 'orderstatus'].includes(p))) return 'orderRead';
  if (
    parts.some((p) =>
      ['instruments', 'instruments-info', 'contracts', 'exchangeinfo', 'markets', 'symbols', 'products'].includes(p),
    )
  )
    return 'discovery';
  return 'read';
};

/** Explicit caller/remaining deadlines win. Invalid or absent bounds use the class default. */
export const requestBudgetMs = (kind: RequestClass, timeout?: number): number =>
  typeof timeout === 'number' && Number.isFinite(timeout) && timeout > 0
    ? Math.max(1, Math.min(MAX_REQUEST_BUDGET_MS, Math.floor(timeout)))
    : (Object.hasOwn(REQUEST_BUDGET_MS, kind) ? REQUEST_BUDGET_MS[kind] : REQUEST_BUDGET_MS.read);

export const budgetDiagnostic = (kind: RequestClass, budgetMs: number, elapsedMs: number): string =>
  `class=${kind} budget=${budgetMs}ms elapsed=${Math.max(0, Math.floor(elapsedMs))}ms`;
