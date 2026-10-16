# GRVT and Aster runtime delivery evidence

Measured combined commit: `b39e7e009436a1f3cbc44e3ace3c992a03055c79`.
This manually authored record follows that commit; later evidence-only commits
are not described as having executed these gates.

Started from integration `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`;
merged integration `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788` before delivery.
Reviewed feature ancestors included explicitly:

- GRVT proxy timing `a37beba854f9541b3ba156f951d58c556e460f82` (PR 7855).
- Aster streams `2fa27b3bf279b871cd146409f951b8085dcf944d` (PR 7859).
- Aster numeric ingress `69a0b85cc2087faeee60ed65bab23f3db2b11bfa` (PR 7871).
- Aster metadata/signing `7e80a899e067c4c26f5433c1ee24de178c3613f1` (PR 7877).

Each received readonly sub-agent MERGE review; no unreviewed feature ancestor
is being presented as an integration parent. Their earlier gate exits remain
bound to the separate SHAs in their own evidence files.

## Measured row changes

Python tomllib over the venue manifests at b39:

| Venue | Ported | Divergent | Pending | FINDING-containing rows |
|---|---:|---:|---:|---:|
| GRVT | 113 | 51 | 0 | 0 |
| AsterDEX | 142 | 89 | 0 | 0 |

Aster baseline at ac0 was 130/92/9 with 30 FINDING-containing rows (including
embedded findings). Nine pending rows moved: eight ported, one CHOSEN class
composition divergence. Four existing divergent rows became ported across the
reviewed slices, including the newly concrete V3 signature adapter. Thus 13 rows
changed status, and 30 Aster findings plus the one GRVT finding were closed.
This is a manifest census, not a claim of exhaustive runtime proof.

