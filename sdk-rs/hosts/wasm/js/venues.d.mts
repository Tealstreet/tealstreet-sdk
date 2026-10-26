/** Every family build, by its hosts/wasm Cargo feature name. */
export const venueFamilies: readonly string[];
/** The family build containing a Rust venue; throws for an unknown venue. */
export function venueFamily(venue: string): string;
