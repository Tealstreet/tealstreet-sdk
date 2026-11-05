# Bitget position receive order

Classic and UTA WS position rows remain per-identity observations, including
explicit zero-size closures. Neither a snapshot omission nor an empty fragment
closes an identity or establishes positions readiness. Bitget says receive order
matches commit order per connection and leg "in practice"; this is not a formal
sequence, loss-detection or fill-coverage contract. Native `uTime` and gateway
`ts` are display metadata, never ordering evidence between WS and REST.

Each valid observation records its connection epoch, local receive ordinal and
local receipt time, keyed by symbol and side. A zero retains the same marker
after the Store row disappears. Each marker also retains its observed Store
row ID: a stale cached row from another mode cannot borrow that marker to block
an unobserved hedge peer, replace the current row or resurrect a zero. Confirmed
setting writes may update that row in place without severing its receipt guard.
An admitted new-mode REST replacement rebinds the exact Store ID without changing
its receipt time, epoch or ordinal. Malformed REST fallback cannot retain a
superseded cached mode alias or turn it into that replacement.
Malformed observations preserve prior financial evidence (including zero markers
without a Store row) and only hold the
affected identity across an older read. Hedge peers remain independently eligible
for REST healing. A conflicting one-way side or
position mode cannot coexist with a protected current row.

Every REST positions census records its first physical signing-release cut,
through the existing `on_signing_release` hook. Preparation waits do not count as
send time. Inverse collateral successors preserve that earliest cut. A WS
observation after the cut wins for its identity even when native timestamps
disagree or the local grace expires before the response. Reconnection ends the
epoch, clears old WS ordering evidence, and requires a genuinely new released
census; an older reply neither publishes nor settles `readPositions`.

REST has no read-your-writes guarantee. The existing five-second grace is kept
as a bounded heuristic using local receipt time only. A recent pre-send WS row
or zero also wins over conflicting REST. Its age is fixed at the census's first
physical release, so a slow response cannot expire that read's protection.
Matching pre-send financial state can settle immediately. Post-send WS evidence
still holds the read even if values
agree, or an empty census visits no row after an explicit zero. The marker census
keeps that debt independent of current Store rows; only a genuinely later census
can settle it. Once the grace passes without a newer WS observation, a fresh read
may heal it. This cannot establish that a fill was included in a position row.

A conflicting ordinary census owes one coalesced 500 ms repair through the same
positions admission. All held identities in that cut share it. A repair cannot
recursively create another 500 ms repair; unchanged WS-driven repair, busy/pause
retry, two-second open-position polling and `readPositions` retry still apply.
The setting-write fence, complete-clean-REST absence path and malformed-row
isolation remain in force. No order or partial fill implies position closure.

Production controls cover stale REST rows and omissions, zero tombstones,
same-clock receive order, delayed physical release, reconnect, hedge-peer
healing, one-way coherence, agreeing reads, bounded repair and inverse collateral
cuts. Six reviewer probes pin exact-ID rebinding, malformed-row authority and
post-cut zero/omission debt. Additional controls pin original receipt age after
rebinding, malformed REST fallback after a zero, and a zero marker's later
malformed-WS barrier. Coverage includes Classic production/demo and UTA production. Existing
UTA testnet/inverse refusal is unchanged. The account test that formerly used a
larger native timestamp to heal recent WS state deliberately now waits for the
local grace and a fresh read; all financial and malformed-row assertions remain.

No engine, SDK bridge, credential, request inventory or hosted-signing change.
