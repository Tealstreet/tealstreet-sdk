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
  'fetchSymbolMaxLeverage',
  'fetchTickersRaw',
  'fetchTickersWithoutParse',
  'fetchTickersIntx',
  'fetchTickersCfm',
  'fetchTickersSpot',
];

export function publicRestPlan(exchange, description, epoch, { enableOptions = true } = {}) {
  const coinbase = ['coinbase', 'coinbasespot'].includes(exchange);
  const markets = description.markets ?? [];
  const market =
    markets.find((m) => m.active && m.base === 'BTC' && m.quote === 'USDT') ??
    markets.find((m) => m.active && m.base === 'BTC') ??
    markets.find((m) => m.active) ??
    markets[0];
  const symbol = market?.symbol ?? (exchange.startsWith('hyperliquid') ? 'BTC' : 'BTCUSDT');
  const methods = new Set(description.methods);
  const plan = [
    { label: 'markets', method: exchange.startsWith('hyperliquid') ? '$markets' : 'fetchMarkets', args: [] },
  ];
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
      ...(coinbase ? { skip: 'AUTH_REQUIRED: TS Coinbase candles use the authenticated brokerage endpoint' } : {}),
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
  if (['bybit', 'bybitdemo'].includes(exchange)) {
    plan.push({ label: 'option-markets', method: 'fetchOptionMarkets', args: [] });
    plan.push({ label: 'option-expiries', method: 'fetchOptionExpiries', args: ['BTC'] });
    plan.push({ label: 'option-chain', method: 'fetchOptionChain', args: [{ underlying: 'BTC' }] });
  }
  for (const method of [
    'fetchSymbolMaxLeverage',
    'fetchTickersRaw',
    'fetchTickersWithoutParse',
    'fetchTickersIntx',
    'fetchTickersCfm',
    'fetchTickersSpot',
  ]) {
    if (!(description.overrides ?? []).includes(method)) continue;
    if (coinbase && ['fetchTickersCfm', 'fetchTickersSpot'].includes(method)) {
      plan.push({ label: method, method, args: [], skip: 'AUTH_REQUIRED: TS Coinbase brokerage products require JWT' });
      continue;
    }
    // Bitrue max leverage may call a signed leverage-bracket read. Keep
    // authenticated metadata out of this keyless method inventory.
    if (method === 'fetchSymbolMaxLeverage' && exchange === 'bitrue') {
      plan.push({ label: method, method, args: [symbol], skip: 'AUTH_REQUIRED: leverage-bracket metadata' });
      continue;
    }
    plan.push({ label: method, method, args: method === 'fetchSymbolMaxLeverage' ? [symbol] : [] });
  }
  plan.push({
    label: 'tickers-funding-open-interest',
    method: 'fetchTickers',
    args: [],
    ...(exchange.startsWith('hyperliquid') ? { method: '$tickers' } : {}),
    // The maintained public-only startup uses INTX directly. CoinbaseSpot's
    // general fetchTickers override first probes authenticated portfolios.
    ...(coinbase ? { method: 'fetchTickersIntx' } : {}),
    ...(['apexomni', 'bitrue', 'lighter', 'lighter-robinhood'].includes(exchange)
      ? { skip: 'WS_REQUIRED: TS live ticker values require startup streams' }
      : {}),
  });
  for (const spec of plan) {
    if (!enableOptions && spec.label.startsWith('option-'))
      spec.skip = 'PROFILE_DISABLED: options are tested in the separate options-enabled capture';
    if (
      (!market && spec.args?.[0]?.symbol) ||
      (!market && ['orderbook', 'public-trades', 'fetchSymbolMaxLeverage'].includes(spec.label))
    )
      spec.skip = 'NO_TRADING_MARKETS: TS bootstrap returned no market for this read';
  }
  return { symbol, plan };
}

export function resultStatus(spec, left, right, records, stable = true) {
  const diffs = differences(left, right);
  if (spec.skip) return { status: 'UNAVAILABLE', reason: spec.skip, diffs: [] };
  if (records.some((r) => r.replayMiss || r.blocked || r.transportError)) return { status: 'INPUT_DIFF', diffs };
  if (left?.timeout || right?.timeout) return { status: 'TIMEOUT', diffs: [] };
  if (left?.notRun || right?.notRun) return { status: 'NOT_RUN', diffs: [] };
  if (left?.probeFailure || right?.probeFailure) return { status: 'BLOCKED', diffs: [] };
  if (differences(left?.clock, right?.clock).length) return { status: 'CLOCK_DIFF', diffs };
  if (!stable) return { status: 'NONDETERMINISTIC', diffs };
  if (records.some((r) => r.status >= 400 && !r.expectedFallback)) return { status: 'BLOCKED', diffs };
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

// Store snapshots and derived option/leverage reads inherit bootstrap timing.
// Matching clocks at the later read do not establish the same startup state.
export function qualifyUpstream(checks) {
  const bootstrap = checks.find((check) => check.label === 'bootstrap-markets');
  if (!bootstrap || bootstrap.status === 'PASS') return checks;
  const stateful = new Set([
    '$markets',
    '$tickers',
    'fetchOptionChain',
    'fetchOptionExpiries',
    'fetchSymbolMaxLeverage',
  ]);
  return checks.map((check) =>
    stateful.has(check.method) && ['PASS', 'EMPTY', 'DIFF'].includes(check.status)
      ? {
          ...check,
          unqualifiedStatus: check.status,
          status: 'UPSTREAM_BLOCKED',
          reason: `Bootstrap ${bootstrap.status}: Store-derived values do not establish identical startup state`,
        }
      : check,
  );
}
