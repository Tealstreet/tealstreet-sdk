export type VenueFamily = 'binance' | 'bingx' | 'blofin' | 'bybit' | 'cryptocom' | 'mexc' | 'okx' | 'weex';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;
