import { differences } from './compare.mjs';

export const publicReadMethods = [
  'fetchMarkets',
  'fetchTickers',
  'fetchOHLCV',
  'fetchOrderBook',
  'fetchPublicTrades',
  'fetchOptionMarkets',
  'fetchOptionChain',
  'fetchOptionExpiries',
];

export function publicRestPlan(exchange, description, epoch) {
  const markets = description.markets ?? [];
  const market =
    markets.find((m) => m.active && m.base === 'BTC' && m.quote === 'USDT') ??
    markets.find((m) => m.active && m.base === 'BTC') ??
    markets.find((m) => m.active) ??
    markets[0];
  const symbol = market?.symbol ?? (exchange.startsWith('hyperliquid') ? 'BTC' : 'BTCUSDT');
  const methods = new Set(description.methods);
  const plan = [];
  for (const [interval, duration] of [
    ['1m', 60000],
    ['1h', 3600000],
    ['1d', 86400000],
  ]) {
    const to = Math.floor(epoch / duration) * duration - 1;
    plan.push({
      label: `candles:${interval}`,
      method: 'fetchOHLCV',
      args: [{ symbol, interval, from: to - duration * 10, to, limit: 10 }],
      ...(exchange === 'bitrue' ? { skip: 'WS_REQUIRED: TS fetchOHLCV uses its live public socket' } : {}),
    });
  }
  plan.push({
    label: 'orderbook',
    method: 'fetchOrderBook',
    args: [symbol],
    ...(!methods.has('fetchOrderBook') ? { skip: 'NO_TS_REST_METHOD: production orderbook is a subscription' } : {}),
  });
  plan.push({
    label: 'public-trades',
    method: 'fetchPublicTrades',
    args: [symbol, 10],
    ...(!description.tradeOverride ? { skip: 'NO_TS_REST_METHOD: inherited empty public-trade fallback' } : {}),
  });
  if (exchange === 'bybit') {
    plan.push({ label: 'option-markets', method: 'fetchOptionMarkets', args: [] });
    plan.push({ label: 'option-expiries', method: 'fetchOptionExpiries', args: ['BTC'] });
    plan.push({ label: 'option-chain', method: 'fetchOptionChain', args: [{ underlying: 'BTC' }] });
  }
  plan.push({
    label: 'tickers-funding-open-interest',
    method: 'fetchTickers',
    args: [],
    ...(exchange.startsWith('hyperliquid') ? { method: '$tickers' } : {}),
    ...(['apexomni', 'bitrue', 'lighter'].includes(exchange)
      ? { skip: 'WS_REQUIRED: TS live ticker values require startup streams' }
      : {}),
  });
  return { symbol, plan };
}

export function resultStatus(spec, left, right, records, stable = true) {
  const diffs = differences(left, right);
  if (spec.skip) return { status: 'UNAVAILABLE', reason: spec.skip, diffs: [] };
  if (!stable) return { status: 'NONDETERMINISTIC', diffs };
  if (records.some((r) => r.replayMiss || r.blocked || r.transportError)) return { status: 'INPUT_DIFF', diffs };
  if (records.some((r) => r.status >= 400 && !r.expectedFallback)) return { status: 'BLOCKED', diffs };
  if (left?.timeout || right?.timeout) return { status: 'TIMEOUT', diffs };
  if (diffs.length) return { status: 'DIFF', diffs };
  if (left?.failure || right?.failure || left?.errors || right?.errors) return { status: 'BLOCKED', diffs };
  const value = left?.value;
  if (
    (Array.isArray(value) && !value.length) ||
    (spec.method === 'fetchOrderBook' && !value?.bids?.length && !value?.asks?.length)
  ) {
    return { status: 'EMPTY', diffs };
  }
  return { status: 'PASS', diffs };
}

// Preserve own-undefined and all original fields in the full output artifact.
// These counts describe coverage; they do not replace the exact ticker diff.
export function tickerCoverage(reply) {
  return Object.fromEntries(
    ['fundingRate', 'fundingTime', 'openInterest'].map((field) => {
      const rows = Array.isArray(reply?.value) ? reply.value : [];
      return [
        field,
        {
          rows: rows.length,
          own: rows.filter((r) => Object.hasOwn(r, field)).length,
          finite: rows.filter((r) => Number.isFinite(r[field])).length,
          nonzero: rows.filter((r) => Number.isFinite(r[field]) && r[field] !== 0).length,
        },
      ];
    }),
  );
}
