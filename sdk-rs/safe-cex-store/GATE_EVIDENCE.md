# P4 utils — priority Store contract

The isolated contract adds `Store`, `StoreUpdate`, `PackedTickerDelta`, and the
four model declarations required by its full snapshot/read surface. Concrete
`DefaultStore` behavior remains for the next slice. Behavioral sources remain
`packages/safe-cex/src/store/store.interface.ts` and `packages/safe-cex/src/types.ts`.

`Store` supports `Rc<dyn Store>`: mutations take `&self`, callbacks are local,
and collection getters return `std::cell::Ref` guards. Release read guards before
reentrant mutation. Dictionary contracts use insertion-preserving `IndexMap`.
Opaque anonymous diagnostics/configuration values retain their JSON payloads.
Workspace `serde_json` features remain unchanged, including `preserve_order`
and `arbitrary_precision`.

All commands ran from the repository root against **safe-cex-types and
safe-cex-store**. Each gate passed, its independent mutation failed for the
named reason, and the restored gate passed. Local logs: `/tmp/p4-contract-*-*.log`.

| Gate | Command | Negative control |
| --- | --- | --- |
| Format | `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-types -p safe-cex-store --check` | malformed spacing in a temporary private function produced a rustfmt diff |
| Strict clippy | `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-types -p safe-cex-store --all-targets -- -D warnings` | an unused temporary function was rejected |
| Runtime/source oracles | `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-types -p safe-cex-store --all-targets -- --include-ignored` | nesting the packed tuple tail caused `flat_packed_tuple_and_partial_update_preserve_wire_shape` to fail |
| Wasm | `cargo build --manifest-path sdk-rs/Cargo.toml -p safe-cex-types -p safe-cex-store --target wasm32-unknown-unknown` | target-only `compile_error!` was rejected |
| Coverage | `yarn parity:extract:check` | removing the PackedTickerDelta source row was detected |
| Invention/status/name/provenance | `yarn parity:check` | an unresolved packed-tuple mapping was rejected |
| Report | `yarn parity:report` | an unresolved StoreUpdate mapping prevented a validated report |

Yarn used the actual Yarn 1.22.22 script under Node 24 with `YARN_IGNORE_PATH=1`,
not the Volta shim. The contract tests additionally check omission versus present
null, reject null for an optional typed collection, preserve dictionary key
insertion order, and compile reads/updates through `Rc<dyn Store>`.

The existing store source oracle still compares object key **sets** and values;
it does not detect object-key reordering. Array order and exact float-bit checks
remain. This contract slice did not widen or replace that comparator.

Adversarial review in a separate checkout identified independently partial worker
update counters. `StoreUpdateIdsPatch` now represents each counter as optional;
full snapshots/getters retain `StoreUpdateIds`. A round-trip test covers orders
only, positions only (including explicit zero), and the empty patch. Restoring
the complete-counter update type makes this test fail with a missing sibling
field rather than silently manufacturing zero.

P4 full-store continuation (2026-10-12): package-local fmt, strict all-target
clippy, all-target tests with ignored live oracles, wasm builds, extraction
coverage, parity-check and parity-report each passed green/negative/restored.
Controls reused the independent contract probes above; logs are
`/tmp/p4-store-full-gates.log` and `/tmp/p4-contract-<gate>-<phase>.log`.
The additional full-store differential negative changed marketsVersion increments
from one to two: the current-source oracle rejected the first update frame.
Restoring it passed all four P4 store tests, including reentrant Rc callbacks,
unknown packed masks and populated stub losslessness. Logs:
`/tmp/p4-store-oracle-negative.log`, `/tmp/p4-store-oracle-restored.log`.

Adversarial review in detached checkout `/tmp/rust-p4-store-full-review-c66e2s`
at 9574918893 caught worker/packed dictionary alias rebinding. Separate value
refresh from binding fixes it. The live oracle now pins 49 frames, including
worker duplicate IDs, packed updates to non-owning rows and colliding market
symbols. Replacing the worker value refresh with binding fails the live update
frame (`/tmp/p4-store-alias-negative.log`); restored four tests pass
(`/tmp/p4-store-alias-restored.log`). Strict clippy passed after the fix.

