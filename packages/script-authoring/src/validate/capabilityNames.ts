import type { ReadMethodName, WriteMethodName } from '@tealstreet/script-api-core';

/**
 * Runtime mirrors of the type-level unions in
 * `script-api-core/src/api/capabilities.types.ts`.
 *
 * They live here rather than as runtime exports in core because adding a value
 * export to that file would change the core API surface, trip
 * `scripts/check-api-version-bump.mjs`, and force a types-bundle regen — all
 * for arrays only the authoring-time checker needs.
 *
 * Drift is caught at compile time by the assertions below instead.
 *
 * `as const` is MANDATORY. Without it `(typeof ARR)[number]` widens to
 * `string`, `_MissingWrite` silently passes, and only the `_Extra` direction
 * still errors. Do not annotate these `: readonly WriteMethodName[]` either —
 * that defeats the missing-member direction the same way.
 */
export const WRITE_METHOD_NAMES = [
  'placeOrder',
  'placeOrders',
  'cancelOrder',
  'cancelOrders',
  'updateOrder',
  'closePosition',
  'setLeverage',
  'setPositionMode',
  'setMarginType',
  'runCommand',
] as const;

export const READ_METHOD_NAMES = [
  'getActiveAccount',
  'getActiveAccounts',
  'getActiveSymbol',
  'getActiveExchange',
  'getAccounts',
  'getMarkets',
  'getMarket',
  'getTicker',
  'getTickers',
  'getPositions',
  'getHistoricalPosition',
  'getOrders',
  'getBalance',
  'getCombinedBalance',
  'getOrderbook',
  'getCandles',
] as const;

/**
 * Compile-time drift guards. If core adds or removes a method name, one of
 * these fails to compile with TS2344 and this package's typecheck goes red —
 * which is the point. Keep BOTH directions: `_Extra*` doubles as the canary
 * for a lost `as const`.
 */
type Assert<T extends never> = T;
type _MissingWrite = Assert<Exclude<WriteMethodName, (typeof WRITE_METHOD_NAMES)[number]>>;
type _ExtraWrite = Assert<Exclude<(typeof WRITE_METHOD_NAMES)[number], WriteMethodName>>;
type _MissingRead = Assert<Exclude<ReadMethodName, (typeof READ_METHOD_NAMES)[number]>>;
type _ExtraRead = Assert<Exclude<(typeof READ_METHOD_NAMES)[number], ReadMethodName>>;

/**
 * Writes that the runtime gates as more than themselves.
 *
 * `closePosition` is synthesized by both adapters as
 * `placeOrder({ reduceOnly: true })`, so `buildApi.ts` validates BOTH
 * capabilities. A static check that required only `closePosition` would
 * green-light a script the runtime then throws on.
 */
export const IMPLIED_WRITES: Readonly<Partial<Record<WriteMethodName, readonly WriteMethodName[]>>> = Object.freeze({
  closePosition: ['placeOrder'] as const,
});

export const isWriteMethod = (name: string): name is WriteMethodName =>
  (WRITE_METHOD_NAMES as readonly string[]).includes(name);

export const isReadMethod = (name: string): name is ReadMethodName =>
  (READ_METHOD_NAMES as readonly string[]).includes(name);
