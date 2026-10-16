# Polymarket remaining runtime rows

**16 pending → ported, 0 → divergent.** Tomllib: **107/18/16 → 123/18/0**
(ported/divergent/pending). Independent floor: seven manifests and 141 rows,
stopped at `polymarket.ws-public.ts.toml::PolymarketPublicWebSocket#handleOrderBookUpdate`.
This is a symbol census, separate from the bounded behavior witnesses below.
Exact changed symbols and real target mappings: [census.json](evidence/completion/census-5798132876.json).

Gate input SHA: **`5798132876e0abce62a25b4ebdc049d3d9b71e92`**.
Compiled-control input SHA: **`251c53f07e58c2f4410ce4573e1ad131df912d6e`**. Original integration
parent: `c187d0e075`; merged integration `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`
through `4f83d58d03d3eb28132976db2c83934678e3fa3d`; latest integration
`1a43f3a8f25b7e5fe1c238794e79bad11e4c4788` merged through the gate-input SHA. No unpublished foreign
branch was used as an ancestor. Evidence is a later commit; these exits are
measured at the named input SHA, not relabeled as that evidence commit.

## Scope

Eight exchange rows cover the evaluated class, constructor, markets, tickers,
placement, replacement, history reads and history polling. Eight further rows
cover the API factory/class/credential field, both socket classes, the private
credential field and public price/book handlers. TypeScript remains the
maintained reference.

The new exchange bodies retain prediction-market behavior: pUSD collateral, paired outcome-token symbols, bounded binary order prices, unit contract size and 1x leverage limits, original Gamma metadata aliases, and negative-risk signing metadata. Replacement orders cancel first and retain explicit undefined overrides. History polling retains the source duplicate filter, queued-timeout cleanup and in-flight callback behavior.

The new API factory owns request classification, per-client rate state, authentication callback invocation, structured/raw/undefined signing views, and both response/error interceptors. The class ports retain evaluated class identity, inherited prototypes, declaration/arrow ordering, independent collection fields, original callback capabilities and late receiver reads. Public socket price and book handlers execute native Rust bodies; book callbacks share the original book capability.

Imported SDK/base/HTTP/retry/query-string/throttle implementations and language intrinsics are explicit executable dependencies. The new source oracles use recording bindings for those services; they prove the owned body and delegated arguments, not those kernels or a complete JavaScript VM. Selected existing market/account/spread bodies execute through the composed class bindings. Other existing method bodies remain separately mapped; their full composed path coverage is not claimed here.

Native checks use distinct price/size, bid/ask and cumulative book values, native numeric bits, original reference identity, initial constructor values and callable property capabilities before wire projections. The fixture floors and compiled controls are finite observations.

Canonical declarations are linked without copying them:
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
The 18 pre-existing divergent rows retain their own reasons; this slice moves
only the 16 pending rows.

## Measured gates

Commands, exits, raw output hashes and redacted logs:
[final gate sheet](evidence/completion/final-5798132876/gates.json).
Cargo commands are serial. Scoped check/test/clippy/build and scoped indexer
commands use `--locked --jobs 1`; fmt and repository parity-reader wrappers retain
the commands shown in the sheet. Host and wasm clippy
inherit the strict workspace lints; wasm commands cover the library target.

| Gate | Exit |
| --- | --- |
| source-oracle | 0 |
| completion-source-oracle | 0 |
| check | 0 |
| tests | 0 |
| fmt | 0 |
| host-clippy | 0 |
| wasm-clippy | 0 |
| wasm-build | 0 |
| targets | 0 |
| writer | 0 |
| extractor | 0 |
| parity-check | 1 |
| parity-report | 1 |
| parity-progress | 1 |

The expected 138 venue tests (119 prior + 19 new) passed, including ignored
cases; enumeration stopped after the `ws_timer_clear` binary. Doc-tests were
not separately run. The two new actual-source oracles have independent
floors 128 (stopped `listen/15`) and 64 (stopped `book/7`). Venue invention/status
checks resolve every off-pending row against the real indexer with empty
exemptions; the independently expected row floor is 141. Writer and extractor
output explicitly confirm zero updates, additions and orphaned symbols.

