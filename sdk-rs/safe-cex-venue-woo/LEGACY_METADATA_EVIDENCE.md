# Legacy WOO metadata method evidence

Code and initial measured gates: `cfc2f7b20757366cd12c5cdaa8bf1d7ca1d8f5a3`.
Actual parent: `600bbd302bdd09d20f938650e360385e68f55cba`, integration PR 7706.
Fresh branch `feat/rust-woo-legacy-metadata`; no earlier unreviewed feature branch
was used as an ancestor. Backpack is complete and was not edited.

This slice moves **10 pending -> ported, 0 -> divergent**. Python 3.11 tomllib
census of only woo.exchange.ts.toml, woo.ws-public.ts.toml and
woo.ws-private.ts.toml: parent **12 ported / 20 divergent / 69 pending**;
code commit **22 / 20 / 59**, independently expected 101 rows in both.
All ten changed rows belong to woo.exchange.ts; neither socket nor any v3
family row/source was edited. Legacy endpoint URLs do not change file ownership.

The ten native targets in src/legacy_metadata.rs port fetchAccountInfo,
getPositionMode, afterMethodCalledInWorker, createPositionMetadataCacheKey,
buildPositionMetadataFromLeverage, fetchPositionMetadataForSymbol,
updateCachedPositionMetadataLeverage, confirmPositionModeAndRefreshPositions,
setLeverage and changePositionMode. They implement named method bodies over
compatible Host primitives, not a completed exchange class or runtime adapter.
The public fetchPositionMetadata wrapper and full WOO wiring remain pending.

## Measured clean gates

