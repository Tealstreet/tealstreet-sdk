/** Every Rust venue name in the beta bundle, sorted. */
export const betaVenueNames: readonly string[];
/** Every family build the beta bundle ships, sorted. */
export const betaVenueFamilies: readonly string[];
/** Thrown for a known Rust venue that the beta bundle leaves out. */
export class BetaVenueUnavailableError extends Error {
  readonly venue: string;
  constructor(venue: string);
}
/** The family of a beta venue; throws for an unknown venue or an OUT one. */
export function betaVenueFamily(venue: string): string;
/** The signer shapes a beta venue accepts, or `undefined` when unrestricted. */
export function betaVenueSigners(venue: string): readonly string[] | undefined;
