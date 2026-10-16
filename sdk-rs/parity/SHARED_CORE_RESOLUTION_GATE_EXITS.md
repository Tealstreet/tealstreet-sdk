# Shared-core resolution gate exits

Feature parent actually used: integration
333fd419fdbf70f081039610f22da9ee653bc96f. Merge-down parent:
d4e20845a85cc4e2711c99648ef7547f8aa806df. Measured post-merge tree:
3ccc5accdcdd439da858a93f513b115a333a8377. Neither unmerged audit PR 7937 nor
7940 is an ancestor. Their frozen historical tables are cited separately.

This file is authored evidence, not generated test output. Exact measured SHAs
remain explicit; its later documentation commit does not retroactively change
the SHA covered by a gate. Runtime TypeScript loading used the session-provided
SAFE_CEX_TYPESCRIPT_PATH; no credential or machine path is part of this artifact.
The source runtime measured here is Node v24.20.0; Node 26 was not run.

## Baseline regressions and first repair measurements

All Cargo invocations used `--manifest-path sdk-rs/Cargo.toml --locked --jobs 2`.
Tests were selected by package and integration-test target, with exact test
filters where a test name is listed. The two-file measurement ran both entire
scoped integration-test targets.

| Measured SHA | Target / test | Exit | Actual observation |
| --- | --- | ---: | --- |
| 29fa1f68bc0bd957d8941552a90d5998c60f4489 | safe-cex-store / patch_info / native_info_patch_retains_reserved_key_object | 101 | Compiled; failed `STORE-INFO native object retained: order/false` |
| 29fa1f68bc0bd957d8941552a90d5998c60f4489 | safe-cex-exchange / cross_trade_numeric / public_numeric_weights_retain_javascript_number_bits | 101 | Compiled; failed `CROSS-NUMERIC native config accepted`, number out of range |
| 5ac22c6e43bea5e52597767a32e5584c9894b5a7 | same Store test | 0 | Object/key/native sibling assertions passed; this earlier test had eight iterations including a repeated batch |
| 5ac22c6e43bea5e52597767a32e5584c9894b5a7 | same numeric test | 0 | Native bit assertions passed through raw and owned-Value entry |
| 3c5dbe6e3f9e3957c5f4ec748ef9a06651b1e920 | safe-cex-exchange / cross_trade / native_position_audits_match_source_requests_before_polling | 101 | Compiled; failed `CROSS native eager audit calls stock constructor skips master order`, actual 1 versus expected 0 |
| 77bfe0869ee0a2c8b063481aa6907f3370cb0b60 | same position-audit test | 0 | All 26 independent source/native audit cases passed |
| f1158311bcff33282b13abc491e88090b4a1097b | safe-cex-store / patch_info | 0 | Seven operation/carrier cases and four actual source Store paths passed |
| a5b086ac28bd5e27c252a89b82188eaaa2c40a7c | safe-cex-exchange / cross_trade and cross_trade_numeric | 0 | Seven CrossTrade tests plus one numeric test; native assertions and actual source witnesses passed |

The baseline failures expose preexisting defects; they are not substituted for
deliberate controls. The combined test attempt launched at
8a2c9121adeb992a68d43ecdd19c195477ce7426 exited 101 before tests because
incremental object copies reported missing files. The cause of that filesystem
loss was not established by this lane. Subsequent invocations explicitly used
`CARGO_INCREMENTAL=0`; no cache was deleted by this lane.

## Deliberate controls at the post-merge SHA

Base tree for every control below is
3ccc5accdcdd439da858a93f513b115a333a8377. Each temporary production mutation
was restored byte-for-byte before the restored test ran, with one Cargo at a
time. No mutation was committed. The harness independently required exit 101
and the named assertion, then exit 0 after restoration; compile failure alone
could not satisfy the control. Enumeration stops at control 6 in this table.

| Control | Deliberate break | Named failed assertion | Mutated exit | Restored exit |
| --- | --- | --- | ---: | ---: |
| 1 | Accept inherited symbol in copier relevant-position filter | `CROSS native eager audit calls stock constructor filters copier` | 101 | 0 |
| 2 | Insert primitive reverse __proto__ as an own key | `CROSS native eager audit calls invert proto setter skips close` | 101 | 0 |
| 3 | Rebuffer literal info through serde_json::from_value | `STORE-INFO native object retained: order/false` | 101 | 0 |
| 4 | Bypass outer config numeric capture | `CROSS-NUMERIC native config accepted` | 101 | 0 |
| 5 | Bypass standalone copier numeric capture | `CROSS-NUMERIC native copier accepted` | 101 | 0 |
| 6 | Replace inherited constructor identity with prototype identity | `CROSS native inherited constructor` | 101 | 0 |

