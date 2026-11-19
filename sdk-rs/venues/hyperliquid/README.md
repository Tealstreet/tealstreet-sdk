# Hyperliquid write outcomes

Hyperliquid writes follow the legacy TS serial/grouping behavior. The shared
WritePlan records outcomes and has no validation or dispatch authority.

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

The put-back after a refused replacement never waits on freshness: the position is
unprotected until it lands, and the original trigger was already accepted. A stale
mark is re-read once (1.5 s bound, plan held); if no fresh mark results, the original
is re-placed anyway and the Restored outcome says no fresh mark was available. A
fresh mark still refuses a put-back that would fire immediately.

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
