# WEEX retry-policy gate evidence

Measured code revision: `bf9daca874727363899e17e2b208d5de62a4b51d`.
This narrative is a documentation-only descendant; it does not transfer gate
results to a different code revision.

Authoring parent: `5b5d9004c528addfc33e1ff47ad6961ac5dc94d8`.
Before delivery, merged fetched integration
`f22d7b2c94b74e1f29f96d28b6d07c3163bb5409` in merge commit
`ccf482a1bf4834af84519c5b442192cca8b3b20e`. No unrelated unpublished lane work
was used as an ancestor. The branch's own authoring commit is
`06e113796f711ed2852ccb53d92dd9ff6b831d94`.

## Measured gain

Three rows moved pending to ported; zero moved to divergent:

| TypeScript symbol | Rust implementation |
| --- | --- |
| parseRetryAfterMs | retry.rs::parse_retry_after_ms, retry.rs::WeexRetryAfterValue |
| isWeexReadOnlyRequest | retry.rs::is_weex_read_only_request |
| shouldRetryWeexRequest | retry.rs::should_retry_weex_request |

An independent, fixed list of these three expected transitions was compared
against TOML rows read using `git show` at the measured revision and merged
integration parent: Python exit 0. Both the authoring parent and merged
integration have 28 ported / 26 divergent / 102 pending WEEX rows. The measured
revision has 31 ported / 26 divergent / 99 pending, across 156 WEEX rows.

These are ports of local decisions with explicit host dependencies. Date.parse
receives original UTF16 and the caller supplies the clock value. The network
classifier is the imported axios-retry capability supplied as a callback;
the oracle invokes the installed actual classifier. Neither dependency itself,
arbitrary Axios property getters, nor a full Axios runtime is claimed ported.

## Gates at the measured revision

All cargo commands ran serially. Oracle tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Observed result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | exit 0 |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test market_data | exit 0; both named tests passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/retry-policy-mutations.py | exit 0; three compiled failures, each individually restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | exit 0; canonical scoped invention/status gate: 156 rows, 63 public Rust items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | exit 0; output: 7341 symbols, zero updates, zero added, zero orphaned |

The extractor writer ran after the manifest edit before authoring commit
`06e113796f`: output reported one normalized manifest, zero added/orphaned
symbols. Its draft-tree run is not represented as a final-revision gate.

The retry-policy target's named test is
`retry_policy_matches_actual_typescript`. Its first and only corpus iteration
stops at 29 Retry-After cases, 19 read-only cases, and 19 retry decisions:
67 native results, 17 date capability calls, 16 classifier calls. Fixed ordered
case identities and independent counts are asserted. Native f64 results,
boolean decisions, UTF16 arguments and short-circuit calls are checked directly;
there is no wire roundtrip comparison. This finite corpus is a coverage floor.

## Discriminating controls

Each control changed only its named production expression from the measured
revision. Each compiled, exited 101 with the named native equality assertion,
and then passed the retry-policy target with exit 0 after byte-for-byte source
restoration. Compile errors do not satisfy the control script.

| Mutation | Failed native assertion | Restored test |
| --- | --- | --- |
| Retry-After seconds multiplier 1000 to 100 | retry_policy::retry_after::seconds | exit 0 |
| Remove GET from accepted read methods | retry_policy::read_only::get | exit 0 |
| Retry-count bound less than 1 to less than 2 | retry_policy::policy::rate_second | exit 0 |

All three selected controls were caught; zero survivors in this iteration.
Other mutations were not enumerated.

## Merge compatibility and review

Scoped check at merge revision `ccf482a1bf4834af84519c5b442192cca8b3b20e`
failed with exit 101: shared Ticker now requires timestamp. Correction in the
measured revision supplies None, matching WEEX normalizeTicker's absent source
property, and adds a native timestamp-absence assertion to market_data. The
extractor check passed at the merge revision; the final gates above were run
again against the corrected revision. No failed check is reported as passing.

Sub-agent weex_review inspected code, manifests and tests at the measured
revision and found no concrete issues. That review ran no cargo, tests or
mutations and is separate from the measured gates.

Not run: workspace compilation, full WEEX or monorepo suites, host clippy,
wasm clippy, live venue requests, CI reruns. Income paging is outside this slice.

Local logs are ignored under sdk-rs/target/weex-retry-policy-gates/;
this tracked narrative records their exits and measured revision.
