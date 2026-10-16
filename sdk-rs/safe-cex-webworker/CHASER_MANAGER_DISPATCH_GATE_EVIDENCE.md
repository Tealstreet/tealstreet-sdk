# Chaser manager membership/dispatch gate evidence

Code/test/manifest gate and control SHA: `ee3cada10b78079a800037381a5dbad59d3caa3e`. Actual initial parent: `f0cf136efbc17b93fe52369e71342e620dd925ff` (integration). Branch: `feat/rust-webworker-chaser-manager-state`. No external unreviewed branch is an ancestor. This is a coherent eight-row portion of ChaserOrderManager, not a complete manager/supervision port.

## Rows moved

**1 pending -> ported, 7 pending -> divergent**. The live instances getter is ported. Class/add/singleton use individual CHOSEN ownership/linking/thread scopes; supervisor-error, exchange cleanup and two removal methods have individual MIXED String/typed error/state/dependency boundaries. Every row has a real exported native target and its own reason; existing helper targets/statuses were unchanged.

Independent tomllib comparison of the fixed eight symbol identities against the parent, stopping at **file 38 / row 222**, produced **112 ported / 80 divergent / 30 pending -> 113 / 87 / 22**. The fixed 38-file/222-row floor and eight identities were asserted independently of measured profile sizes. Census is not semantic proof.

Five bodies remain pending: tryPlaceOrders, findLostPlacement, processInstance, cleanupInstance and emitComplete. The shared partial cancellation and optional fill-reconciliation gaps are not bypassed. Required ChaserSupervisor methods construct their callee prefixes/futures, with no default implementation or venue-local empty-success shim. New consumer methods do not claim automatic worker/startup wiring. `.aimux-brief.md` was absent; `.aimux-next.md` cited nonexistent venue paths. The actual existing worker paths/crate were used, with the requested fresh integration parent.

## Completed gates at the gate SHA

| Gate | Exit | Output |
| --- | --- | --- |
| Scoped cargo check, all targets | 0 | Finished dev profile |
| Dispatch target + retained chaser_manager target | 0 | 4 + 1 tests passed, none failed |
| Extractor check | 0 | 7341 symbols / 512 files; 0 updated, added or orphaned |

