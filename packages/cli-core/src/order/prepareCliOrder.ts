import type { ExchangeLike } from '../types';
import type { ExchangeName, PlaceOrderOpts } from '@tealstreet/safe-cex/src/types';

import { tradfiPreflightMessage } from '@tealstreet/safe-cex/src/instrument-class/describe';
import { classifyMarket } from '@tealstreet/safe-cex/src/instrument-class/resolve';
import { assertMinimumPrice, roundOrderPrice } from '@tealstreet/script-api-core/src/order/orderPricing';
import { nativePositionSize } from '@tealstreet/safe-cex/src/utils/nativePositionSize';
import { clampReduceOnlyOrder, prepareOrder } from '@tealstreet/script-api-core/src/order/prepareOrder';
import { ScriptValidationError } from '@tealstreet/script-api-core/src/order/ScriptValidationError';

/**
 * The CLI's door onto the shared order policy.
 *
 * Trading commands used to call `exchange.placeOrder(...)` with whatever they
 * had computed. So `buy`/`sell` never rounded a limit price to the tick,
 * `stop` sent an unrounded trigger AND an `amount: 0` infinity stop to the
 * twelve venues that do not support one, `close` sent a raw
 * `Math.abs(position.contracts)`, and nothing anywhere applied the reduce-only
 * clamp. Web, mobile and scripts had all of it, through `createCoreAdapter`.
 *
 * These commands could not simply take an adapter. They run from three hosts —
 * the REPL, the script adapter, and `worker-host/router.ts`, which is the web
 * app's in-browser CLI worker — and two of those build a `CommandContext` with
 * a bare exchange handle and no account name at all, which is the thing a
 * `CoreAdapter` resolves everything through. Lifting the policy out of that
 * closure into `prepareOrder` is what made it reachable; this is the thin part
 * that stays here.
 *
 * The market lookup THROWS rather than passing the order through unrounded.
 * "Market not loaded yet" is precisely the state in which unrounded numbers
 * were reaching the venue, so it is the state that has to refuse.
 */
export const prepareCliOrder = <T extends { symbol: string; type: PlaceOrderOpts['type']; amount: number }>(
  exchange: ExchangeLike,
  opts: T,
): PlaceOrderOpts => {
  const market = exchange.store?.markets?.find((m) => m.symbol === opts.symbol);
  if (!market) throw new ScriptValidationError(`Market not found for ${opts.symbol}`);

  // `ExchangeLike.name` is a plain string; every venue check downstream treats
  // an unrecognised one as "not this venue" and fails closed.
  const exchangeName = exchange.name as ExchangeName;

  // Tradfi preflight. `classifyMarket` reads from `Market.info` with no new
  // REST call, and returns `unknown` on every venue whose classifier is not
  // written yet — so an unrecognised venue silently passes through. Only a
  // positively classified non-crypto instrument refuses. Binance's TradFi
  // perps (`contractType: 'TRADIFY_PERPETUAL'`, with Binance's own typo) are
  // the obvious next entry here, but their classifier is deliberately out of
  // this pass — Bybit is where the user report came from.
  const tradfiMsg = tradfiPreflightMessage(opts.symbol, exchangeName, classifyMarket(exchangeName, market));
  if (tradfiMsg) throw new ScriptValidationError(tradfiMsg);

  const rounded = prepareOrder(opts, { market, exchangeName });

  return clampReduceOnlyOrder(rounded, {
    market,
    exchangeName,
    positions: exchange.store?.positions ?? [],
    restingOrders: exchange.store?.orders ?? [],
    ticker: exchange.store?.tickers?.find((t) => t.symbol === opts.symbol),
  });
};

/**
 * The plural form, for a caller that sends a whole ladder in one venue call —
 * `scale`, via `exchange.placeOrders`. A throw from ANY rung rejects the batch,
 * which is right there: one `placeOrders` either goes or it does not.
 *
 * `swarm` deliberately does NOT use this. It fires each slice as its own
 * `placeOrder` and settles them independently, so it prepares per slice inside
 * an async map — see the comment at its call site.
 */
export const prepareCliOrders = <T extends { symbol: string; type: PlaceOrderOpts['type']; amount: number }>(
  exchange: ExchangeLike,
  orders: readonly T[],
): PlaceOrderOpts[] => orders.map((order) => prepareCliOrder(exchange, order));

/**
 * Round an amend's price to the venue tick. `bump` and `move` shift a resting
 * order's price by an offset and send the result raw.
 *
 * Deliberately price-only: `roundOrderSizeToNearest` is the amend SIZE rule
 * and neither command touches size, so pulling it in here would invite a
 * caller to round a size with the placement rule by accident.
 */
export const prepareCliAmendPrice = (exchange: ExchangeLike, symbol: string, price: number): number => {
  const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
  if (!market) throw new ScriptValidationError(`Market not found for ${symbol}`);
  const rounded = roundOrderPrice(price, market);
  // Rounding itself can produce zero, so a caller's pre-rounding `price > 0`
  // check does not cover this. `bump`/`move` validate the shifted price before
  // handing it over, and `roundOrderPrice(0.0002, tick 0.001)` is 0 — which
  // would amend a resting order to `price: 0`. `prepareOrder` and
  // `createCoreAdapter.updateOrder` both re-check; this is the third copy.
  if (!Number.isFinite(rounded) || rounded <= 0) {
    throw new ScriptValidationError(`price ${price} rounds to ${rounded} for '${symbol}' — below the venue tick`);
  }
  assertMinimumPrice(rounded, market);
  return rounded;
};

/**
 * A position's size in the unit an order's `amount` is in.
 *
 * `close` and a bare `%pos` size both took `position.contracts` straight from
 * the store and handed it to `placeOrder`. That is BASE, and `amount` is
 * NATIVE — they differ by `contractSize`, so on OKX's `DOGE-USDT-SWAP` (1000
 * DOGE per contract) a full close asked for a thousand times the position, and
 * on a Bybit inverse market it asked for a fraction of it.
 *
 * `prepareCliOrder` cannot fix this: it rounds and validates the amount it is
 * GIVEN, it does not know the number was in the wrong unit to begin with. So
 * the conversion has to happen at the point the size is read off a position,
 * which is here.
 *
 * REFUSES when the market has not loaded, rather than falling back to the raw
 * count. The fallback was tempting — it is the unit the code used before, and
 * `prepareCliOrder` refuses a moment later anyway — but there is no `await`
 * between the two, so a market missing at the size read is missing at
 * placement too. There is no race to protect against, and the fallback would
 * only make the old bug a tested behaviour and leave a future caller one
 * missed `prepareCliOrder` away from re-opening it.
 *
 * The missing-`contractSize` fallback inside `nativePositionSize` is different
 * and stays: there IS a market there, and one-to-one is the rule.
 */
export const nativeCloseSize = (
  exchange: ExchangeLike,
  position: { symbol: string; contracts: number; notional?: number; extra?: { contractsNative?: number } },
): number => {
  const market = exchange.store?.markets?.find((m) => m.symbol === position.symbol);
  if (!market) throw new ScriptValidationError(`Market not found for ${position.symbol}`);
  return nativePositionSize(position, market);
};
