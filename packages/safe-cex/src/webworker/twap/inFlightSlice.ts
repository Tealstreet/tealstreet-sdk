import type { Exchange } from '../../exchanges/base';
import type { TwapState } from './TwapState';

/**
 * Settles a slice a previous worker was placing when it died: true when it
 * counts as placed (found by client id, or not confirmable either way, with a
 * warning, because placing it again could double it); false on a definitive
 * miss, so its size is placed again.
 */
export const inFlightSlicePlaced = async (
  exchange: Exchange,
  symbol: string,
  inFlight: NonNullable<TwapState['inFlight']>,
): Promise<boolean> => {
  const warn = () =>
    exchange.emitter.emit(
      'error',
      `TWAP ${symbol}: a ${inFlight.size} slice was being placed when the page closed and could not be confirmed; it is not placed again. Check your orders.`,
    );
  if (!inFlight.clientOrderId) {
    warn();
    return true;
  }
  try {
    const lookup = await exchange.findOrderByClientOrderId(symbol, inFlight.clientOrderId);
    if (!lookup.found && !lookup.definitive) warn();
    return lookup.found || !lookup.definitive;
  } catch {
    warn();
    return true;
  }
};
