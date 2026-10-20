// Literal family imports preserve separate browser chunks. Initialization is
// shared with explicit SDK loaders through the same per-family promise cache.
import { initializeWebEngine } from './engine-cache.mjs';
import { venueFamily } from './venues.mjs';

function importEngine(family) {
  switch (family) {
    case 'txflow': return import('../pkg/web/txflow/safe_cex_wasm.js');
    case 'coinbase': return import('../pkg/web/coinbase/safe_cex_wasm.js');
    case 'polymarket': return import('../pkg/web/polymarket/safe_cex_wasm.js');
    case 'grvt': return import('../pkg/web/grvt/safe_cex_wasm.js');
    case 'breakoutprop': return import('../pkg/web/breakoutprop/safe_cex_wasm.js');
    case 'phemex': return import('../pkg/web/phemex/safe_cex_wasm.js');
    case 'paradex': return import('../pkg/web/paradex/safe_cex_wasm.js');
    case 'apexomni': return import('../pkg/web/apexomni/safe_cex_wasm.js');
    case 'kraken': return import('../pkg/web/kraken/safe_cex_wasm.js');
    case 'asterdex': return import('../pkg/web/asterdex/safe_cex_wasm.js');
    case 'kucoin': return import('../pkg/web/kucoin/safe_cex_wasm.js');
    case 'backpack': return import('../pkg/web/backpack/safe_cex_wasm.js');
    case 'binance': return import('../pkg/web/binance/safe_cex_wasm.js');
    case 'bingx': return import('../pkg/web/bingx/safe_cex_wasm.js');
    case 'bitget': return import('../pkg/web/bitget/safe_cex_wasm.js');
    case 'bitrue': return import('../pkg/web/bitrue/safe_cex_wasm.js');
    case 'blofin': return import('../pkg/web/blofin/safe_cex_wasm.js');
    case 'bybit': return import('../pkg/web/bybit/safe_cex_wasm.js');
    case 'cryptocom': return import('../pkg/web/cryptocom/safe_cex_wasm.js');
    case 'hyperliquid': return import('../pkg/web/hyperliquid/safe_cex_wasm.js');
    case 'lighter': return import('../pkg/web/lighter/safe_cex_wasm.js');
    case 'mexc': return import('../pkg/web/mexc/safe_cex_wasm.js');
    case 'okx': return import('../pkg/web/okx/safe_cex_wasm.js');
    case 'orderly': return import('../pkg/web/orderly/safe_cex_wasm.js');
    case 'weex': return import('../pkg/web/weex/safe_cex_wasm.js');
    case 'woo': return import('../pkg/web/woo/safe_cex_wasm.js');
  }
}

export function loadEngine(venue) {
  const family = venueFamily(venue);
  return initializeWebEngine(family, () => importEngine(family));
}
