export type VenueFamily = 'txflow' | 'coinbase' | 'polymarket' | 'grvt' | 'breakoutprop' | 'phemex' | 'paradex' | 'apexomni' | 'kraken' | 'asterdex' | 'kucoin' | 'kucoinspot' | 'backpack' | 'binance' | 'bingx' | 'bitget' | 'bitrue' | 'blofin' | 'bybit' | 'cryptocom' | 'hyperliquid' | 'lighter' | 'mexc' | 'orderly' | 'okx' | 'weex' | 'woo';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;

/** Canonical build families, once each; aliases use venueFamily. */
export const venueFamilies: readonly VenueFamily[];
