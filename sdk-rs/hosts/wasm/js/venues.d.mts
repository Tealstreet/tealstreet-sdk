export type VenueFamily = 'backpack' | 'binance' | 'bingx' | 'bitget' | 'bitrue' | 'blofin' | 'bybit' | 'cryptocom' | 'hyperliquid' | 'lighter' | 'mexc' | 'orderly' | 'okx' | 'weex' | 'woo';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;
