export type VenueFamily = 'binance' | 'bingx' | 'bybit' | 'cryptocom' | 'okx' | 'weex';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;
