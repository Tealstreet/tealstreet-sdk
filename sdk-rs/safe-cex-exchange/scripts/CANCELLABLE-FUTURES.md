# Cancellation discovery and classification

Run `node --test sdk-rs/safe-cex-exchange/scripts/cancellable-futures.test.cjs`
and `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange
--all-features --test cancellable_future_inventory --jobs 2`. CI runs both in the mandatory
`Audit cancellable future classifications` step. The ordinary core test matrix
also discovers the production classification test. Node uses only built-ins;
there is no root npm dependency, compiler selector or missing-package skip.

The Rust AST walker starts at the production `src/lib.rs` module graph. It
includes every potentially enabled platform/feature branch and excludes branches
that cannot be production with `test=false`. Filenames do not decide whether
code is production. It sees async functions/blocks/closures, await and poll_fn,
explicit Future polling, future-bearing signatures (including callback factories
and generic bounds), host/owned spawns and unexpanded macro review sites.
Macro expression arguments are inspected; unknown macros require classification.

The original pass discovered **107** creator functions. Inspecting future inputs
and bounds adds **five** synchronous callback/registry entrypoints, bringing the
pre-merge inventory to **112 candidates**. That is discovery, not 112 live tasks,
112 tested drop paths, or a semantic proof of cancellation safety. The merged inventory discovers **115 candidates**: three new HTTP/socket operation wrappers. Current classifications are 12 guarded, 43 delegated, 33 immediate, 25 without an owned latch, and **two open findings**. The two findings make the production gate fail,
even though they are routed to core. A discovery count must never hide failures
behind exit zero.

Each entry in `tests/cancellable-futures.json` records the producer's canonical
signature/body hash and exact boundary list. The producer includes guard creation
before async/pin/spawn. Moving a guard into an unpolled future therefore requires
a new review. Guarded entries additionally pin their implementation source files,
name a real production Drop implementation and name real, nonempty #[test]
functions. This deliberately coarse helper-file hash also flags formatting or
unrelated helper changes: review them rather than refreshing blindly. It catches
removing cleanup while leaving the creator unchanged. A guard name is not proof.

The classifications explain the ownership decision. `delegated` names the
existing lifetime owner or callback contract; it does not prove arbitrary host
or venue implementations honor that contract. `immediate` includes synchronous
factory registration without polling a callback. `no-owned-latch` means this
creator has no independent pending registration/reservation, not that abandoning
it reverses completed external side effects. `finding` requires a routing record
and a permanent witness and blocks the gate until repaired.

Shared waiter registrations are detected both as field pushes and through local
RefMut aliases of wakers/waiters. They cannot be classified as no-owned-latch.
The public API witnesses in `tests/cancellation_findings.rs` measure cancelled
PositionFetch and proxy Flight callers retaining a Weak-sentinel waker until the
shared TASK drops. Core owns both production repairs; flip the existing
witnesses when consuming them. Do not duplicate or privately repair production.

This is a syntax/ownership-review gate, not Rust type inference or macro
expansion. Ambient/external aliases, callback implementations and semantic
effects still require review. Known macros are parsed where possible; unparsed
future-bearing arguments are review sites. New opaque macro sites fail closed.
Runtime TASK-drop tests remain necessary: dropping a waiter while a harness keeps
driving its task is not evidence that disposal cancels that task.

The exporter is an explicit discovery aid:
`CANCELLABLE_DISCOVERY_OUTPUT=/tmp/core-futures.json cargo test
--manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange
--test cancellable_future_inventory export_cancellable_future_discovery
--jobs 2 -- --include-ignored`. It never updates classifications. Default
include-ignored runs export to a temporary file and clean it up. The mandatory
gate runs classification, not this exporter; CI mutation tests reject replacing
the gate with export-only execution.

Controls reject new/stale/duplicate creators, changed boundaries, moved pre-pin
guards, removed Drop implementation, missing witnesses, unknown classifications,
blank reasons, waved-through waiter registration and routed-but-open findings.
AST fixtures additionally cover statement/expression macros, future-in-vector
arguments, borrowed waiter aliases, compound test cfg and file-relative #[path] modules. They are syntax
fixtures, not compiling production behavior mutations. The production gate
currently exits 101 naming the two confirmed findings; helper controls passing
do not make that red gate green.

Merge reconciliation at 56e818d52b reviewed the three new creator bodies and two changed HTTP bodies. The WebSocket helper-file digest changed only for start_message_operation and its tests; SocketTaskGuard and ReopenResyncGuard bodies are unchanged. Both public waiter-retention witnesses still reproduce on this merged tree, despite an earlier announcement of fixes. The mandatory classification gate remains intentionally nonzero on those two findings; nothing is waived.

Verification after merge: four AST fixtures, 11 Node classification controls, seven socket task-drop/error tests and both public retention witnesses pass. Strict host clippy for the two gate/witness integration targets passes. The production gate executes all guard/witness validation and exits 101 specifically at OPEN production cancellation FINDINGS, naming wait_for_flight and PositionFetch::wait. This is a functioning red gate, not a merge-ready green claim.

Witness enablement is fail-closed: ignored tests and cfg_attr are rejected, and conditional tests/modules are accepted only for cfg(test) and the explicitly enabled health feature in the mandatory all-features lane. Other feature/target cfg needs separately demonstrated execution rather than being counted from AST presence. This pins the reviewer-measured metadata/default-matrix bypass where adding #[ignore] was accepted without changing any production fingerprint. The separate existing core lane includes ignored tests, so this did not remove the witness from all CI execution.

Witness-bypass controls: adding #[ignore] to the actual referenced core_snapshots witness now fails classification at the enabled non-ignored witness assertion (exit101). Substituting an ignore-permissive validator compiles and fails cancellation_witness_must_be_enabled_and_not_ignored; restored baseline passes. The two health TASK-drop witnesses were explicitly run with --all-features and passed. Five discovery/enablement fixtures pass; full classification reaches only the two OPEN waiter findings. Strict scoped all-features host clippy and 234 wiring assertions pass.

Review follow-up corrected its feature-execution concern: the existing mandatory Check exchange core parity and lifecycle lane runs all-features/all-targets/include-ignored and its own CI test pins those flags. No duplicate execution lane is added. Repair review approves witness enablement; overall DO NOT MERGE remains the two independently reproduced core waiter findings.

At aa632b5578 the original two public waiter witnesses flip at their named assertions: WAITER Drop releases the sentinel immediately. The committed regressions now assert immediate release and zero wakes after TASK Drop. Both owners are guarded by per-future Registration in waiters.rs; the current 115 discoveries classify as 14 guarded, 43 delegated, 33 immediate and 25 without an owned latch, with zero findings. Those are classification counts, not 115 proven-safe creators. The earlier two-finding results above describe the pre-repair tree.
