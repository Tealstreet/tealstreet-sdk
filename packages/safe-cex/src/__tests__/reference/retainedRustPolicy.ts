import type * as BetaPolicy from '../../exchanges/rust/betaVenues';

// Historical adapter/provider assertions run below the separate beta boundary.
// Unmocked factory/worker tests enforce real beta admission; this is no activation.
export const retainedRustPolicy = (policy: typeof BetaPolicy): typeof BetaPolicy => ({
  ...policy,
  assertBetaAccount: () => {},
});
