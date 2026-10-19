export type VenueFamily = 'binance' | 'bybit' | 'okx';
/** Rejects unsupported names before loading an engine. */
export function venueFamily(venue: string): VenueFamily;
