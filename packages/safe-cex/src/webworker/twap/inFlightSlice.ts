import type { Exchange } from '../../exchanges/base';
import type { Order, OrderSide } from '../../types';
import type { TwapState } from './TwapState';

/** How a restore settles its in-flight slice; `order` is the slice's order when it was found. */
export type InFlightSliceOutcome = { placed: boolean; order?: Order };

/**
 * Settles a slice a previous worker was placing when it died: placed when it
 * was found by client id, or is not confirmable either way (with a warning,
 * because placing it again could double it); not placed on a definitive miss,
 * so its size is placed again.
 */
export const settleInFlightSlice = async (
  exchange: Exchange,
  symbol: string,
  inFlight: NonNullable<TwapState['inFlight']>,
  side: OrderSide,
  isCurrent: () => boolean = () => true,
): Promise<InFlightSliceOutcome> => {
  // Everything saved about it, so the user can find the order by hand.
  const at = inFlight.price ? ` at ${inFlight.price}` : '';
  const id = inFlight.clientOrderId ? ` (client id ${inFlight.clientOrderId})` : '';
  const warn = () =>
    isCurrent() &&
    exchange.emitter.emit(
      'error',
      `TWAP ${symbol}: a ${side} ${inFlight.size}${at} slice${id} was being placed when the page closed and could not be confirmed; it is not placed again. Check your orders.`,
    );
  if (!inFlight.clientOrderId) {
    warn();
    return { placed: true };
  }
  try {
    const lookup = await exchange.findOrderByClientOrderId(symbol, inFlight.clientOrderId);
    if (lookup.found) return { placed: true, order: lookup.order };
    if (!lookup.definitive) warn();
    return { placed: !lookup.definitive };
  } catch {
    warn();
    return { placed: true };
  }
};
