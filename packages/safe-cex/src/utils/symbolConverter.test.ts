import type { ExchangeName } from '../types';

import { describe, expect, it } from 'vitest';

import samples from './__testdata__/market-samples.json';
import { convertSymbol, fromBinance, parseBinanceSymbol, toBinance } from './symbolConverter';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type Sample = {
  symbol: string;
  base: string;
  quote: string;
  binance: string;
  id?: string;
  _note?: string;
  _source?: string;
};

function getSamples(exchange: string): Sample[] {
  const data = (samples as Record<string, unknown>)[exchange];
  if (!Array.isArray(data)) return [];
  return data;
}

const ALL_EXCHANGES: ExchangeName[] = [
  'apexomni',
  'asterdex',
  'backpack',
  'binance',
  'binanceinverse',
  'binancespot',
  'binancepm',
  'bingx',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
  'bitrue',
  'bitunix',
  'blofin',
  'breakoutprop',
  'bybit',
  'bybitdemo',
  'hyrotrader',
  'bybitinverse',
  'bybitspot',
  'coinbase',
  'coinbasespot',
  'cryptocom',
  'grvt',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  'lighter-robinhood',
  'mexc',
  'okx',
  'okxdemo',
  'paradex',
  'phemex',
  'polymarket',
  'kodiakfi',
  'mode',
  'salsadex',
  'tealstreet-dex',
  'txflow',
  'what',
  'weex',
  'woo',
  'woofi',
  'kucoin',
  'kucoinspot',
  'kraken',
];

// Symbols that can't round-trip (expiry dates, vendor prefixes, etc.)
function canRoundTrip(sample: Sample, exchange: string): boolean {
  // Expiring futures: BTCUSDT-03APR26, BTCUSDH26
  if (sample.symbol.match(/-\d{2}[A-Z]{3}\d{2}$/)) return false;
  if (sample.symbol.match(/[FGHJKMNQUVXZ]\d{2}$/)) return false;
  // Vendor-prefixed HL symbols: XYZ|NFLX-PERP
  if (sample.symbol.includes('|')) return false;
  // OKX special suffix: BTCUSD_UM
  if (sample.symbol.includes('_UM')) return false;
  // Lighter slash symbols depend on default quote
  if (exchange === 'lighter' && sample.symbol.includes('/')) return false;
  // Phemex U prefix: U1000SHIBUSDT
  if (exchange === 'phemex' && sample.symbol.startsWith('U') && /\d/.test(sample.symbol[1])) return false;
  // Spot pairs on perp exchanges
  if (sample._note?.includes('spot pair')) return false;
  // Polymarket is non-standard
  if (exchange === 'polymarket') return false;
  return true;
}

// ---------------------------------------------------------------------------
// parseBinanceSymbol
// ---------------------------------------------------------------------------

describe('parseBinanceSymbol', () => {
  const cases: [string, string, string][] = [
    ['BTCUSDT', 'BTC', 'USDT'],
    ['1000PEPEUSDT', '1000PEPE', 'USDT'],
    ['KAITOUSDC', 'KAITO', 'USDC'],
    ['BTCUSD', 'BTC', 'USD'],
    ['BTCUSDG', 'BTC', 'USDG'],
    ['CUSDT', 'C', 'USDT'],
    ['TUSDT', 'T', 'USDT'],
    ['1INCHUSDT', '1INCH', 'USDT'],
    ['4USDT', '4', 'USDT'],
  ];

  for (const [symbol, base, quote] of cases) {
    it(`${symbol} → base=${base}, quote=${quote}`, () => {
      expect(parseBinanceSymbol(symbol)).toEqual({ base, quote });
    });
  }
});

// ---------------------------------------------------------------------------
// toBinance — every sample from every exchange
// ---------------------------------------------------------------------------

describe('toBinance', () => {
  for (const exchange of ALL_EXCHANGES) {
    if (exchange === 'polymarket') continue;
    const exchangeSamples = getSamples(exchange);
    if (exchangeSamples.length === 0) continue;

    describe(exchange, () => {
      for (const sample of exchangeSamples) {
        it(`${sample.symbol} → ${sample.binance}`, () => {
          expect(toBinance(sample.symbol, exchange as ExchangeName)).toBe(sample.binance);
        });
      }
    });
  }
});

// ---------------------------------------------------------------------------
// fromBinance — every sample, Binance format back to exchange format
// ---------------------------------------------------------------------------

