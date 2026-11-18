# BingX Spot qualification

The `bingx` suite requires four distinct cases: its existing perpetual public and
private tapes, plus `bingxspot-public.jsonl.gz` and `bingxspot-private.jsonl`.
The Spot public case qualifies CPU; the private case qualifies correctness.
Neither a perpetual pass nor a missing Spot case can qualify the complete suite.

`bingxspot-fixtures.mjs` builds offline fixtures from the documented Spot examples
in `sdk-rs/venues/bingx/tests/fixtures/spot`. It does not fetch from a venue.
The public tape adds synthetic Spot ticker and full-depth updates with one GZIP
frame. The private sequence adds listen-key creation, limit placement, duplicate
partial execution reports, confirmed cancellation, market BUY quote sizing,
market SELL base sizing, historical fills, candles, REST depth and a balance
notification. Its compressed execution report carries raw order and execution
integers above JavaScript's safe range. All credentials and session keys are
public fixture values; no live account or production capture is represented.

The TS replay adapter calls `BingxSpotReference`, the independent Spot oracle in
`packages/safe-cex/src/__tests__/reference/bingxspot`. Its streaming adapter maps
the documented Spot fields independently. No perpetual adapter, perpetual tape,
candidate-derived expectation or output fixer supplies Spot results. Protocol
checks separately validate signatures, request economics and Spot subscriptions.

The nondefault WASM feature `bingx-spot-replay` exports
`bingxSpotReplayFixture(publicDataOnly)`. It constructs the real native Spot
engine with fixed fixture credentials; it accepts no key, endpoint or venue.
Normal TS/WASM/native factories and visibility metadata remain closed. CI checks
the shipped family's size and absence of the fixture export before building and
archiving a separate fixture module for correctness replay. The measurement job
also checks shipped size before its fixture rebuild and seals the actual module
used for its Spot and perpetual cases. No size cap or admission list changes.

Generate inputs and build the qualification module:

```sh
node sdk-rs/harness/bingxspot-fixtures.mjs --public
yarn rust:private-tape-bingxspot
yarn rust:build-wasm --venue bingx --bingx-spot-replay
```

CPU qualification comes only from the original Actions attempt 1: inspect the
Spot public case and its disposition in `plan.json` from `cpu-plan`. A job badge,
a rerun or local correctness-only timing is not CPU certification. The existing
raw objectives, fixed pair count and one in-job remeasurement are unchanged.
