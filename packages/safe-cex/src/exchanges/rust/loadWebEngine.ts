import type { VenueFamily } from '@tealstreet/safe-cex-wasm/venues';
import type { WasmEngineModule } from './rust.exchange';

import { initializeWebEngine } from '@tealstreet/safe-cex-wasm/engine-cache';
import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';

// Literal imports keep each venue in its own browser chunk.
const importEngine = (family: VenueFamily) => {
  switch (family) {
    case 'asterdex':
      return import('@tealstreet/safe-cex-wasm/web/asterdex');
    case 'kucoin':
      return import('@tealstreet/safe-cex-wasm/web/kucoin');
    case 'orderly':
      return import('@tealstreet/safe-cex-wasm/web/orderly');
    case 'woo':
      return import('@tealstreet/safe-cex-wasm/web/woo');
    case 'lighter':
      return import('@tealstreet/safe-cex-wasm/web/lighter');
    case 'bitrue':
      return import('@tealstreet/safe-cex-wasm/web/bitrue');
    case 'backpack':
      return import('@tealstreet/safe-cex-wasm/web/backpack');
    case 'binance':
      return import('@tealstreet/safe-cex-wasm/web/binance');
    case 'bingx':
      return import('@tealstreet/safe-cex-wasm/web/bingx');
    case 'bitget':
      return import('@tealstreet/safe-cex-wasm/web/bitget');
    case 'blofin':
      return import('@tealstreet/safe-cex-wasm/web/blofin');
    case 'bybit':
      return import('@tealstreet/safe-cex-wasm/web/bybit');
    case 'cryptocom':
      return import('@tealstreet/safe-cex-wasm/web/cryptocom');
    case 'hyperliquid':
      return import('@tealstreet/safe-cex-wasm/web/hyperliquid');
    case 'mexc':
      return import('@tealstreet/safe-cex-wasm/web/mexc');
    case 'okx':
      return import('@tealstreet/safe-cex-wasm/web/okx');
    case 'weex':
      return import('@tealstreet/safe-cex-wasm/web/weex');
  }
};

/** Loads only this venue's engine, shared by accounts and aliases in this realm. */
export const loadWebEngine = (venue: string): Promise<WasmEngineModule> => {
  const family = venueFamily(venue);
  return initializeWebEngine(family, () => importEngine(family));
};
