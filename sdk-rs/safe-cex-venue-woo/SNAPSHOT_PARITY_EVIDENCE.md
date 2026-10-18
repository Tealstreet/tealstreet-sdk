> Historical evidence: the test-only V3 comparison/canonical bodies, operation holders and their oracle commands are retired. The live legacy operation shape is retained; see CLAUDE.md.

# WOO native snapshot comparison

Eight public-parity bodies now use native algorithms over retained host references.
Parent `ee644266687ce3c8d459df9a024d21dfed31fba0` carries unmerged PRs 7858,
7862 and 7867; integration base is `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
These ancestors have local sub-agent reviews and are not integration-only ancestry.

The new bodies own comparison, native V3 candle mapping, validation and ordered
result construction. Opaque capabilities supply language operations and live
iteration, including exact-width padded destructuring with iterator closure.
Initialization follows market fields, ticker map, interval freeze, candle map.
Existing typed adapters and their divergence annotations remain unchanged.

Eight new native assertion groups cover independent literal fields, provenance,
counts, canonical text, NaN/signed zero, duplicate/key order, request blockers,
source addition, live aliases, and exact objects passed to freeze. These run
before 24 maintained-source cases. Source-file case enumeration stops at the
blocked candle comparison. This finite ordinary-record/ASCII fixture does not
prove descriptor enforcement or every exotic host realm/accessor/iterator.
Freeze assertions prove invocation and exact argument identity only.

Sub-agent review caught module initialization order and missing native freeze
proofs; both were corrected before gates. The measured receipt follows.

Canonical declarations are cited in
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Measured receipt

Gate SHA `23f990cbadfd024bf5b44401002d7f4dd335a425`.
Independent tomllib: 214 ported / 308 divergent / 10 pending, 532 rows in
21 manifests, stopping at woo.ws-public.ts.toml. Eight pending→ported against
the stated parent, 28 cumulatively against integration.

Measured exits at that SHA:

- Scoped WOO check, all targets / jobs 2: 0.
- Focused public_canonical target, jobs 2 / one test thread: 0, 18 passed.
- Extractor check: 0, 7341 symbols / 512 files, zero updates/additions/orphans.
- WOO-only CI wiring tests: 0, two passed.

Initial SHA `57c7eee797` passed scoped check (0), but the focused target exited
101 with 17 passed / one failed: the candle literal expectation placed value
before missing-key despite original legacy Map order 3 then 1. The expectation
was corrected against source iteration; production was unchanged. That failure
was not a deliberate mutation or claimed sensitivity proof. Extractor and wiring
were not run at that initial SHA.

Eight isolated compiling controls each exited 101 before source capture:

- unsupported interval path: snapshot_initialization_and_unsupported_interval_native;
- validation detail: kline_context_blockers_and_addition_are_native;
- mapped timestamp divisor: v3_rows_native_mapping_canonical_and_refusals;
- inverted side equality: orderbook_side_native_values_missing_levels_and_same_value;
- book route literal: orderbook_comparison_native_status_canonical_and_aliases;
- market route literal: market_comparison_native_duplicate_field_and_key_order;
- ticker route literal: ticker_comparison_native_all_nine_value_fields_and_blocker;
- candle authoritative handle: candle_comparison_native_match_and_blocked_fallback.

Each edit was restored before the next. The complete restored focused target
then exited 0, all 18 passed. A separate blanket freeze omission also compiled
and failed the native result freeze assertion; it does not prove every nested
site independently. Six further isolated omissions each compiled and exited 101
at `SNAPSHOT-NATIVE required freeze` naming result, blockers array, mismatches
array, mismatch, provenance, or unsupported interval blocker respectively.
Their final byte-exact restoration again passed all 18, exit 0. No mutation
survived this finite set; other untried edits are not covered by that statement.

Local adversarial sub-agent review independently checked census, mapping targets,
exemption preservation, native-before-source assertion placement, logs and
portability, with no remaining concrete blocker. No Cargo was run by reviewer.
The eight controls establish these specific sensitivities, not universal coverage.

Not run: workspace compilation, full venue suite, shadow_compare rerun, host/wasm
clippy, wasm build, live exchange tests, exhaustive prototype/realm cases.
