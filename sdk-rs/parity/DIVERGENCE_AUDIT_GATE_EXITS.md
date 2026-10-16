# Divergence audit measurements

Audit parent: `0445a753756f197e4363222eab9e787868e4f700`, fetched integration.
The branch starts directly there, without open feature ancestors. A later fetch
returned the same integration revision; merge-down reported already up to date.
The frozen selection, classifications and finite floor are in
[DIVERGENCE_AUDIT.md](DIVERGENCE_AUDIT.md). No production source was changed by
this audit. All temporary native mutations were restored byte-for-byte.

## Native witnesses and controls

Initial BingX test revision: `d13ec5b5014d295e13cacfa639af0aa584e44d26`.
The command completed after `17d62c23a2cebd68e29c0d4a647d78a3dd6505ad` added
only the control runner. `git diff --exit-code` over the two venues' source and
test trees returned 0 between these revisions. The measured test command was:

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bingx --test data_reads --locked --jobs 1`

Exit **0**, **7 tests passed**. This includes the original 36-case live corpus,
the three new independently fixed hints, and a source-control parent test that
requires child exit 101 plus `BINGX-AUDIT source hint type` in its diagnostic.

At `17d62c23a2cebd68e29c0d4a647d78a3dd6505ad`:

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-blofin --test orders --test rest_lifecycle --locked --jobs 1`

Exit **0**, **3 orders tests and 10 REST/lifecycle tests passed**. Orders include
the original 1,543-case corpus and two new early-refusal cases. REST/lifecycle
includes the original 61-case corpus and one sum-order case. Its API/constructor
subcorpora also execute through the existing single-capture oracle. The orders
source-control parent requires child exit 101 plus `BLOFIN-AUDIT source clock`.
These supplements assert known source/native differences, not conformance.

At the same revision, `python3 sdk-rs/parity/scripts/divergence-audit-controls.py`
returned **0**. It ran one Cargo process at a time with jobs 1. Each production
mutation compiled, failed a runtime assertion at **101**, restored the original
bytes in a finally block, then reran the same exact test at **0**:

| Native mutation | Required assertion |
| --- | --- |
| BingX response hint uses JSON display instead of JS String coercion | `BINGX-AUDIT native coerced hint` |
| BloFin order mapper calls clock before side refusal | `BLOFIN-AUDIT native skipped clock` |
| BloFin validation parses available in place of frozen | `BLOFIN-AUDIT validation native precedence` |

All three controls were detected; no survivor was dropped. Later
`git diff --exit-code` over both production source trees returned 0. The native
fields/error categories/messages, clock counts and emitted hints are asserted
directly; the new tests do not rely on decode/re-encode comparisons.

## Source observations

At `457c1f7f144dead7f9ebcbee8ed08fae9c2381a4`, Node executed
`safe-cex-venue-apexomni/scripts/divergence-audit.cjs`: baseline **0**;
`SAFE_CEX_APEX_AUDIT_SOURCE_CONTROL=client-id` **1**, named returned-client-id-unit
assertion; `query` **1**, named transmitted-query assertion; restored baseline
**0**. The two controls mutate only extracted source held in memory.

At `335fee5fea714f4c6232589de1f59a4232172ec7`, Node executed
`safe-cex-venue-bingx/tests/diagnostic_audit.cjs`: baseline **0**;
`SAFE_CEX_BINGX_AUDIT_DIAGNOSTIC_CONTROL=1` **1**, named diagnostic-returned-unit
assertion; restored baseline **0**. It observes the actual returned Set member
before serialization. Together with the two child controls above, these are five
detected source controls, separate from the three native production controls.

Source-only executions of BingX `account_reads_oracle.cjs`, BingX
`types_oracle.cjs`, and BloFin `types_oracle.cjs` returned **0** at revision
`17d62c23a2cebd68e29c0d4a647d78a3dd6505ad`. Their sampled observations were
getAccount.userId=[97,D800,98], FETCH_MARGIN_MODE_RESPONSE.msg=[D800], and
POSITIONS_RESPONSE.code=D800. These are source observations; the native refusal
paths for these justification findings were inspected, not newly executed.

At `7a4c368c2ff50962a0b0eca979d574420f86ec52`, source-only ApexOmni
`scripts/transaction-records.cjs` and `scripts/records.cjs` each returned **0**.
The former directly records amount/tx.amount=[D800]. The latter extracts the
maintained HistoryFundingObject schema and observes the shared materialized
string boundary. A separate typed-JSON.parse witness compiled the actual
HistoryFundingObject declaration and observed symbol=[D800], fundingTime=17
(exit **0**). It is a declared-shape observation, not a source runtime validator.
Native whole-record refusal code/tests were inspected, not newly run here.

All new source captures used Node **24.20.0**, **arm64**. No Node 26 or other
architecture result is claimed for this audit.

## Compilation, census and extraction

At `7a4c368c2ff50962a0b0eca979d574420f86ec52`:

