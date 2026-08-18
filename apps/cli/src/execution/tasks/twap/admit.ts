import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { BinanceExchange } from '@tealstreet/safe-cex/src/exchanges/binance/binance.exchange';
import { BitgetExchange } from '@tealstreet/safe-cex/src/exchanges/bitget/bitget.exchange';
import { BlofinExchange } from '@tealstreet/safe-cex/src/exchanges/blofin/blofin.exchange';
import { BybitExchange } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.exchange';
import { OKXExchange } from '@tealstreet/safe-cex/src/exchanges/okx/okx.exchange';
import { trackedRegularOrderCapabilityFor } from '@tealstreet/safe-cex/src/tracked';
import type { Market } from '@tealstreet/safe-cex/src/types';

export const CONFIRMED_TWAP_CAPABILITIES = [
  'binance:usdt-linear-perp:twap-market',
  'bybit:usdt-linear-perp:twap-market',
  'okx:usdt-linear-perp:twap-market',
  'bitget:usdt-linear-perp:twap-market',
  'blofin:usdt-linear-perp:twap-market',
] as const;

export type ConfirmedTwapCapability = (typeof CONFIRMED_TWAP_CAPABILITIES)[number];

export const SESSION_BOUND_BANNER =
  'CONFIRMED (session-bound: if this CLI exits the task stops; orders already sent remain live at the venue)';

export const capabilityKeyFor = (exchange: Exchange, market: Market): ConfirmedTwapCapability | undefined => {
  const selected = exchange.store.marketsBySymbol[market.symbol];
  if (
    market.active !== true ||
    market.isReadOnly === true ||
    market.isSpot === true ||
    market.isInverse === true ||
    market.isExpiring === true ||
    market.quote !== 'USDT' ||
    selected?.id !== market.id ||
    selected.symbol !== market.symbol
  ) {
    return undefined;
  }
  if (exchange instanceof BinanceExchange) return 'binance:usdt-linear-perp:twap-market';
  if (exchange instanceof BybitExchange) return 'bybit:usdt-linear-perp:twap-market';
  if (exchange instanceof OKXExchange) return 'okx:usdt-linear-perp:twap-market';
  if (exchange instanceof BitgetExchange) return 'bitget:usdt-linear-perp:twap-market';
  if (exchange instanceof BlofinExchange) return 'blofin:usdt-linear-perp:twap-market';
  return undefined;
};

export type TwapEngineAdmission =
  | { readonly engine: 'legacy'; readonly reason: 'explicit-legacy' | 'not-listed' }
  | { readonly engine: 'confirmed'; readonly capability: ConfirmedTwapCapability }
  | { readonly engine: 'error'; readonly capability: ConfirmedTwapCapability; readonly message: string };

export const admitTwapEngine = (input: {
  readonly exchange: Exchange;
  readonly market: Market;
  readonly explicitLegacy: boolean;
}): TwapEngineAdmission => {
  if (input.explicitLegacy) {
    return { engine: 'legacy', reason: 'explicit-legacy' };
  }
  const capability = capabilityKeyFor(input.exchange, input.market);
  if (capability === undefined) {
    return { engine: 'legacy', reason: 'not-listed' };
  }
  if (trackedRegularOrderCapabilityFor(input.exchange, input.market) === undefined) {
    return {
      engine: 'error',
      capability,
      message: `Confirmed TWAP failed to initialize for ${capability}. Retry with the legacy command: twap ... legacy`,
    };
  }
  return { engine: 'confirmed', capability };
};
