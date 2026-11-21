# Hyperliquid write outcomes

Hyperliquid writes follow the legacy TS serial/grouping behavior, except that
plain placements within one command are batched (see Batched placements). The
shared WritePlan records outcomes and has no validation or dispatch authority.

Stop-after-UNKNOWN is a recorded parity exception (Sean/daxx, 2026-11-08;
doc #3355). A genuine Unknown halts further physical writes for that action,
including replacement after an uncertain cancellation. The engine duplicate-write
fence remains authoritative. Unknown never triggers a retry or protective put-back.
Definite refusals retain the TS behavior; an already-terminal perp cancellation
can still proceed to replacement.

Protective put-back kept per Sam's decision, SPLIT_PLAN_INVARIANT.md exception 2
(doc #3355). After a definite full-position replacement refusal, master's existing
policy attempts the captured original protection once with a fresh identity.
Source, fill, position, newer-protection and fresh-mark checks remain unchanged.
The journal records this recovery without controlling dispatch.

Unknown publications retain placed native IDs and indexed Unknown/NotSent rows.
The terminal error, Unknown elements and `order.outcome_unknown` telemetry carry
`The outcome is unknown. Check Open Orders`. The literal is intentionally copied
from the engine under the lead's venue-local wording rule. Telemetry identifies the
command and lists actual placed IDs and unknown indices as comma-separated fields.

Recording tests in `after_failure_parity.rs`, `write_plan_preflight.rs` and
`spot.rs` cite the TS continuation sites and pin this exception, accepted prefixes,
standard wording, journal rows, telemetry and absence of later writes.

Engine-sealed HTTP503 Unknown retains halt, prefix IDs, journal and canonical
message assertions. Telemetry for that path awaits a later engine hook; its
telemetry assertion alone is deferred. Other Unknown pins require telemetry.

## Batched placements

TS placed a PlaceOrders command's rows one signed `order` action at a time, so a
burst of N orders took N paced round trips. Now consecutive plain limit/market rows
on one DEX (core perps, spot, or one HIP-3 DEX) share one `na` action of at most 40
rows, the most that still costs one request of weight. Any row with an attached leg
or a trigger keeps an action of its own, as before, so `normalTpsl` and
`positionTpsl` grouping never contains another command's or row's order.

Each status row settles its own order, in caller order: a refused row is NotSent
and fails only itself, and its siblings stay placed and acknowledged. A reply whose
row count differs from the batch, or that cannot be parsed, proves nothing about
any row, so every row is Unknown. A `waitingForFill` row in a batch has no parent
and is Unknown as well. A failed write still halts the command's later writes, and
nothing is resent. One action takes one nonce, so signing is unchanged.

Separate commands are not batched together: each owns its own engine Plan, and one
physical request cannot settle several owners (`docs/plans/RUST_EXECUTION_GROUPS.md`).

## Confirmed fill reconciliation

A confirmed perp `filled` reply schedules a `clearinghouseState` read for its
DEX through the existing 350 ms coalescer and account-read admission. This also
covers the core DEX, which the periodic HIP-3 poll does not read. A delayed private
stream must not leave a confirmed fill waiting indefinitely for a position update.
The reply never creates a position: only a usable account snapshot or live fill
does. Failed reads preserve state, newer stream evidence wins, and no write is
retried. Resting replies and quiet accounts gain no position polling.
Frame-fed page engines leave reconciliation to their account-frame provider;
they do not arm local position-refresh timers or account reads.

## Cancellation reporting

Decoded failed cancellation batches retain an indexed logical journal beside
the engine's native classification and partial metadata. Confirmed rows report
`applied` with effect `cancel` and their existing target IDs. Unproven rows
report `unknown` with the native diagnostic. Cancel targets remain absent from
created-order IDs; an empty created-ID list does not mean no cancellations applied.

A decoded Unknown row publishes `The outcome is unknown. Check Open Orders`
and one independent `order.outcome_unknown` event correlated by `commandId`.
Native error text remains available in `venueMessage`. The journal and diagnostic
do not authorize dispatch, retries, restoration or store changes.
Decoded cancellation facts settle the command's passive journal before terminal
publication; one correlated event retains the actual Unknown indices and placed IDs.

`tests/support/beta_batch_outcomes.rs` records the real cancel payload, checks
store effects and empty created IDs, and pins journal, terminal guidance and
diagnostics independently with the Unknown row first, middle and last. Confirmed
and already-gone cancellations remain successful without Unknown diagnostics.

## Protective-edit mark freshness

A protective cancel-then-place is vetoed at the cancel's release unless the mark
was observed within five seconds (`FRESH_TICKER_MS`). `fastAssetCtxs` diffs omit an
unchanged mark, so a quiet mark on a streaming symbol used to age out and refuse
the edit (AGT-47). A later row for the symbol omitting `markPx` now confirms the
mark this socket last sent, only while the stored mark still equals it. A new
socket, an unreadable frame, an explicit null or a mark set by any other source
retires that proof; other symbols' rows never supply it.

When the mark would still not be fresh one second after sizing (capped at the
amend bound), the amend reads `activeAssetData` once under the same owner and
bound before anything is canceled. A failed, mismatched or unanswered read refuses
with the order unchanged; a 429 retries within the same bound, as the sizing read does.
A read's mark is discarded when the stream observed the mark after the read was
queued, so a delayed snapshot never replaces a newer mark.

The put-back after a refused replacement never consults the mark: the position is
unprotected until it lands, and the original trigger was already accepted. It is
re-placed as-is even when the mark has already crossed its trigger, and the venue
fires it; it is never converted to a market close (AGT-47, Sam 2026-11-20). Only a
closed position, newer protection or a fill since the cancel stops it.

Rediscovering the market list (a HIP-3 catalogue retry) carries existing ticker
rows without re-dating their marks (AGT-55), so it neither passes a stale mark nor
pre-empts an in-flight `activeAssetData` read.

## Candle catalogue readiness

The first nonempty catalogue opens MARKETS before all HIP-3 DEX and optional spot
rows arrive. A candle read for a missing row waits only while its catalogue is
unresolved; an existing core row can read immediately. A completed relevant
catalogue or explicit exclusion refuses with `Market not found`, even if an
unrelated DEX is still loading. Failed/malformed discovery is not proof of absence.
Optional spot publication is not completion: every universe row must resolve to
a usable market or explicit exclusion. A malformed or missing context keeps the
original read bound; usable siblings publish and explicit exclusions still refuse.
Standalone spot records the same per-row exclusion facts as merged spot, so an
excluded row refuses immediately even while a malformed sibling remains unresolved.

The wait owns its original command Context and timer, capped at 20 seconds and
any shorter caller bound. Discovery and HTTP share that deadline; neither arrival
nor an early host timer renews it. Deferred release runs outside the discovery
HTTP callback and rechecks the original Context. Retired generations cannot send
or complete a reused command ID. Candle time geometry is captured at receipt.

A queued candle read reserves 20 plus one weight per 60 candles its sent span can
return (ceil(span/step)+1; `1M` is a fixed 30-day bucket), matching Hyperliquid's
documented charge. The cap is 6,000 rows (120): a full 1m reply was seen at 5,182.
A flat 104 queued chart-sized reads ~15 s each (AGT-113); the shared host budget
derives the same weight from the request body.

## Stop-entry brackets (AGT-115)

`wires` groups legs (`normalTpsl`) only with a limit or market entry. A stop or take-profit
entry with a non-zero attached leg used to be placed without it; `preflight` now refuses the
whole batch before any write with the engine's shared `stop_entry_legs_refusal` copy.

A market entry dropped its legs too until AGT-117. It is a marketable Gtc limit at the
slippage-capped price, so its legs now group with it exactly as a limit entry's do: market
triggers at the entry's size (reduce-only on perps), each with the entry's `p` (the venue
applies its own 10% slippage when a trigger fires). Verified live 2026-11-21 on a perp: a 220
DOGE market buy with SL and TP answered `[filled, "waitingForTrigger", "waitingForTrigger"]` and
both legs rested as reduce-only triggers of 220. The reply names only the entry, so the result is
that one ID; the hosted receipt counts no separate legs for Hyperliquid, and a refused leg stays
partial evidence.

Spot (the standalone venue and spot pairs in the perp account) has no trigger orders. The venue
refuses a spot bracket, limit or market entry, as one row for the whole group: "Placing trigger
orders is not enabled on spot trading yet." (live 2026-11-21, HYPE/USDC, nothing filled or
resting). `BATCH` classifies it as a refusal, so the command is refused rather than unknown.