## Combined gates at b39

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-asterdex
  --all-targets --jobs 2`: exit 0.
- The same scoped check for `safe-cex-venue-grvt`: exit 0. These are two scoped
  checks, not workspace compilation. Only one cargo process ran at a time.
- Seven named Aster test targets compiled with `--no-run --jobs 2`: exit 0.
  Their recorded Cargo executables were then run with the filters below.
- Extractor check: exit 0; output `Would update 0 manifests; 0 added,
  0 orphaned symbols` across 512 source manifests. Writers after annotation
  edits also exited 0. File indexes were derived from preserved row targets;
  no rows were regenerated.
- rustfmt on the three new modules and runtime test: exit 0.
- Added public-slice lines scanned for machine paths: exit 0, zero matches.

| Named Aster target/filter | Node 24 exit / tests | Node 26 exit / tests |
|---|---|---|
| runtime_assembly (test file) | 0 / 2 | 0 / 2 |
| live_api: api_message_signature* | 0 / 3 | 0 / 3 |
| live_exchange_helpers: exchange_helpers_match_typescript | 0 / 1 | not run |
| live_exchange_helpers: order_mapper_matches_typescript | 0 / 1 | not run |
| live_exchange_helpers: mapper_metadata_retains_native_numbers | 0 / 1 | 0 / 1 |
| live_types: numeric_ingress_native_bits | 0 / 1 | 0 / 1 |
| live_types: object_record_depth_cost_is_explicit | 0 / 1 | not run |
| live_types: opaque_extensions_retain_native_numeric_values | 0 / 1 | not run |
| live_public_stream (test file) | 0 / 3 | not run |
| live_private_stream (test file) | 0 / 12 | not run |
| live_account_helpers: account_helpers_match_typescript | 0 / 1 | not run |
| live_account_helpers: mirror_cumulative_order_fill_matches_typescript | 0 / 1 | not run |

GRVT `builder_proxy` test file: Node 24 cargo test exit 0 / five tests;
the Cargo-recorded executable under Node 26 exited 0 / five tests. This covers
eager resolver entry, pending/waker behavior, header ordering, errors/registry
replacement, borrowed cancellation cost, and the source empty-URL child control.

Runtime source floor: 70 cases through iteration 69, independently enumerated
as startup 16, tick 11, disposal 4, constructor 3, public factory 3, request 12,
fulfilled response 4, rejected response 12, retry predicate 5. The constructor
floor is 44 arrow fields ending at dispose, plus two static values. Native
values, bits, original payload/error/owner/Store/promise handles and ordered
effects are asserted before any wire observation. The retry source observer
records the actual returned request Promise before async adoption. The retained
loop callback is invoked after replacing tick and with a foreign receiver.

Other measured floors retain their independent expectations: metadata 18 cells,
88 terminating cases ending at 87 (two positive-infinite precision cases are
excluded because the source loop does not terminate); numeric ingress 68 fields
across 22 shapes; object-depth cost 17 roots ending at 16. See the owning evidence
files for their earlier controls and exact acceptance costs.

Source dependency isolation at b39: exit 0 with exactly six copied input files,
no ancestor node_modules, 70 runtime cases/44 fields/two statics. Missing explicit
TypeScript compiler: exit 1, MODULE_NOT_FOUND. The runtime library/base/socket
fixtures capture dependency operations; they do not prove crypto or network
primitive implementations. The source window mutation child failed 101 and the
unchanged parent passed on both Node majors. Signer wrong-chain and missing real
ethers child controls also passed their expected-failure assertions.

## Independent compiled native controls

At `3660a6a304dc9848063adf836425a708e4b03460`, temporary production call-site
selectors were compiled once (exit 0), then activated independently. All 18
selected controls exited 101 at the named native assertions below. The same
compiled binary with no selected mutation exited 0. The three production files
were restored byte-for-byte (git diff exit 0). Aster src/tests/scripts are
byte-identical from 3660 to b39; the restored b39 tests above passed.

| Production control | Named native failure |
|---|---|
| class owner replaced | start:default iteration 0, native runtime result should resolve |
| public constructor changed to private | start:default iteration 0/20/1, native text |
| cloned socket owner, same Store | constructor:primary iteration 31/20/2, native bool |
| primary slot initialized from secondary | constructor:primary iteration 31/apis/0, native number bits |
| ticker fetch replaced by market fetch | start:default iteration 0/10/0, native text |
| position-version guard bypassed | tick:due iteration 16, native array shape |
| disposal skipped | dispose:all iteration 27, native bool |
| public rate 10 changed to 11 | constructor:primary iteration 31/19/2/maxRequests, native number bits |
| recvWindow changed | request:v1 iteration 37/params/recvWindow, native number bits |
| returned primary instance replaced | constructor:primary iteration 31/apis/0, native number bits |
| ABI string type changed to bytes | request:v3 iteration 44/0/1/0, native text |
| read-only retry guard removed | retry:write iteration 66/retry, native bool |
| retry return changed to await | rejection:timestamp iteration 54, native array shape |
| fulfilled response data getter cached | response:data-getter iteration 52, native text |
| suppression message evaluated early | rejection:config-getter iteration 63, native array shape |
| response destructuring replaced by field gets | rejection:null-response iteration 64, native text |
| startup options cached before clock callee | start:clock-start-getter iteration 5/3/1, native undefined |
| scheduled tick method captured early | native retained loop return identity |

No control survived. The earlier stream slice did find a surviving account
mutation aimed at the wrong test family; that finding and its corrected raw
outbound-account call-site proof remain in STREAM_FINDINGS_GATE_EXITS.md.

Readonly runtime review initially rejected getter ordering, destructuring error
shape, added retry await, a constant socket-owner assertion and the unexercised
loop callback. Actual-source/native regression cases were added for each, and
review returned MERGE at `63e82878b0b2e1d57b2c89e88d34dd3a0da0696a`.
The reviewed runtime modules and runtime fixture are unchanged in b39.

Official live docs checked during this unit: [API repository](https://github.com/asterdex/api-docs)
and [V3 reference](https://github.com/asterdex/api-docs/blob/master/V3%28Recommended%29/EN/aster-finance-futures-api-v3.md).
TypeScript remains the behavior reference, including its query-placement choices.
Class composition cost cites [the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy);
other declarations remain at their [canonical numeric](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[closed-output](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), and
[alias](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) anchors.

Not run at b39: workspace build, full crate/monorepo suites, the full 51-test type
file, host clippy, wasm check, wasm clippy, credentialed exchange calls, concrete
HTTP/network integration, CI reruns, or source infinite-precision termination.
No such coverage is claimed.

## Requested workspace follow-up

Tested revision: `7c68574cbac56e525dd6d6a4b796dedc61856fdb`.
Last merged integration parent: `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`.
The four feature ancestors recorded above had local sub-agent reviews; they were
open PRs when these commands started. No production or test source changed during
these three gate measurements. Commands ran from `sdk-rs`, sequentially, with
`CARGO_BUILD_JOBS=2`.

| Command | Measured exit | Observation |
| --- | ---: | --- |
| `cargo build --workspace --locked` | 0 | Finished dev profile in 2m 37s. |
| `cargo clippy --workspace --all-targets --locked` | 101 | `tests/runtime_assembly.rs::Host::leaf` has two independent adjacent if statements on one line; `possible_missing_else` denies that layout. This is the first reported failure, not an exhaustive inventory of later host sites. |
| `node ../.github/scripts/rust-wasm.mjs --clippy` | 0 | Finished dev profile in 55m 53s. Policy coverage validated all 38 wasm packages, `safe-cex-ffi/wasm`, target `wasm32-unknown-unknown`, all targets and jobs 2; native-only `safe-cex-parity` was excluded by policy. |

The initial uncapped workspace-build attempt was interrupted deliberately and
returned 130. It is not the passing build above; the capped rerun returned 0.
Wasm produced no lint site in this run. The failing host fixture is excluded by
its `cfg(not(target_arch = "wasm32"))`; the wasm pass cannot validate that body.
The follow-up formatting commit keeps both independent guards and their order.
No lint allowance or conditional gate was added.

No workspace tests, wasm runtime tests, live HTTP/WebSocket calls, or CI reruns
were performed for this follow-up. Earlier scoped source/native tests remain
bound to their earlier revisions above. Generated logs remain outside the repo.

Repair verification at `289e591f561e25cfee38a09b473aa1546a669606`:
`CARGO_BUILD_JOBS=2 cargo clippy --manifest-path sdk-rs/Cargo.toml -p
safe-cex-venue-asterdex --all-targets --locked --jobs 2` returned 0 (1m 46s).
This is the AsterDEX host check only. Workspace host clippy was not rerun after
this formatting repair; its measured result above remains 101. Workspace build
and policy wasm clippy remain measurements of 7c68574c, not of this repair SHA.

Merge-down revision `d779365cdc665390fa74a1ab24948e213c2ee41f` incorporates
integration `0445a753756f197e4363222eab9e787868e4f700` without conflicts.
A `git show` enumeration of literal workflow step-name lines found 352 before
and 352 after, with no additions or removals; this is that enumeration only.
No gates were rerun on the merge revision. Earlier measurements remain bound to
the full revisions above. A read-only sub-agent review of the fixture repair and
follow-up evidence found no issue; it ran no gates.

## Full host rerun after the repair

Tested revision: `43b9c840b9a453684054576b54d8d453c4a5ce9e`, incorporating
integration `f853d56fd227c2106dba609141b1fdd44bcab125` and the earlier Aster
repair. `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --locked`
from sdk-rs returned **101**. Its first reported error was Bybit
`tests/inverse_read_surface.rs:240`, `clippy::manual_noop_waker`.
The Aster source/test trees are byte-identical to the scoped repair measurement
at 289e591f (`git diff --exit-code`: 0); this is not a full-workspace pass.

Repair revision `da2bc0f9cde5b2dd7c8063e58435396f60aa630b` replaces that Bybit
test-only custom no-op Wake implementation with `Waker::noop()`, removing the
orphaned type/imports. A read-only sub-agent review passed this exact diff and
verified unchanged pinning, 1,024-poll bound, return and panic behavior. It ran
no Cargo. No Bybit production source or parity annotation changes accompany it.

Full restored measurement at `da2bc0f9cde5b2dd7c8063e58435396f60aa630b`:
`CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --locked` from sdk-rs
returned **0**, finishing the dev profile in **8m 15s**. This is the full host
workspace gate, including the two repaired test fixtures. The only working-tree
change during this measurement was this manually authored evidence file; no
Rust source, manifest, test, lockfile or build-script input changed. The previous
43b9c840 result remains 101; this passing result belongs to da2bc0f9.
No wasm gate, runtime/oracle test or CI rerun was performed for these lint repairs.
