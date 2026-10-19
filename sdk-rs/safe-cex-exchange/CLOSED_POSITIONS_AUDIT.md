# Closed-position resolver timing audit

The mapped `resolve_closed_positions` entry now calls its selected venue or
derivation callback before returning. `ClosedPositionSource` requires the host
runtime and owns shareable static callbacks so the canonical `start_eager`
driver retains the pending operation after its waiter and original source drop.
The callback future uses that same driver as the source fetch Promise; awaiting
it retains a microtask boundary even for a ready result. No method-local task
registry, value serialization or alternate Promise resolver is introduced.

## Executed evidence

On unchanged master `eacf4f633e`, the two `closed_position_timing` tests compiled
and failed: neither selected fetch ran until polling, and abandoning the waiter
never started its held fetch. The repaired target invokes the actual mapped
utility with `NativeRuntime` and compares six profiles with the maintained
`packages/safe-cex/src/utils/resolveClosedPositions.ts` executed in Node:
venue, derived, independent derivation, held fetch, rejected fetch and unavailable.
The source probe checks options identity; native checks the forwarded typed
options. Dropping a held waiter and its source still reaches fetch completion.

`remaining_utils::native_closed_positions_provenance_independence_and_one_fetch`
retains its entry provenance, independence defaults/override, selected callback,
no duplicate fetch, unavailable reason and rejection checks. Its only fixture
migration supplies the required runtime and drives its Promise scheduler.

## Boundary

This is the mapped exported native utility and its supplied fetch capability.
There are no production Rust assembly callers of this helper at this baseline;
the tests do not claim a new venue or browser assembly path. The existing typed
ClosedPnlEntry and HistoricalTradeOptions, owned result collections and
ExchangeError callback ABI remain. Arbitrary JS property replacement, options
identity and returned object aliases are outside that typed surface. The separate
ClosedPositionsResult surrogate/identity finding is not discharged here.