2026-10-12 production seam correction: interior state moved inside DefaultStore;
Store is implemented on the real production type. Full touched-crate gates
passed in `/tmp/p4-seams-*.log`. The facade missing-impl control proves the store
crate builds while actual constructor composition fails; details in
safe-cex/GATE_EVIDENCE.md. Frozen terminal snapshots now exercise DefaultStore,
and the49-frame live source differential also uses the production type.

Priority calcOrderBookTotal is plain indexed addition in existing array order,
with no sort. Facade re-exports it together with the already-ported model-utils
create_position_id and normalize_execution_position_side. The production facade
test calls all3 APIs and checks unsorted asks/bids retain order and independent
running totals. All7 store+facade gates have independent negative/restored
controls (`/tmp/p4-venue-utils-*.log`): inserting a bid sort fails specifically
venue_utilities_are_callable_and_accumulation_preserves_order. Other controls
check formatting, dead code, wasm failure, deleted row and missing rs targets.
sortOrderBook remains pending for the later utility block.

sortOrderBook now has an independent public implementation and facade alias.
200 maintained live-source finite-book cases compare order, stable ties, retained
totals and subsequent accumulator results. The former NaN-price permutation divergence is repaired by the shared V8
TimSort. The live 112-profile native/source gate covers both sides, preserves
all price/amount/total bits and pins CI Node 24.20.0 / V8 13.6. The old recursive
merge fails the named STORE-BOOK-SORT assertion before the direct cutover.
All7 independent
negative/restored store+facade gates pass (`/tmp/p4-orderbook-*.log`); reversing
asks rejects specifically current_source_orderbook_sort_and_totals. No sorting
was added to the accumulator.

## StoreUpdate input shape

2026-10-12: StoreUpdate and its partial update-ID record use the shared map-only
front guard, with private serde companions. Their bare and complete22-field
positional arrays are rejected. The SAME complete array is permanently asserted
valid for the private companion, making unrelated field errors incapable of
concealing guard removal. Removing the guard fails that exact public rejection;
restoring it passes. Shared types/store/facade gates and independent controls
passed; see the strict-codec section in ../safe-cex-types/GATE_EVIDENCE.md.
The private StoreUpdate visibility doctest also has its own public-path control;
this fails because the replacement compiles, and the restored private path passes.

## Native numeric patches (2026-10-12)

The facade's `native_store_patches` uses the production DefaultStore through the
Store trait. Six binary64 inputs (positive/negative zero, finite, NaN, both
infinities) cover unrelated position updates, nested PositionExtra values, typed
upserts, orders, market precision, balance, ticker/BBO overwrites, aliases,
legacy deltas and packed deltas. JSON is never the target-model mutation path.
Native Number/typed nested patches retain their values; absent fields do nothing.
Invalid known field types return StorePatchError. This closed-model acceptance
boundary is CHOSEN and disclosed on the partial mutation rows.

