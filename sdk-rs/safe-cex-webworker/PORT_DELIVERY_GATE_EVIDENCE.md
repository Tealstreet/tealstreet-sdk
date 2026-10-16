# Worker port delivery gate evidence

Code and test gate SHA: `05c3ec4ef35e197f373a5fc6b5cf9f0674480432`. Actual branch parent: `2c17c1976a94ff9b919ad0ac534dd77ea704960f` (reviewed integration `2c17c1976a`). Branch: `feat/rust-webworker-port-delivery`. No external unreviewed branch is an ancestor.

## Rows moved

**4 pending -> ported, 1 pending -> divergent (CHOSEN)**: config registration, state cleanup, everywhere cleanup, per-port delivery, and broadcast respectively. Python 3.12 `tomllib` compared symbol identities/statuses in all 38 tracked webworker manifests against the parent, stopping at file 38 / row 222: **107 ported / 63 divergent / 52 pending -> 111 / 64 / 47**. This is the tracked worker manifest census, not proof of complete runtime parity. The two ExchangeWorker registries and `start` remain pending with real implementation-gap notes; no substitute targets were added.

## Measured gate exits

All commands below used the code/test gate SHA above, with uncommitted mutations only where explicitly named. The manifests were changed using `parseManifest` / `serializeManifest`, followed by the writer `parity:extract` (exit 0; output `Updated 0 manifests; 0 added, 0 orphaned symbols.`). Pending-note-only edits also use the writer. The extractor check output was inspected, not inferred from yarn's exit alone.

| SHA | Gate | Exit |
| --- | --- | --- |
| `05c3ec4ef3` | scoped cargo check, all targets | 0 |
| `05c3ec4ef3` | worker_ports target, four native/oracle tests | 0 |
| `05c3ec4ef3` | extractor check: 7341 symbols / 512 source files, zero drift | 0 |

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test worker_ports --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

The check reported `Would update 0 manifests; 0 added, 0 orphaned symbols.` A scratch wrapper initially expected the writer's `Updated` wording and exited 1 after the extractor itself exited 0; that wrapper assertion was corrected against the actual output. It is not a failed extractor gate. An exploratory uncommitted test run earlier exited 101: three profile comparisons rejected JSON integer vs binary64 Number representation; typed `Profile`, `Routing`, and `TruthRow` decoding fixed these comparisons before the committed gate. Native identity/state/bit assertions were retained.

## Deliberate breaks and restorations

Every control below mutated only `src/worker_ports.rs` from `05c3ec4ef3`, ran the named target with the cargo test command above plus `TEST_NAME -- --exact`, restored the original source in a `finally` block, and reran that target. These five controls are the stopping floor, not an exhaustive mutation inventory. None survived. A source-to-working-tree `git diff --exit-code 05c3ec4ef3 -- sdk-rs/safe-cex-webworker/src/worker_ports.rs` exited 0 after the final restoration.

| Helper / mutation | Native assertion that failed | Broken exit | Restored exit |
| --- | --- | --- | --- |
| `handlers.borrow_mut().push(handler); -> let _ = (handlers, handler);` | **PORT native duplicate config cardinality** | 101 | 0 |
| `.retain(\|(candidate, _)\| !Rc::ptr_eq(candidate, port)); -> .retain(\|_\| true);` | **PORT native cleanup deletes state** | 101 | 0 |
| `self.ports.borrow_mut().shift_remove(&key); -> let _ = self.ports.borrow_mut().get(&key);` | **PORT native cleanup deletes empty group** | 101 | 0 |
| `THROTTLE_HIDDEN_MS: f64 = 2500.0; -> THROTTLE_HIDDEN_MS: f64 = 2499.0;` | **PORT native first tail delay** | 101 | 0 |
| `sub_id.filter(\|id\| !id.is_empty()) -> sub_id.filter(\|_\| false)` | **PORT native destination 0** | 101 | 0 |

The initial config mutation `handlers.borrow_mut().push(handler)` -> `let _ = handler` was **inconclusive**: compile exit 101 (`-D unused-variables`, unused `handlers`), before a test could run; restoration exit 0. It was retained and replaced with the compiling `let _ = (handlers, handler)` mutation, which failed the native cardinality assertion at runtime. This compiler rejection is not counted among the five discriminating controls.

