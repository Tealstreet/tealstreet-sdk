// Single source of truth for HOW (tradetunnel) egress region selection.
//
// Pure data + predicate, deliberately free of any side effects (no HOWWebSocket
// construction, no configureHOW) so it can be imported by pure consumers — most
// importantly safe-cex's routing table, which derives its declared `howEgress`
// from this list. This package owns the tunnel; safe-cex depends on it, never the
// reverse, so the list lives here in exactly one place.

// Exchanges that should use the Tokyo (jp) egress server. Matched as a
// case-insensitive substring against either an exchange name OR a request
// baseURL, so `arkham`/`arkm`, `binance`/`binancespot`, `bitget`/`bitgetusdc`,
// `breakout`/`breakoutprop`, `aster`/`asterdex`, etc. all resolve together.
export const TOKYO_EGRESS_EXCHANGES = [
  // url pattern, so both arkham & arkm
  'arkham',
  'arkm',
  'hlnode',
  'hyperliquid',
  'hyperliquidspot',
  'bitunix',
  'bitrue',
  'bitget',
  'binance',
  'kucoin',
  'kraken',
  'breakout',
  'aster',
  'paradex',
  'grvt',
  'backpack',
] as const;

export const TOKYO_EGRESS_PATTERN = new RegExp(TOKYO_EGRESS_EXCHANGES.join('|'), 'i');

// Does this key (an exchange name OR a request baseURL) egress via Tokyo (vs
// Singapore)? The one predicate every egress decision routes through.
export const isTokyoEgress = (key: string): boolean => TOKYO_EGRESS_PATTERN.test(key);