**Retained external failure:** all three repository-wide parity readers stop
at `safe-cex-webworker/src/chaser_manager.rs::thread_local!`; they provide no
repository-wide parity results. The [isolated reproduction](evidence/completion/external-finding-5798132876.json)
exits 1 with only webworker indexed, and verifies source bytes equal the
integration blob. These are failed gates, not green results or exemptions.

The two new source oracles also passed from a source copy with no ancestor
`node_modules`, using only the explicit external TypeScript compiler and Node
builtins: [isolation](evidence/completion/isolation.json). That measurement
belongs to `28b8831f6ba33fa4736e9dd4e1336da84bb6f32f`;
[input equivalence](evidence/completion/input-equivalence.json) verifies the
same source/oracle hashes at 251c. The subsequent
[merge input comparison](evidence/completion/merge-input-equivalence.json)
verifies all SDK/safe-cex inputs, Rust CI workflow and parity package commands
unchanged through the current gate-input SHA. The full legacy oracle
dependency corpus was not re-isolated.

## Discrimination

[Compiled-control receipts and logs](evidence/completion/controls-251c53f07e/summary.json)
measure baseline 19 tests exit 0, then 17 Rust production faults and one CJS
source-guard meta-control through the compiled Rust test. Every declared
mutation exits 101 at its named assertion; every restored test exits 0.
Floor 18 controls, stopped at `source-control-noop`. No survivor, compile-only
failure or wrong named assertion occurred in this declared floor. The meta-control
is an oracle-execution check, not a decoded-native-field fault.

| Control | Exact test | Assertion | Baseline / mutation / restored |
| --- | --- | --- | --- |
| class-maps | `class_native` | `native independent maps` | 0 / 101 / 0 |
| constructor-default | `constructor_native` | `constructor default EOA` | 0 / 101 / 0 |
| market-collateral | `markets_native` | `prediction collateral` | 0 / 101 / 0 |
| ticker-bid | `tickers_native` | `native bits bid` | 0 / 101 / 0 |
| price-axis | `place_native` | `price tick axis` | 0 / 101 / 0 |
| amount-axis | `place_native` | `amount decimal axis` | 0 / 101 / 0 |
| replacement-presence | `update_native` | `explicit undefined overrides amount` | 0 / 101 / 0 |
| history-time | `ohlcv_native` | `candle milliseconds` | 0 / 101 / 0 |
| poll-duplicates | `listen_native` | `strictly newer candle emissions` | 0 / 101 / 0 |
| factory-rate | `completion::factory_native` | `native shared client rate limiter` | 0 / 101 / 0 |
| api-class-prototype | `completion::api_class_native` | `native independent API prototype floor` | 0 / 101 / 0 |
| api-credential-reference | `completion::api_credentials_native` | `native API callback property reference retains function alias` | 0 / 101 / 0 |
| public-class-policy | `completion::public_class_native` | `native socket offline recovery policy` | 0 / 101 / 0 |
| private-class-policy | `completion::private_class_native` | `native socket offline recovery policy` | 0 / 101 / 0 |
| private-credential-reference | `completion::private_credentials_native` | `native private callback property reference retains function alias` | 0 / 101 / 0 |
| price-field-axis | `completion::price_native` | `native best bid` | 0 / 101 / 0 |
| book-shared-alias | `completion::book_native` | `native shared order book callback alias` | 0 / 101 / 0 |
| source-control-noop | `source_oracle_guard_rejects_price_mutation` | `source guard must reject production source mutation` | 0 / 101 / 0 |

The [standalone source control](evidence/completion/source-control.json) at
`28b8831f6ba33fa4736e9dd4e1336da84bb6f32f` changes the actual source price
axis in memory: exit 1 at `source valid limit order succeeds`, restored exit 0
with 128 fixtures. It is not relabeled as a measurement at 251c.