| Command | Exit | Observation |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bingx -p safe-cex-venue-blofin --all-targets --locked --jobs 1` | 0 | Both changed Rust test crates; not a workspace check. |
| `yarn parity:extract:check` through the explicit Node/Yarn entry | 0 | Output: 7,341 symbols, 512 source files, would update 0 manifests, 0 added/orphaned. Output text was read. |
| Python 3.14 tomllib census over the three venue manifest directories | 0 | ApexOmni (28 ported,140 divergent,0 pending), BingX (20,79,0), BloFin (40,85,0); new audit findings 5/4/6. |

The canonical `parity:extract` reconciler ran after the manifest edits; its output
was `Updated 0 manifests; 0 added, 0 orphaned symbols.` Annotations were retained,
no rows were regenerated, and no file-index array was hand-written. Native test
inputs and runner were byte-identical from 17d62c23 to 7a4c368c (`git diff
--exit-code` returned 0 over those exact source/test/runner paths). The diagnostic
script was a separate addition and has its own source measurement above.

## Requested workspace follow-up, separate branch

These measurements belong to `7c68574cbac56e525dd6d6a4b796dedc61856fdb`:
workspace locked build **0**; workspace all-targets host clippy **101** at
AsterDEX `tests/runtime_assembly.rs:441:249`, `possible_missing_else`;
policy wasm clippy **0** using `.github/rust-wasm.json` (38 packages, FFI wasm
feature, wasm32 target/all targets). The initial uncapped build was interrupted
at **130**, then the jobs-2 rerun passed. No wasm lint site reproduced. The host
fixture is cfg-excluded on wasm; that pass cannot validate it.

The independent-guard formatting repair passed scoped AsterDEX all-targets host
clippy **0** at `289e591f561e25cfee38a09b473aa1546a669606`. Its branch was merged
down and pushed at `3d3fccf8b639d92fd14386b034f7a4aeaf99b782`; detailed committed
evidence and read-only sub-agent review are on PR 7885. At initial publication,
workspace host clippy had not been rerun after repair. The continuation below
records the subsequently requested full-workspace rerun.

Not run on this audit branch: workspace build/clippy, wasm compilation/clippy or
runtime tests, full crate/workspace/monorepo suites, newly executed native decoder
or scheduling witnesses for the justification/static findings, live credentialed
HTTP/WebSocket calls, or CI reruns. The earlier workspace gates above cover their
named earlier revision only. Logs/corpus captures remain outside the repository.

## Requested full-workspace host rerun

The repair branch merged fetched integration
`f853d56fd227c2106dba609141b1fdd44bcab125`, producing
`43b9c840b9a453684054576b54d8d453c4a5ce9e`.
From `sdk-rs`, `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --locked`
returned **101**. The emitted failure was Bybit
`tests/inverse_read_surface.rs:240`, `clippy::manual_noop_waker`.

Commit `da2bc0f9cde5b2dd7c8063e58435396f60aa630b` replaced that test's custom
no-op Wake implementation with Waker::noop(), preserving its bounded polling.
The exact same full-workspace command then returned **0**, finished in **8m15s**.
Only the manually authored AsterDEX gate evidence was dirty during that pass;
no Rust, test, manifest, lockfile or build input differed from the measured SHA.
Read-only sub-agent review passed. Detailed evidence was committed and pushed at
`07c3d6ee391d14606bdcd8822d49c327bea9a460` on
[PR 7885](https://github.com/Tealstreet/tealstreet-next/pull/7885).
No wasm or runtime-test rerun accompanied this lint-only repair. This is a
separate branch measurement, not workspace coverage of the continuation branch.

## Continuation measurements

Fresh audit parent: `f853d56fd227c2106dba609141b1fdd44bcab125`, directly on
integration with the initial audit merged and no open feature ancestors.
The freeze commit is `f87fff2d5227101ead8f5220d6b4ba2c81faefc9`.
The final fetch returned the same integration head; merge-down reported already
up to date. Production Rust and TypeScript for all three venues remain identical
to that parent (`git diff --exit-code` over their six source trees: **0**).

At `be22d83e337f6257cc0dd07afa3a801d1a3fd4a2`, the first focused Apex test
invocation omitted the mandatory TypeScript compiler environment selector. It
returned **101** at `APEX-CONT actual source`, with ERR_INVALID_ARG_TYPE for the
missing compiler path. This was a harness invocation error, not a mutation or a
native semantic finding. With SAFE_CEX_TYPESCRIPT_PATH supplied, the same command
returned **0**, **1 passed, 59 filtered out**:

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --test exchange --locked --jobs 1 continuation_position_eager_coercion_known_difference -- --exact --nocapture`

Only the audit document and finding notes were dirty during these invocations;
they are not test/build inputs. The native test/script inputs did not change
between be22d83e and the committed audit revision below.

At `c5db07a11f3f37ed767d79e98366f593c41b4b91`:

| Command | Exit | Measured scope |
| --- | ---: | --- |
| `python3 sdk-rs/parity/scripts/divergence-audit-controls.py --only apexomni` | 0 | Two sequential production controls, each named runtime assertion **101**, restored exact test **0**, **1 passed/59 filtered out**. |
| `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --all-targets --locked --jobs 1` | 0 | Apex host all-targets compilation/lint only, finished in 53.41s; not a workspace or wasm gate. |
| `yarn parity:extract:check` via the explicit Node/Yarn entry | 0 | Output: 7,341 symbols across 512 files; would update **0**, added **0**, orphaned **0**. Output text was read. |
| Python 3.14 tomllib ordered-row comparison with the integration parent | 0 | Apex **28/140/0**, BingX **20/79/0**, BloFin **40/85/0**; **zero** status/target moves; **62/50/48** new finding notes; every historical annotation retained. |

