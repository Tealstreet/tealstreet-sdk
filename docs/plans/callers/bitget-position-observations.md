# Bitget position frames are per-identity observations

Bitget Classic and UTA position WS `snapshot` and `update` frames publish valid
individual rows. A symbol or hedge leg omitted from a frame, including `data: []`,
is not proof that its position closed. A WS fragment does not establish the
positions-loaded flag or complete a `readPositions` command.

SDK, NX and server callers keep displaying cached exposure until a valid explicit
zero-size row identifies its closure, or a complete clean REST position census
establishes its absence under the existing row-admission and stream guards. A
filled or canceled order, or a partial execution, does not itself close a whole
position. Fill execution identity remains separate from position state: neither
these WS observations nor a REST read is a formal execution-inclusion barrier.

The shared handler no longer records whole-WS-snapshot membership, omission
tombstones or a five-second whole-set fence that could hide a REST-only peer.
WS mode coherence accounts for retained peers. A partial frame cannot replace
the account's position mode while incompatible cached exposure remains; REST
reconciles that transition. Existing per-row guards, 500 ms coalesced REST
repair, busy/rate-limit retry and two-second open-position polling are retained.
No caller API, order-read cover or uncertain-order release changes.

The policy covers the shared Classic/UTA handler. Production-binding regressions
exercise Classic production/demo and UTA production. Existing UTA-testnet and
inverse refusal remains in force; this change does not activate those routes.
No additional deletion prerequisite or caller migration is introduced.