## Review and provenance

The earlier static review rejected `4f83d58d03`: repeated class evaluation,
inert prototype witnesses and constructor checks after overwriting initial
values left proof gaps. `0cb6a3eada` and `2fb566a27c` added evaluated shared
class handles, executable prototype/getter/composed-call witnesses and native
initial-state checks before late writes. The [28b review](evidence/completion/review-28b8831f6b.json)
returned MERGE within the declared dependency boundary, with stale live notes
as a nonblocking finding. `858134e102` reconciles those notes and fixes the two
invalid field-syntax targets to existing construction/callable-handle symbols.
No accessor or placeholder was created for the target gate. The [251c review](evidence/completion/review-251c53f07e.json)
returned MERGE on the corrected code/mappings and control receipts; that review
did not execute gates or verify the final wasm/parity results. The pre-merge
[evidence review](evidence/completion/review-final-evidence.json) verifies the
recorded final exits and the corrected limit/command wording by reading logs;
the sub-agent executed no gates or controls.

[Merge step inventory](evidence/completion/merge-gate-counts.json): Rust workflow
named step declarations 92 → 92; all tracked workflows 352 → 373. No prior
per-file step name was dropped. Enumeration stopped at
`vercel-deploy.yml::Select committed deployments`. CI wiring was inspected:
its existing `cargo test --locked` job includes this workspace member and
installs TypeScript; the 19 new tests are not ignored. CI execution is not
claimed.

The [latest merge inventory](evidence/completion/merge-down-5798132876.json)
retains a workflow-count reduction finding: all tracked workflow named-step
declarations 373 → 352, with dropped names in the two duplicate mirror
workflows and scheduled-tests.yaml. Rust remains 92 → 92 with identical
per-file names. Incoming integration PR7853 moves mirror suites to public
runners. This reduction is retained as an integration CI finding, rather than
claiming global gate preservation from a green scoped build. The incoming
[static coverage guard](evidence/completion/incoming-ci-coverage.json) exits 0;
it checks configured exclusions, not execution of mirror suites or CI jobs.
No upstream CI change was rewritten by this venue slice. The
[post-merge sub-agent review](evidence/completion/review-post-merge.json)
returns MERGE within the declared dependency boundary after reading the new
code/input comparison, gate logs and CI reduction disclosure. No gates or
controls were executed by the reviewer.

The [pre-merge 251c gate sheet](evidence/completion/final-251c53f07e/gates.json)
remains attached to its original SHA; it is not relabeled as the merge run.
The compiled-control and isolated-source measurements likewise retain their
original SHAs and the input-equivalence links.

Earlier measurements remain separate: [4f83 partial run](evidence/completion/initial-4f83d58d03/gates.json)
recorded five exits 0, then log writing hit ENOSPC during wasm-clippy. Its wasm
command exit was not recorded; later gates were not run. [28b full run](evidence/completion/initial-28b8831f6b/gates.json)
retains the two invalid credential field targets and three global-reader
failures. These records do not substitute for the final input-SHA gate sheet.
Logs redact local paths; final gate receipts retain hashes of raw output.

Not run in the earlier scoped batch: workspace builds/tests, separate doc-tests, wasm tests, live endpoints,
real ECDSA/V2-builder conformance, comprehensive library-kernel or JavaScript VM
conformance, complete concrete SDK registry integration, or CI reruns.

## Requested workspace follow-up

The [follow-up receipts](evidence/completion/followup-08ba408bf1/gates.json) measure
`08ba408bf178b9257297f5f45695fb810a48b216`: workspace build --locked, host clippy
--workspace --locked --all-targets, wasm policy (14 tests), wasm build and wasm
clippy all exit 0. The wasm helper selects all 38 packages declared at that SHA,
including safe-cex-ffi/wasm. These are separate measurements from the earlier
scoped batch and do not cover subsequent FINDING repairs. Only evidence files
changed while these source inputs were being measured.

Not run in this follow-up: workspace tests, wasm runtime tests, live endpoints
or CI reruns.
