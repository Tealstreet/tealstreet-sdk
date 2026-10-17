> Historical evidence: the test-only V3 comparison/canonical bodies, operation holders and their oracle commands are retired. The live legacy operation shape is retained; see CLAUDE.md.

# WOO public canonical helpers

Base: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.

Eight rows move from pending to ported: the two request contexts,
canonicalString, canonicalMarket, canonicalizeWooV3Markets,
canonicalLegacyTickerStats, canonicalV3TickerStats, and
canonicalizeWooLegacyTickerStats. Existing symbol identities are preserved.

The opaque native operations provide language primitives; the Rust bodies own
field projection, argument evaluation, mapper construction, copying and method
invocation. Tests use a finite ordinary-record host with BTC/ETH ASCII ordering.
They establish a floor of five native assertion groups and five source cases,
stopping at canonicalizeWooLegacyTickerStats. They do not establish exhaustive
host conformance or exotic accessor, iterator, locale or prototype coverage.

All projected fields and their order have independent native assertions before
source observations. The immutable contexts retain distinct UTF-16 code units.
The source witness compiles the maintained TypeScript, checks the context type
syntax and invokes the actual helpers. The focused Cargo target is included in
the existing mandatory WOO CI oracle command.

Sub-agent review found missing pre-source native field assertions; these were
added. An unrelated package.json duplicate-key cleanup was excluded.

Applicable declarations remain in
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Gates and mutation controls are not yet measured for this initial code commit.

## Measured gate receipt

Code/gate SHA: `30f4aee83455b6588a07b8ce65d380d68b06394a`.
Parent integration: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
No unreviewed feature ancestor. `tomllib` census: WOO 194 ported /
308 divergent / 30 pending, exactly eight pending→ported from this parent.
Kucoin remains 146 / 45 / 0; this WOO slice makes no Kucoin closure claim.

Measured exits:

- Scoped `cargo check -p safe-cex-venue-woo --all-targets --jobs 2`: 0.
- Focused `cargo test --locked -p safe-cex-venue-woo --test public_canonical
  --jobs 2 -- --test-threads=1`: 0; five passed.
- `parity:extract:check`: 0, output says would update 0 / add 0 / orphan 0.
  Repeated after exact restoration of the mutation: 0 with the same counts.
- WOO-only `rust-ci-wiring.test.mjs`: 0; two passed, including existing
  removal/masking controls.

One compiling five-family mutation batch exited 101 with all five named native
assertions failing: amount precision, sorted result, undefined stringify result,
kline symbol, and V3 fundingRate. Mutations respectively substituted price for
amount, reversed comparator operands, wrapped the stringify argument in an
array, exchanged context fields, and substituted fundingTime for fundingRate.
Exact source-byte restoration followed by the focused target exited 0; five
passed. No survivor in this bounded batch; this is not an exhaustive mutation
census.

The receipt commit changes this document only. Production, tests, scripts,
manifest and CI inputs remain byte-identical to the measured SHA. No workspace
build, full venue suite, clippy, wasm, network test or live venue was run.
