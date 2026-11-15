import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Market } from '../generated/Market';
import type { Writable } from '../types';

import { MARKET_FLAG } from '@tealstreet/safe-cex-wasm/wire';

// NaN is absence on the packed wire; the native producer owns display policy.
const readOptionalNumber = (value: number): number | undefined => {
  if (Number.isNaN(value)) return undefined;
  if (!Number.isFinite(value)) throw new Error('Nonfinite Market output');
  return value;
};

/** Materialize the generated declaration from native columns and typed metadata. */
export function readMarketFrame(frame: Frame, markets = frame.markets()): Market[] | undefined {
  if (!markets) return undefined;
  const predictionMarkets = frame.predictionMarkets?.();
  const rows: Writable<Market>[] = [];
  for (let i = 0; i < markets.count; i++) {
    const flags = markets.flags[i];
    const flag = (bit: number) => ((flags & bit) !== 0 ? true : undefined);
    const minPrice = readOptionalNumber(markets.minPrice[i]);
    const prettySymbol = markets.prettySymbol?.(i);
    rows.push({
      id: markets.id(i),
      symbol: markets.symbol(i),
      ...(prettySymbol !== undefined && { prettySymbol }),
      base: markets.base(i),
      quote: markets.quote(i),
      active: (flags & MARKET_FLAG.ACTIVE) !== 0,
      precision: { amount: markets.precisionAmount[i], price: markets.precisionPrice[i] },
      limits: {
        amount: { min: markets.minAmount[i], max: markets.maxAmount[i] },
        leverage: { min: markets.minLeverage[i], max: markets.maxLeverage[i] },
        ...(minPrice !== undefined && { price: { min: minPrice } }),
      },
      contractSize: readOptionalNumber(markets.contractSize[i]),
      isInverse: flag(MARKET_FLAG.INVERSE),
      isSpot: flag(MARKET_FLAG.SPOT),
      isPerpetual: flag(MARKET_FLAG.PERPETUAL),
      isExpiring: flag(MARKET_FLAG.EXPIRING),
      isReadOnly: flag(MARKET_FLAG.READ_ONLY),
      takerFee: readOptionalNumber(markets.takerFee[i]),
      makerFee: readOptionalNumber(markets.makerFee[i]),
    });
  }
  if (predictionMarkets) {
    for (let i = 0; i < predictionMarkets.count; i++) {
      const market = rows[predictionMarkets.market[i]];
      if (!market) continue;
      market.info = {
        conditionId: predictionMarkets.conditionId(i),
        tokenId: market.id,
        outcome: predictionMarkets.outcome(i),
        tickSize: predictionMarkets.tickSize[i],
        negRisk: (predictionMarkets.flags[i] & 1) !== 0,
        question: predictionMarkets.question(i),
        description: predictionMarkets.description(i),
        endDate: predictionMarkets.endDate(i),
        slug: predictionMarkets.slug(i),
      };
    }
  }
  const classifications = frame.instrumentClassifications?.();
  if (classifications) {
    for (let i = 0; i < classifications.count; i++) {
      const market = rows[classifications.market[i]];
      if (!market) throw new Error('Instrument classification has no market');
      market.info = {
        ...market.info,
        symbolType: classifications.symbolType(i),
        marketRegion: classifications.marketRegion(i),
        underlyingTicker: classifications.underlyingTicker(i),
      };
    }
  }
  // TS Hyperliquid's merged spot disclosure: the wrapped token behind the pair.
  const underlying = frame.underlyingTokens?.();
  if (underlying) {
    for (let i = 0; i < underlying.count; i++) {
      const market = rows[underlying.market[i]];
      if (!market) throw new Error('Underlying token has no market');
      market.info = {
        ...market.info,
        hlTokenName: underlying.name(i),
        hlTokenFullName: underlying.fullName(i),
      };
    }
  }
  const products = frame.derivativeProducts?.();
  if (products) {
    for (let i = 0; i < products.count; i++) {
      const market = rows[products.market[i]];
      if (!market) throw new Error('Derivative product has no market');
      const kind = products.kind[i];
      const expiry = readOptionalNumber(products.expiry[i]);
      const strike = readOptionalNumber(products.strike[i]);
      const style = products.style(i);
      const settlement = products.settlement(i);
      const settleAsset = products.settleAsset(i);
      const premiumAsset = products.premiumAsset(i);
      const instrumentKind = kind === 1 ? 'FUTURE' : kind === 2 ? 'CALL' : kind === 3 ? 'PUT' : undefined;
      market.info = {
        ...market.info,
        ...(instrumentKind !== undefined && { instrumentKind }),
        underlying: products.underlying(i),
        expiry,
        strike,
      };
      if (
        (kind === 2 || kind === 3) &&
        expiry !== undefined &&
        strike !== undefined &&
        style === 'european' &&
        settlement === 'cash' &&
        settleAsset &&
        premiumAsset
      ) {
        market.option = {
          underlying: products.underlying(i),
          expiry,
          strike,
          right: kind === 2 ? 'call' : 'put',
          style,
          settlement,
          settleAsset,
          premiumAsset,
        };
      }
    }
  }
  return rows;
}