The two controls independently bypass the unused customInitialMarginRate and
defaultInitialMarginRate coercions when customImr is already truthy. Both require
`APEX-CONT native eager leverage coercion` in a compiled test's failure output;
compiler errors cannot satisfy the control. Both were caught, both restored
passes completed, and the final production-source diff returned **0**. The runner
had no survivors. These are known-difference canaries, not repaired parity.
The first default census helper keyed rows only by ts and failed an independent
length assertion on same-name type/value declarations. Ordered full-row pairing
corrected that helper; the table above comes from the successful measured run.

Before cross-review, the canonical writer ran after all 160 note edits, returned **0**, and reported
updated **0** manifests, added **0**, orphaned **0**. It reconciled existing rows;
no annotations or arrays were regenerated or hand-written.

## Continuation source evidence and limits

All continuation source supplements ran with SAFE_CEX_TYPESCRIPT_PATH, Node
**24.20.0/arm64**. At the freeze SHA, the three read-only reviewers executed the
maintained Apex record/transaction/signer scripts, interface-derived open-string
supplements, scalar serializer/unit probes, OHLCV nonarray probes, websocket
dispatch/order probes, BingX type/unit/clock/eager-prefix probes, and BloFin type
and ten targeted method probes: successful exits **0**. Apex's initial temporary
interface-string probe had a replacement-string SyntaxError (**1**); using a
callback replacement corrected the harness (**0**). The null-account/order
supplement ran at be22d83e (**0**). Native contrasts for these reports were
inspection-only, except the two position cases explicitly tested above.

The permanent Apex `scripts/continuation-observations.cjs` ran source-only at
`072776586c0658415ea49bfc1d1729574675be17`, exit **0**. Its seven fixed cases cover
the two ignored leverage fields, missing-market PnL ordering, three order enum/
bracket-clock paths, and filter-before-map failure precedence. The Rust canary
asserts the two leverage cases natively; it does not claim seven native checks.
At c5db07a1, the root separately reran the BingX combined source probe and BloFin
ten-method probe, each **0**, confirming their primitive units/effects/errors.

Not run for this continuation: workspace build or workspace host clippy, wasm
build/clippy/runtime, full crate/workspace/monorepo test suites, the 59 other Apex
exchange tests, fresh native decoder or scheduling tests for the inspected
findings, Node 26 or another architecture, live credentialed requests, or CI
reruns. The separate full-host pass above stays attached to da2bc0f9. At this
measured revision, the draft stopped at frozen iterations **140/79/85**, with
**160** additional findings and **one unresolved** row. The cross-review correction
below supersedes those draft classifications. Findings are recorded work, not
implementation repairs.

## Adversarial cross-review correction

The read-only cross-review at `8d1d80dd1276c9642eca12ebb20218ed5af78af5`
identified one P2: BingX iterations 51/73 had only helper-output unit probes;
downstream consumers emit error text or construct rejected Errors, and the
nonempty errMsg comparison does not distinguish D800 from U+FFFD. Neither a
downstream unit-sensitive reader nor a diagnostic-only scope was established.
The executed helper probes remain source-output evidence only.

Those two classifications become unresolved/proof_gap, and their exact original
manifest notes are restored. The final continuation classification is therefore
**107 supported / 158 findings / 3 unresolved**, including the original UUID
domain gap. Final cumulative classification: **128/173/3** out of **304**.
The prior c5db07a1 measurement remains a historical 160-note snapshot and is not
reported as measuring the corrected note census. No native source, tests or
control runner changed in this correction.

The other reviewer found throw-versus-report wording in the Apex filter notes.
Their source paths throw/reject without an emitter; the prefixes now say so.
No status or target changes accompany these wording and proof-scope corrections.

At correction SHA `e64665ac702c9472082c4bc796c36878b05344c4`, fresh
`yarn parity:extract:check` returned **0**: 7,341 symbols, 512 files, would update
**0**, added **0**, orphaned **0**; output text was read. Its preceding writer
returned **0** with the same zero reconciliation counts. Python 3.14 tomllib
ordered-row comparison returned **0**, measuring triples **28/140/0**, **20/79/0**,
**40/85/0**; new finding notes **62/48/48**, total finding notes **67/52/54**,
zero status/target moves and exact historical retention. All 268 authored
classifications reconcile as **107/158/3**, kinds **71 explanation / 68 scheduling /
19 behavior**, stopping at **140/79/85**. Comparing all three crate trees and the
control runner with c5db07a1 returned **0**; no native gates were rerun for notes.

Both read-only sub-agent cross-reviews passed at e64665ac, after accepting the
consumer-proof correction and throw wording. Neither reviewer ran Cargo or an
extractor; their inspection is separate from the root's measured gates above.