```sh
/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test chaser_manager_dispatch --test chaser_manager --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Manifest edits used parseManifest/serializeManifest, followed by the writer parity:extract: exit 0, inspected text `Updated 0 manifests; 0 added, 0 orphaned symbols.` Check output also explicitly said `Would update 0 manifests; 0 added, 0 orphaned symbols.` Shim exit alone was not used as proof. Cargo commands ran sequentially, with jobs 2.

## Native assertions and source scope

All native assertions precede source comparisons. Direct native witnesses inspect the actual original instance Rc, stored list Rc and external reorder, repeated addition of the same Rc with an independent count of three, fresh/default versus singleton identity, deferred startup failure, real stopped/stop-reason/current-ID/ever-tracked state and native listener/callee effects. No decode/re-encode or roundtrip assertion. The real native ChaserOrderInstance, BaseExchange and DefaultStore are invoked.

Four bounded families are covered: live membership/add; supervisor failures; exchange cleanup; remove/keep. Supervisor loop stops at **iteration 10** (cleanup rejection), covering Error versus owned string/null/array/object coercion, already stopped, listener stopping reentrantly, log throw, error-listener throw and awaited cleanup failure. Exchange cleanup snapshots pointer matches, preserves duplicate references, excludes a new member appended during cleanup and reads live stopped flags. Its warning-throw case commits one stopped state and skips cleanup/later entries. Removal retains the first selected same-ID object through callback list changes; keep clears IDs before disposal, re-finds the pointer index afterward, removes one current occurrence and calls completion once after splicing, without cleanup. The native claim assertion proves already-claimed state; the completion effect's independent list count proves completion construction follows splicing, not an independent proof of exact claim timing.

Actual TS ChaserOrderManager, ChaserOrderInstance, constants, clone and enums are executed. Three heavy callees are overridden with dependency fixtures in the TS loader and mandatory native host fixture: processInstance, cleanupInstance and emitComplete. Venue/emitter/store dependencies are fixtures on the source side; this does not execute actual cancellation/fill reconciliation or the source production Store. The consumer logic is actual source and actual Rust; missing body parity remains unclaimed. The retained helper test stops at source scenario 192 / transition 7488. Existing native helper source prefix was independently proved byte-unchanged against the initial parent before these gates.

## Four family discrimination controls

Per the current `.aimux-next.md`, one mutation per new row family, stopping at **iteration 4** (remove/keep). These are temporary uncommitted mutations of the gate SHA, not committed production changes. Each compiled and failed its named native assertion with exit **101**; restored same selected test exited **0**. No survivor or compiler-blocked control occurred among these four. The final source comparison to the gate SHA exited 0.

Commands were the cargo test command above restricted to --test chaser_manager_dispatch, followed by TEST_NAME -- --exact:

| Family | Mutation | Named native assertion | Mutant exit | Restored exit |
| --- | --- | --- | --- | --- |
| list-and-add | `        self.chaser_order_instances             .borrow_mut()             .push(instance.clone());         let processing = supervisor.construct_supervision(self.clone(), instance.clone());  ->         let processing = supervisor.construct_supervision(self.clone(), instance.clone());         self.chaser_order_instances             .borrow_mut()             .push(instance.clone()); ` | **MANAGER native add visible before process** | 101 | 0 |
| supervisor-error | `if !instance.is_stopped() {                 instance.mark_complete(Some(&format!("Chaser {phase} failed"))); -> if false && !instance.is_stopped() {                 instance.mark_complete(Some(&format!("Chaser {phase} failed")));` | **MANAGER native supervisor stopped cutover** | 101 | 0 |
| exchange-cleanup | `.filter(\|instance\| Rc::ptr_eq(&instance.exchange, &exchange)) -> .filter(\|instance\| instance.exchange.base().name == exchange.base().name)` | **MANAGER native exchange pointer filter** | 101 | 0 |
| remove-and-keep | `instance.set_current_order_ids(Vec::new());         instance.dispose(); -> instance.dispose();         instance.set_current_order_ids(Vec::new());` | **MANAGER native keep clears before dispose** | 101 | 0 |

Selected tests, respectively: list_registration_and_detached_startup_error_have_live_identity; supervisor_error_prefix_preserves_live_flags_and_failure_cutovers; exchange_cleanup_snapshots_pointer_matches_and_retains_warning_cutover; remove_and_keep_select_identity_and_recheck_index_after_disposal. These controls are a four-family floor, not separate mutations of every getter/singleton/row or an exhaustive mutation census.

## Explicit boundaries and review

Native dense Rc/RefCell list/type contracts replace arbitrary JS array/property/prototype mutation; outstanding borrows must be released before calls. Mandatory hooks execute source-like async construction prefixes immediately and capture business errors in returned futures. Native panics are not JS promise rejections. Host owns polling, detached-error reporting and teardown/release of retained future/callback cycles; no JS microtask/GC equivalence or automatically installed supervisor is claimed. Thread-local singleton initialization is lazy and per host thread, whereas source initialization is eager per module/realm. SupervisorFailure distinguishes canonical Error values from owned JSON, excluding arbitrary Error prototype tests/getters/toPrimitive/unknown throw identity. Existing helper/String/state snapshot boundaries remain independently mapped, including check_and_mark_if_complete's reentrant owned-Order selection difference.

Read-only sub-agent context_review reviewed design, implementation, fixtures and final eight notes. No remaining blockers. Review led to an explicit repeated-instance add witness and narrowed the completion-claim wording; review ran no gates/mutations. Exploratory --lib cargo check exited 0 (not the all-targets claim); both exploratory four-test runs exited 0. First standalone source oracle exited 1 because cross-realm arrays were compared by deepStrictEqual. It was corrected to an independent length-two expectation and strict reference comparisons at each index, then exited 0 before committing gate inputs.

## Not run

No workspace-wide build/full monorepo suite, all crate behavioral targets, host clippy, wasm clippy/build, browser/SharedWorker deployment, live venue/network tests or CI rerun. Only the named two Rust targets ran as baseline behavioral gates; control restorations ran the exact selected new test. Scoped compilation does not establish workspace compilation. Tests with fixture heavy callees do not prove those pending bodies.

## Integration merge-down and refreshed gates

Fetched integration `3205aab9c98d146c7c43e4f48b64f7d4b636cbf8` and merged it down at `b294ea5334a129ce8ba4ac9b7e90c2d5b21cf2ad` with no conflict. Scoped `git diff --exit-code ee3cada10b HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **0**, proving source/tests survived byte-unchanged. A separate diff over all seven Rust dependencies and source Chaser/clone/types oracle inputs exited **0**. Cargo.lock added a venue candle-cache dependency; other manifests/exemptions changed. Fresh scoped all-targets check, both selected behavioral targets (**4 + 1 passed**), and extractor check each exited **0 at `b294ea5334a129ce8ba4ac9b7e90c2d5b21cf2ad`**. Extractor text again reported 7341 symbols / 512 files and zero updates/additions/orphans. These are completed, SHA-bound measurements.

The four deliberate controls remain measurements at `ee3cada10b`; they were not rerun after the merge. Their mutation sites, native assertions and dependency implementations are byte-unchanged. Census against merged integration still gives exactly **1 pending->ported, 7->divergent**, ending at 113/87/22 across the same 38-file/222-row floor. Initial built parent remains `f0cf136efb`; additional ancestry is reviewed integration and own sub-agent-reviewed slice work, with no external unreviewed branch ancestor. Evidence-only delivery commits alter no gate inputs.

A final fetch merged integration `c1b0ce61f2247f5604b26ed5a00b142be75d90ef` at `a3dba7387a24719120fe0874d63b13af51fb3eb1`. Own source/tests and all Rust gate inputs (seven dependency crates, manifests/lock/config plus source oracle/package inputs) compared byte-unchanged to `b294ea5334`, exit **0**. Only manifest/exemption inputs changed, so only the extractor was refreshed: **exit 0 at `a3dba7387a24719120fe0874d63b13af51fb3eb1`**, inspected text `Would update 0 manifests; 0 added, 0 orphaned symbols.` Rust check/five tests remain actual measurements at `b294ea5334`, with their unchanged inputs proved; four family controls remain actual measurements at `ee3cada10b`. No new Rust/controls exits are attributed to the final merge. No CLAUDE conflict occurred.