The check, test and extractor check ran at the code SHA above, with no mutation
applied. The writer ran before that commit on the same manifest/source inputs:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2`: **0**, finished dev profile, zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_metadata --jobs 2`: **0**, **7 passed / 0 failed**.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: **0**, output `Updated 0 manifests; 0 added, 0 orphaned symbols` after manual symbol-row updates. The writer was run; no rust_files array was hand-written.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols`.

A development compile failed with a wrong test-helper signature (101), then was
corrected. A development test run failed comparing JSON integer 50000 to floating
50000.0 (101), then was corrected to compare decoded numeric values. The first
writer invocation rejected an unsupported layer label (child output reported 1;
the combined shell also tailed the log, so its writer exit was not measured
separately); corrected to adapted.
These are not passing gates and are not mutation proofs.

## Native assertions and finite source floor

The oracle extracts and transpiles the exact ten class members plus the actual
response-message member and actual constants/classifiers from woo.exchange.ts.
It loads actual enums from types.ts and endpoints from woo.types.ts. Fixtures
supply transport, clock, reverse-symbol and sleep primitives. No credentialed
HTTP or sockets are used.

Independent Rust expectations pin **42 rows**, group names, iteration numbers,
and fixture content. The group floor is account 8 (stopped at 7), mode 3 (2),
worker 5 (4), build 6 (5), cache 8 (7), update 2 (1), confirm 4 (3), write 6 (5).
Global source enumeration stopped at **41**. This is a finite floor, not an
exhaustive account/cache/error/scheduling census.

Before comparing output projections, native assertions inspect UTF16 symbols
and keys, PositionSide/PositionMode/MarginType, f64 bits (negative zero, NaN,
hexadecimal and rounded large parseInt), row order, array/object identity,
request fields, error identity, cache/backoff state and retry effects. There is
no decode-reencode or roundtrip oracle. Native supplements inspect live Map
insertion during Date.now and actual shared-wrapper wake fanout after the last
polled caller drops. An independently driven shared request commits after its
caller drops; Host scheduling must begin after the host RefCell borrow ends.

The source's synchronous GET-throw finally-before-inFlight.set behavior is
intentionally preserved: the stale in-flight entry remains. Cached mode is
captured eagerly before later worker updates. Cache-commit clock errors enter
the same rate-limit fallback catch as request errors; NaN backoff uses JS ||0.

## Review finding and correction

Read-only sub-agent review found the original custom shared future could strand
surviving readers, a cached-mode poll-time snapshot mismatch, cache-commit clock
errors bypassing fallback, and NaN not using ||0. All were corrected before the
code SHA. The shared implementation now uses futures-util's shared future;
no shared SDK type/trait was changed.

**Survivor finding:** changing only cache case 7's NaN seed to Infinity originally
produced byte-identical 42-row oracle output (reviewer Node exit 0). A first input
descriptor derived from the case index also survived that control. The corrected
descriptor reads the actual seeded map before calling the method, while Rust's
expectation independently pins NaN bits. Reviewer recheck exited 0 and observed
NaN `7ff8000000000000` versus Infinity `7ff0000000000000`. The runtime CONTENT
control below confirms the test now rejects this substitution.

## Measured discrimination controls

All controls start from the code SHA and restore the source/oracle byte-for-byte.
The same scoped test command is used, optionally filtered to the named test.

| Control | Runtime failure and measured exit | Restoration |
| --- | --- | --- |
| Strict-success condition inverted; worker truthiness inverted; TTL 30000 -> 29999; cache-update leverage +1; read attempts 3 -> 2, batched with a POST-field mutation | **101**, 6 named tests failed at `METADATA native account error 3`, `METADATA native worker truthiness 0`, `METADATA native cache alias 1`, `METADATA native leverage`, and `METADATA native confirmed mode attempt 2` | **0**, 7/7 |
| Cached mode snapshot -> opposite mode; hedged short -> long; mode POST HEDGE_MODE -> ONE_WAY, grouped | **101**, 5 named tests failed, including `METADATA native cached mode snapshot 2`, `METADATA native side at 1`, and decoded mode body units in `leverage_and_mode_writes_native_fields_errors_and_refresh` | **0**, 7/7 |
| Key colon -> semicolon | **101**, `METADATA native key` in worker_key_and_metadata_native_values | **0**, 1/1, 6 filtered |
| Oracle NaN backoff seed -> Infinity | **101**, `METADATA corpus CONTENT input 29` in cache_ttl_rate_fallback_and_sync_throw | **0**, 1/1, 6 filtered |
| Oracle output shortened to 41 rows | **101**, `METADATA corpus SIZE` in cache_ttl_rate_fallback_and_sync_throw | **0**, 1/1, 6 filtered |

The isolated native POST leverage +1 control failed with **101** at
`METADATA native POST leverage` in
leverage_and_mode_writes_native_fields_errors_and_refresh. Byte-identical
restoration passed **0**, 1/1 with 6 filtered.

The six-test batch's POST-field mutation was masked by the earlier cache-update
leverage assertion in the same test. It is not independently credited by that
batch; the isolated POST control is recorded separately below.

A homogeneous 42-row output (42 copies of the first row) also failed with
**101** at `METADATA corpus CONTENT iteration 1`, then restoration passed **0**,
1/1 with 6 filtered. Counts and content expectations come from Rust fixture
constants, not from output lengths or case-index labels supplied by the oracle.

The first attempt at a six-family batch replaced strict success checking with
`false` and failed compilation because success became unused (101). That attempt
is excluded from discrimination proof; a compiling inversion is used instead.

Discrimination floor: **10 distinct native mutations and 3 corpus mutations**,
stopped at distinct control **12** (zero-based), plus one excluded compiler-only
attempt and the disclosed masking check. All credited runtime controls were
rejected; no credited surviving mutation remains. Restoration SHA256 values:

- src/legacy_metadata.rs: `e1339a662c01ce8e2bc0aff0597c82678e700d918ec08cfe81f2c85dc9186a63`
- tests/legacy_metadata_oracle.cjs: `e94eb0788ddeb3de745d2a958877e0f6cc664959c7329f8e7d50dc30f4d28a0b`

No workspace compilation, full crate test suite, host clippy, wasm clippy,
credentialed demo, production exchange, v3-family test suite, or CI rerun was
performed. Scoped check/test results must not be reported as those claims.

## Integration merge preservation

Merged integration `eb5339fff8f489456695778fe2af9701f8c18b69` down, producing
merge SHA `249e2eb91b2e9de85e63b172ae7546ae43af96ba`. No conflict occurred.
At that merge SHA, measured `git diff --exit-code` of the code SHA against HEAD
for this crate's src/tests returned **0**. The expanded input diff also returned
**0**: all eight reachable internal packages, root Cargo.toml, this crate's
Cargo.toml, actual TypeScript oracle inputs and WOO manifests are byte-unchanged.
The manually examined dependency closure has **481 reachable Cargo.lock package
records**, equal before/after. Changes elsewhere in Cargo.lock do not change
this closure. The two measured scoped Cargo exits remain attached to the code
SHA; Cargo was not rerun at the merge SHA because these inputs did not change.

At the merge SHA, `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js
parity:extract:check` returned **0** and output `Would update 0 manifests; 0 added,
0 orphaned symbols`. Tomllib remeasurement independently asserted 101 owned
legacy rows and exactly ten pending->ported transitions: **22 / 20 / 59**.
This evidence-only follow-up changes no gate input.