describe('fromBinance', () => {
  for (const exchange of ALL_EXCHANGES) {
    if (exchange === 'polymarket') continue;
    const exchangeSamples = getSamples(exchange);
    if (exchangeSamples.length === 0) continue;

    describe(exchange, () => {
      for (const sample of exchangeSamples) {
        if (!canRoundTrip(sample, exchange)) continue;

        it(`${sample.binance} → ${sample.symbol}`, () => {
          expect(fromBinance(sample.binance, exchange as ExchangeName)).toBe(sample.symbol);
        });
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Round-trip: toBinance → fromBinance = original symbol
// ---------------------------------------------------------------------------

describe('round-trip: toBinance then fromBinance', () => {
  for (const exchange of ALL_EXCHANGES) {
    if (exchange === 'polymarket') continue;
    const exchangeSamples = getSamples(exchange);
    if (exchangeSamples.length === 0) continue;

    describe(exchange, () => {
      for (const sample of exchangeSamples) {
        if (!canRoundTrip(sample, exchange)) continue;

        it(`${sample.symbol} → binance → ${exchange}`, () => {
          const binance = toBinance(sample.symbol, exchange as ExchangeName);
          const back = fromBinance(binance, exchange as ExchangeName);
          expect(back).toBe(sample.symbol);
        });
      }
    });
  }
});

// ---------------------------------------------------------------------------
// convertSymbol — cross-exchange via Binance intermediary
// ---------------------------------------------------------------------------

describe('convertSymbol', () => {
  // Identity: same exchange returns same symbol
  describe('identity', () => {
    for (const exchange of ALL_EXCHANGES) {
      if (exchange === 'polymarket') continue;
      const exchangeSamples = getSamples(exchange);
      if (exchangeSamples.length === 0) continue;

      const sample = exchangeSamples[0];
      it(`${exchange}: ${sample.symbol}`, () => {
        expect(convertSymbol(sample.symbol, exchange as ExchangeName, exchange as ExchangeName)).toBe(sample.symbol);
      });
    }
  });

  // Cross-exchange: pick BTC sample from each exchange, convert to every other
  describe('BTC cross-exchange', () => {
    // Get the BTC sample from each exchange (first sample is always BTC-like)
    const btcByExchange: Record<string, Sample> = {};
    for (const exchange of ALL_EXCHANGES) {
      if (exchange === 'polymarket') continue;
      const exchangeSamples = getSamples(exchange);
      const btcSample = exchangeSamples.find(
        (s) => s.binance === 'BTCUSDT' || s.binance === 'BTCUSD' || s.binance === 'BTCUSDC',
      );
      if (btcSample && canRoundTrip(btcSample, exchange)) {
        btcByExchange[exchange] = btcSample;
      }
    }

    // For each pair of exchanges that share the same Binance BTC symbol, test conversion
    const exchanges = Object.keys(btcByExchange) as ExchangeName[];
    for (const from of exchanges) {
      for (const to of exchanges) {
        if (from === to) continue;
        const fromSample = btcByExchange[from];
        const toSample = btcByExchange[to];
        // Only test when they share the same Binance representation
        if (fromSample.binance !== toSample.binance) continue;

        it(`${from} ${fromSample.symbol} → ${to} ${toSample.symbol}`, () => {
          expect(convertSymbol(fromSample.symbol, from, to)).toBe(toSample.symbol);
        });
      }
    }
  });
});

describe('OKX demo symbol parity', () => {
  it('uses the same compact symbol behavior as OKX', () => {
    expect(toBinance('BTCUSDT', 'okxdemo')).toBe(toBinance('BTCUSDT', 'okx'));
    expect(toBinance('BTCUSD_UM', 'okxdemo')).toBe(toBinance('BTCUSD_UM', 'okx'));
    expect(fromBinance('BTCUSDT', 'okxdemo')).toBe(fromBinance('BTCUSDT', 'okx'));
  });

  it('uses the same compact symbol behavior as OKX EU', () => {
    expect(toBinance('BTCUSDT', 'okxeudemo')).toBe(toBinance('BTCUSDT', 'okxeu'));
    expect(toBinance('BTCUSD_UM', 'okxeudemo')).toBe(toBinance('BTCUSD_UM', 'okxeu'));
    expect(fromBinance('BTCUSDT', 'okxeudemo')).toBe(fromBinance('BTCUSDT', 'okxeu'));
  });
});