No survivor in this six-control floor. This is not exhaustive mutation coverage.
The restored position audit, Store and numeric tests also execute their actual
source witnesses. Native field, bit, object-kind and identity assertions precede
wire comparisons; no symmetric roundtrip supplies the claimed proof.

## Other post-merge measurements

Every measurement in this section covers
3ccc5accdcdd439da858a93f513b115a333a8377.

| Command / scope | Exit | Observation |
| --- | ---: | --- |
| Cargo safe-cex, `--test production_store_wiring` | 0 | One actual DefaultStore/BaseExchange composition test passed |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-store -p safe-cex-exchange --all-targets --locked --jobs 2` | 0 | Both selected crates and their targets compiled; not workspace compilation |
| `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-store -p safe-cex-exchange --all-targets --locked --jobs 2 -- -D warnings` | 0 | Strict host lint for the selected two crates; not workspace host clippy |
| `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-store -p safe-cex-exchange --target wasm32-unknown-unknown --lib --locked --jobs 2 -- -D warnings` | 0 | Separate strict wasm library lint for these two entries of .github/rust-wasm.json, using their empty feature lists; not the full wasm package matrix or wasm execution |
| `yarn parity:extract` through the supplied Yarn JS entry | 0 | Output: 7341 symbols, 512 source files; updated 0, added 0, orphaned 0 |
| `yarn parity:extract:check` through the same entry | 0 | Output: would update 0 manifests; no additions or orphans |
| Python 3.14 tomllib comparison to integration d4e20845a85cc4e2711c99648ef7547f8aa806df | 0 | All 7341 row structures identical except notes; exactly the independent selected 46 note identities changed; zero TypeScript production diff |
| Python 3.14 scoped census | 0 | CrossTrade 5/26/0; transport 42/15/0 plus 6 n-a; Store 76/7/0, ordered ported/divergent/pending |
| `rustfmt --edition 2024 --check` on the seven touched Rust files | 0 | No formatting changes required |
| `git diff --check d4e20845a85cc4e2711c99648ef7547f8aa806df...HEAD` | 0 | No whitespace errors |

Yarn output text was inspected; its process exit alone did not decide freshness.
The first taxonomy metadata reader used a system Python without tomllib and
failed; the corrected Python 3.14 readers above produced the measured exits.
Read-only sub-agent review is separate from execution: reviewers ran no Cargo
or tests. The original 412-row classification is historical evidence, not a
claim that all those runtime functions were executed by this resolution.

## Exclusions and delivery

No Cargo was concurrent with another Cargo in this lane. Scoped checks and
lint remain scoped. Workspace build was subsequently run as recorded below.
Not rerun: workspace host clippy, the full wasm policy matrix, any full test suite,
the parity-check binary, wasm runtime execution, live exchange/network tests or
Node 26. It does not establish concrete CrossTrade facade/browser host assembly.
The earlier fleet-wide gates remain claims at their earlier SHAs, not coverage
borrowed for this branch.

The original parent and merge-down were both integration commits. No unmerged
feature branch was inherited; PR 7937 and 7940 remained open at the last check.
Their selected historical notes may need reconciliation if those audit PRs land
before this resolution. The historical taxonomy and read-only reviews do not
move statuses. The 46 selected explanations resolve as 35 rationale-only and
11 direct repair rows, with zero status or target movement.

Final independent implementation review passed at the measured post-merge SHA.
Evidence review inspected all twelve mutation/restoration logs and matched the
six named controls to that SHA; it caught and corrected an overbroad exact-test
filter description. The evidence-only delivery commit adds this file without
changing any measured production, test, manifest or canonical-declaration input.


## Requested workspace build after integration merge-down

Measured commit: `deb33935b451a8127c1125aa1b41879dd9c0883a`.
Its parents are the prior reviewed PR7954 tip
`6f187bcfd3983a14c972e432a96c60b47a178660` and integration
`4891e46a0770116f292342ece37e7c6809983859`, which contains merged PR7970.
The merge completed without conflicts. No unmerged feature branch was added.
The rust.yml named-step sequence remains identical:93 before and93 after;
this is that workflow's named-step inventory, not all workflows or executed CI.

From sdk-rs, ran exactly:

```sh
cargo build --workspace --locked
```

Environment: `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`.
Actual process exit: **0**. Cargo reported Finished dev profile in1m36s;
output contains zero warning lines. No code repairs were needed. This supersedes
the initial resolution delivery's workspace-build NOT RUN entry.

Unlike the earlier PR7970-only build, this measured tree includes both the
integrated shared-core behavior repairs and PR7954's46 resolutions. The later
follow-up changes this evidence document only, keeping every workspace build
input byte-identical to the measured commit. Existing scoped host/wasm lint and
six-control claims remain attached to their earlier exact SHAs; they were not
rerun or broadened by this build.
