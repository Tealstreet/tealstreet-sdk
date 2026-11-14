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
`The outcome is unknown. Check Open Orders`, retains the diagnostic and client
reconciliation identity in `venueMessage`, and emits `order.outcome_unknown`.
Dispatch is unchanged: a definite refusal stops the serial batch, as in
`grvt.exchange.ts:1608-1612` and `exchanges/base.ts:1111-1114`. The TS formatter
accepts a reply without identity at `grvt.exchange.ts:1622-1643`; Rust's existing
stop on that Unknown follows `docs/plans/SPLIT_PLAN_INVARIANT.md` exception 1.
Amendment and protective put-back paths are unchanged (exception 2).