Targets: `config_registration_preserves_duplicates_and_live_callback_identity`, `leading_trailing_visibility_and_current_state_lookup`, `falsey_payloads_zero_timer_and_backward_wall_time`, and `broadcast_live_arrays_sub_ids_failure_cleanup_and_reentrant_state_aliases`. Fixed expectations are 3 registered handler identities / 6 calls; 6 timeline deliveries / 5 timer effects; 12 enumerated truthiness cases; and 15 routing attempts. Every native pointer identity, timestamp bits, state flag, pending alias and map membership assertion precedes its source/profile comparisons. Typed source profiles are decoded directly; no decode/re-encode gate is used.

The oracle evaluates the actual TS helper prefix, stopping immediately before the `start` declaration, with controlled ports and timer globals. Witnesses include visible/disabled delivery, last-value replacement, pre/post timestamp ordering, cancellation, current-state timer lookup, retained old-state alias writes after reentrant replacement, falsey pending retention (including NaN), timer ID zero, backward wall time, duplicate ports, mapped cross-group selection, absent-group precedence, live array appends, replacement-array iteration after post failure, and same-name distinct port identities. These are controlled-host floors, not automatic browser wiring or all host schedules.

## Chosen boundary and review

`broadcastWithThrottle` is CHOSEN divergent: own-key IndexMap lookups with UTF-8 route keys replace the source's plain-object prototype/UTF-16 property ABI, reusing typed registries instead of a second dynamic object subsystem. The native `constructor` sub-ID misses and broadcasts to the group; the actual TS source selects an inherited non-port function and reads its failing postMessage property, followed by cleanup. This difference is explicitly witnessed and not counted as a mutation survivor. The other four helpers preserve the declared port/timer host contract; opaque Rc messages can retain UTF-16 and reference identity.

Read-only sub-agent `context_review` reviewed design, code, fixtures, mappings and typed profile fixes at the code gate SHA, found no blockers, and ran no gates. Review prompted live-array iteration, Rc message carriers, pure JS truthiness and timer-clone identity requirements. Timer hosts must enqueue callbacks rather than run setTimeout inline; Result models synchronous postMessage throws. Hosts must release retained callbacks at teardown: an Rc registry -> host -> callback -> registry cycle is not garbage-collected automatically. Automatic worker lifecycle/start wiring and GC equivalence are not claimed.

## Not run

No workspace-wide build, full monorepo suite, host clippy, wasm clippy/build, browser/SharedWorker deployment, live venue tests, or CI rerun. Scoped compilation does not establish workspace compilation.

## Delivery merge proof

Fetched and merged reviewed integration `ebcb51ffa55881e2e5a745c0d419028b0632da7d` into this branch, producing `aac2523f746e062ae57837a19613c75bcc2d5d5f`. No conflicts occurred. At that merge SHA, `git diff --exit-code 05c3ec4ef3 HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **exit 0**. A separate scoped dependency/input diff covering the webworker/dependency Cargo manifests and sources, workspace Cargo.toml/lock/config, and TS oracle source also measured **exit 0**. Rust check/test and mutation inputs are byte-unchanged, so their exits at `05c3ec4ef3` stand; those gates were **not rerun** after this merge.

Extractor inputs and pending manifest notes changed, so `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` was rerun at `aac2523f746e062ae57837a19613c75bcc2d5d5f`: **exit 0**, inspected text `Would update 0 manifests; 0 added, 0 orphaned symbols.` (7341 symbols / 512 source files). The writer after the three pending notes exited 0 before documentation commit `38a527dbe9`. Tomllib comparison against the merged integration parent again stopped at 38 manifests / 222 rows and measured exactly **4 pending -> ported, 1 -> divergent**, leaving **47 pending**. No imported worker census movement is claimed.

The evidence-only delivery commit preserves the gate source/tests and is committed at a tracked crate path. No external unreviewed branch was merged. Sub-agent evidence review confirmed the original measured claims and disclosed inconclusive attempt; it ran no gates.