A live source witness executes the real TypeScript Store using the isolated
compiler and the same six prices (extending bitget's measured position witness).
A source mutation removes updatePosition's actual assignment; a child must fail
specifically at its contracts assertion. Making that mutation a no-op fails the
Rust control because the child unexpectedly succeeds. The oracle also passed in
`/tmp/p4-native-oracle-mirror-c66e2s`, with no node_modules in any ancestor;
replacing the isolated compiler path fails MODULE_NOT_FOUND rather than skipping.

All eight scoped gates passed with independent rejected/restored controls:
fmt (format probe), strict clippy (dead helper), all-target Store/facade tests
(native non-finites sanitized to zero), wasm (target-specific compile_error),
extract/check (deleted updateTicker row), parity/check and report (different
missing patch targets), progress (malformed TOML). Logs: /tmp/p4-native-patch-*.
The separate model-path control restores a Position target JSON round-trip;
the facade's unrelated position patch fails at that round-trip, and restoring
native mutation passes. This is independent of the ticker/BBO sanitation control.
The alias path also copies numeric bits unconditionally: JS equality suppresses
worker diffs for signed zero but cannot suppress alias mirroring of a direct patch.

Adversarial review identified an info-only/empty legacy-delta regression with
duplicate ticker IDs: unconditional alias synchronization stole the alias from
the last duplicate without notification. The source witness keeps alias B.
Synchronization now occurs only for a non-info field. The sixth production test
pins both empty and info-only inputs; restoring unconditional synchronization
fails that exact test. All eight gates and their independent controls were rerun
end to end after the correction and passed.

A second review found partial validation mutating a canonical ticker before an
error, with aliases/counters/notifications left untouched, and empty legacy
patches validating malformed indices before their no-op guard. Both were real
regressions. Mutation is now staged: known-field errors commit no state across
single models or batches. Successful delta snapshots commit in source order to
retain alias rebindings. PatchSource is sealed to stable immutable map/ModelPatch
iteration. Empty/info-only deltas return before row access, including NaN and
fractional indices. Two additional production tests pin these conditions; the
live source witness also covers all malformed indices with empty/info patches.

Four compiling controls independently restore non-atomic ticker, model and
batch writes, and remove the empty-patch guard before row validation; each fails
its named production assertion, then all native tests pass restored. Nonempty
invalid legacy indices return StorePatchError, preserving caller recovery rather
than panicking. All eight scoped gates and independent controls passed again
end to end on this final staging revision. Nine facade production tests pass.
The preexisting malformed packed-delta index/short-value panics remain separate
findings and are not claimed fixed by the native numeric ABI work.


## Remaining19 utility slice — 2026-10-14

Moved **19 pending rows: 3 ported, 16 divergent**, measured with Python3.11
`tomllib`. Starting parent: `89ad07ac80f6efc169430a2f94bd59898660e9f9`.
The scoped71-manifest /438-symbol census stops at manifest71: 332/87/19 before,
335/103/0 at `3ed6d3eaaed3de346e4aa529fbee9fa0a778bae2`.
Venue/worker/transport files outside this declared TS scope were not censused.
Our initial merge-down parent was `19cb1a2b735c3bc6db40bbb0cee849121cfd985b`.
No unrelated unreviewed branch is an ancestor.

Gates measured at **`3ed6d3eaaed3de346e4aa529fbee9fa0a778bae2`**:

- Combined `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-store
  -p safe-cex-exchange -p safe-cex-crypto -p safe-cex-transport --all-targets
  --jobs 2`: **0**.
- Combined all-targets host clippy, same crates/jobs, `-- -D warnings`: **101**.
  Inherited query-tests `Arc<SerializedParamsAdapter>` warnings at
  `safe-cex-exchange/src/http/query_tests.rs:96,320`; that file is unchanged from
  the merged integration parent. This is a recorded failed gate, not a lint pass.
- Combined focused host clippy `--lib --test remaining_utils --jobs 2
  -- -D warnings`: **0**. This excludes unrelated library unit-test targets.
- Combined `cargo test`, same four crates, `--test remaining_utils --jobs 2`:
  **0**, **9 tests**, independent native fields, bits, object identity and traces.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
  node sdk-rs/parity/utils_final_oracle.cjs`: **0**, including mutable exclusion-list
  and U+D800 returned-ID witnesses before JSON. Stopped at16 unknown-outcome
  cases,4 blob member profiles; these are representative checks, not exhaustive.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`:
  **0**, output reads7341 symbols/512 files and **would update0** manifests.
  The writer was run after manifest changes; its output reported0 drift.

The control runner began at `8a644c1d0b`. It executed14 serial production
mutations: every named native assertion failed at runtime **101**, then passed
restored at **0**; stopped at14/14 and no survivor. The run crossed the
manifest/oracle-only review-fix commit; no Rust code/test inputs changed.
`git diff --exit-code 8a644c1d0b 3ed6d3eaae --` all four crates' `src` and `tests`
measured **0**, and live tracked bytes were restored. Control-start SHA and
individual call sites/assertions are preserved in `utils-final19-evidence/controls.json`;
named panic excerpts are attached separately. No compiler failure was counted.

Sub-agent `ticker_review` performed a static review bound to exact
`3ed6d3eaaed3de346e4aa529fbee9fa0a778bae2`: all earlier findings resolved,
no additional findings. The secondary-key rows disclose immutable native snapshots;
the merge-ID note names the actual-source lone unit and required `&str` surface.
Reviewer ran no Cargo/oracle/lint/control gates.

Raw exits, commands, gate logs and symbol census are tracked in
`utils-final19-evidence/`. Full workspace compilation/tests, wasm check/clippy,
CI reruns, and a fresh global Rust-parity diagnostic enumeration were **not run**.
The user's inherited Rust-parity noise is untouched; no global Rust-parity pass
is claimed. Final merge preservation and delivery are recorded separately.
