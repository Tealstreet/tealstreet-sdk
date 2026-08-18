import { composeClientOrderId } from '../composeClientOrderId';
import type { Exchange } from '../exchanges/base';
import { BinanceExchange } from '../exchanges/binance/binance.exchange';
import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import { BlofinExchange } from '../exchanges/blofin/blofin.exchange';
import { BybitExchange } from '../exchanges/bybit/bybit.exchange';
import { OKXExchange } from '../exchanges/okx/okx.exchange';
import type { Market } from '../types';

import { classifyBinancePlacement, interpretBinanceLookup } from './binance';
import { classifyBitgetPlacement, interpretBitgetLookup } from './bitget';
import { classifyBlofinPlacement, interpretBlofinLookup } from './blofin';
import { classifyBybitPlacement, interpretBybitLookup } from './bybit';
import { classifyOkxPlacement, interpretOkxLookup } from './okx';
import { isRecord } from './placementIds';
import type {
  TrackedLookupResult,
  TrackedPlacementInput,
  TrackedPlacementOutcome,
  TrackedRegularOrderCapability,
} from './types';

type ExactReader = {
  getOrderByClientOrderId(symbol: string, wireClientOrderId: string): Promise<unknown>;
};

const marketIsUsdtLinearPerp = (market: Market): boolean =>
  market.active === true &&
  market.isReadOnly !== true &&
  market.isSpot !== true &&
  market.isInverse !== true &&
  market.isExpiring !== true &&
  market.quote === 'USDT';

const marketBelongsToExchange = (exchange: Exchange, market: Market): boolean => {
  const selected = exchange.store.marketsBySymbol[market.symbol];
  return selected?.id === market.id && selected.symbol === market.symbol;
};
const axiosBody = (error: unknown): unknown => {
  if (isRecord(error) && isRecord(error.response)) {
    return error.response.data;
  }
  return undefined;
};

const lookupOnce = async (
  reader: ExactReader,
  symbol: string,
  wireClientOrderId: string,
): Promise<{ raw: unknown; thrown?: unknown }> => {
  try {
    return { raw: await reader.getOrderByClientOrderId(symbol, wireClientOrderId) };
  } catch (error) {
    const body = axiosBody(error);
    if (body !== undefined) {
      return { raw: body };
    }
    return { raw: undefined, thrown: error };
  }
};

const bindCapability = (
  exchange: Exchange & ExactReader,
  market: Market,
  classifyPlacement: (input: TrackedPlacementInput) => TrackedPlacementOutcome,
  interpret: (raw: unknown, wireClientOrderId: string) => TrackedLookupResult,
): TrackedRegularOrderCapability => ({
  market,
  composeClientOrderId: (callerId: string) => composeClientOrderId(exchange.name, callerId),
  classifyPlacement,
  async lookupExact(input) {
    const observed = await lookupOnce(exchange, market.symbol, input.wireClientOrderId);
    if (observed.thrown !== undefined) {
      return {
        kind: 'indeterminate',
        reason: 'lookup-network-or-http-failure',
        raw: observed.thrown,
      };
    }
    // Single initial observation only. Confirming absence requires a proven
    // visibility-delay bound plus last-write time, which this capability does
    // not have. Absence envelopes therefore stay indeterminate here.
    return interpret(observed.raw, input.wireClientOrderId);
  },
});

export const trackedRegularOrderCapabilityFor = (
  exchange: Exchange,
  market: Market,
): TrackedRegularOrderCapability | undefined => {
  if (
    exchange.isPublicDataOnly ||
    !marketIsUsdtLinearPerp(market) ||
    !marketBelongsToExchange(exchange, market)
  ) {
    return undefined;
  }
  if (typeof (exchange as Exchange & Partial<ExactReader>).getOrderByClientOrderId !== 'function') {
    return undefined;
  }
  const reader = exchange as Exchange & ExactReader;

  if (exchange instanceof BinanceExchange && exchange.name === 'binance') {
    return bindCapability(reader, market, classifyBinancePlacement, (raw, wireId) =>
      interpretBinanceLookup(raw, wireId, market.symbol),
    );
  }
  if (exchange instanceof BybitExchange && exchange.name === 'bybit') {
    return bindCapability(reader, market, classifyBybitPlacement, (raw, wireId) =>
      interpretBybitLookup(raw, wireId, market.id),
    );
  }
  if (exchange instanceof OKXExchange && exchange.name === 'okx') {
    return bindCapability(reader, market, classifyOkxPlacement, (raw, wireId) =>
      interpretOkxLookup(raw, wireId, market.id),
    );
  }
  if (
    exchange instanceof BitgetExchange &&
    exchange.name === 'bitget' &&
    exchange.isClassicV2Account
  ) {
    return bindCapability(reader, market, classifyBitgetPlacement, (raw, wireId) =>
      interpretBitgetLookup(raw, wireId, market.symbol),
    );
  }
  if (exchange instanceof BlofinExchange && exchange.name === 'blofin') {
    return bindCapability(reader, market, classifyBlofinPlacement, (raw, wireId) =>
      interpretBlofinLookup(raw, wireId, market.id),
    );
  }
  return undefined;
};
