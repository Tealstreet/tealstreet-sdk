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
