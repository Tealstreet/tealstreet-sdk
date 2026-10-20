export type VenueFamily = 'backpack' | 'binance' | 'bingx' | 'bitget' | 'blofin' | 'bybit' | 'cryptocom' | 'hyperliquid' | 'mexc' | 'okx' | 'weex';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;
