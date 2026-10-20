export type VenueFamily = 'apexomni' | 'kraken' | 'asterdex' | 'kucoin' | 'backpack' | 'binance' | 'bingx' | 'bitget' | 'bitrue' | 'blofin' | 'bybit' | 'cryptocom' | 'hyperliquid' | 'lighter' | 'mexc' | 'orderly' | 'okx' | 'weex' | 'woo';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;
