# GRVT order sizing

New orders follow the legacy TypeScript formatter: round the requested amount
to the market step and send one order containing one leg, including when the
rounded amount exceeds the advertised maximum. The venue's refusal is returned
to the caller. `KNOWN-ISSUE: TS sends above-max; venue refuses`.

The reference is
`packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts:1415`
(rounding) and `:1556` (single payload). Replacement preflight still checks the
maximum, matching `:2007`; protective put-back retains its existing validation.
The six above-max cases and native refusal propagation are pinned in
`tests/trading.rs::legacy_ts_above_max_create_is_one_payload_and_surfaces_venue_refusal`.

New-create failures include an indexed journal of native creates (parent and
bracket children), retaining acknowledged client IDs and marking the unissued
suffix `notSent`. An ambiguous reply returns
`We couldn’t confirm this change. Check your open orders before trying again.`, retains the diagnostic and client
reconciliation identity in `venueMessage`, and emits `order.outcome_unknown`.
Dispatch is unchanged: a definite refusal stops the serial batch, as in
`grvt.exchange.ts:1608-1612` and `exchanges/base.ts:1111-1114`. The TS formatter
accepts a reply without identity at `grvt.exchange.ts:1622-1643`; Rust's existing
stop on that Unknown follows `docs/plans/SPLIT_PLAN_INVARIANT.md` exception 1.
Amendment and protective put-back paths are unchanged (exception 2).

# Unknown outcome telemetry

GRVT registers the engine Unknown hook and emits the same `order.outcome_unknown`
event at terminal native trading failures. Trade requests currently use the legacy
request API, which has no managed delivery and cannot invoke the engine hook.
Managed engine sealing and native decoding therefore each emit once, without
changing requests, retry decisions, reconciliation text or completion outcomes.
Venue and account attribution comes from the host context.

The NX #9262 projection contains only `command` (`command.<decimal ID>`), literal
`outcome: "unknown"`, optional `placedOrderIds` and `unknownIndices`, plus decimal
`request` and optional numeric `status` for engine-sealed failures. Request IDs
match `^[0-9]{1,20}$`; status is omitted unless it is in 100..=599. Lists retain at
most 16 entries; placed IDs also retain only a prefix of whole entries fitting
the engine’s 512-byte string-field limit. Order IDs must be 1..=32 decimal digits
and indices must fit u16.
Invalid lists are omitted as a whole. Native placement IDs are GRVT's existing
command-result client identities; native failures have no element journal and
emit an empty index list. The full caller-visible evidence is unchanged. URLs,
endpoints, queries, bodies, signed payloads and native error text are excluded.
