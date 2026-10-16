# TWAP lifecycle gate evidence

Gate code/test SHA: `6dbe5a4c9196e23aa7b00b12a0dedb5a27602f7b`. Implementation SHA: `7058c738cd` followed by receiver-identity proof `6dbe5a4c91`. Actual initial parent: `cc3d25150b320656935009943f72d17b27e380a0` (reviewed integration). Branch: `feat/rust-webworker-twap-lifecycle`; no unreviewed external branch is an ancestor.

## Rows moved

**1 pending -> ported, 10 pending -> divergent**. All eleven pending TwapInstance rows now have real symbols in `src/twap_instance.rs`; its three prior helper rows keep their existing mappings and notes. The supplied callback field is ported; lifecycle rows use specific MIXED or CHOSEN buckets for shared String/type constraints, native host/future ownership, typed spread/snapshot fields and error contracts. Python 3.12 `tomllib` compared symbol identities/statuses across 38 tracked worker manifests against the initial parent, stopping at file 38 / row 222: **111 ported / 64 divergent / 47 pending -> 112 / 74 / 36**. This census is not proof of complete worker/runtime parity.

## Measured gates at the gate SHA

| Gate | Measured exit |
| --- | --- |
| Scoped cargo check, all targets | 0 |
| Five native/oracle lifecycle tests and one retained source-floor test | 0 |
| Extractor check, 7341 symbols / 512 source files, zero drift | 0 |

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test twap_lifecycle --test twap_lifecycle_source --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

The manifest used `parseManifest` / `serializeManifest` and then the writer `parity:extract` before implementation commit (exit 0, inspected output `Updated 0 manifests; 0 added, 0 orphaned symbols.`). The checks at both `7058c738cd` and `6dbe5a4c91` reported `Would update 0 manifests; 0 added, 0 orphaned symbols.` with exit 0. Fresh Rust check/tests also exited 0 at both commits. The second measurement follows the added direct callback receiver-pointer assertion; the earlier measurement is not presented as covering changed tests. An exploratory uncommitted test run exited 101 due to unused `support/runtime.rs` fields/advance under `-D warnings`; initializing the real fixture clock through `advance(1000)` fixed this without weakening lints. Subsequent exploratory tests passed, but the gate table uses committed measurements only.

## Native proof and stopping floors

The native target executes **252 independent cases** from the fixed product of four statuses, seven size arrays, three placement outcomes and three interruptions, stopping on case 252. It directly asserts status, execution-count bits, remaining-array cardinality/values, placed native order values/venue identity, manager membership, selected result, timer counts/wait bits, rejected-slice logging, fired paused/completed guard and cleanup marker before source profile comparisons. Source snapshots are projected onto lifecycle fields (status, ordersSizes, ordersExecuted, nextOrderAt, startedAt, markForCleanup); unrelated ordinary Market/options metadata is outside this profile claim. No decoded state is re-encoded for a roundtrip gate.

Additional native/source witnesses cover constructor input/callable/manager identities, exact callback receiver pointer, supplied-state alias, empty/whitespace IDs, ID generation before factory error, callback-before-post ordering, callback state replacement/removal, committed state on callback/post errors, two/three wall-clock reads for resume, raw negative delay, main/slice timer clear order, already-completed terminate, **8 limit-price cases**, and **12 deadline cases** (eight captured-action/stopped-state combinations plus missing, filled, NaN remainder and NaN pct), stopping at case 8 and case 12 respectively. A reentrant RNG/pending rejection witness proves inputs are captured before RNG mutation, execution count after the idle callback, placement options after callback, and failed-slice symbol at rejection time. Exact code/test receiver comparison uses raw pointer equality without dereference; source asserts `this === instance`.

The lifecycle oracle executes actual TwapInstance, TwapManager, safe-math, enums and installed Lodash. The policy oracle's imported ChaserOrderInstance/manager are dependency fixtures; Rust constructs a real ChaserOrderInstance and asserts its native option fields/ID and remaining-size policy outcomes. This does not port or prove the still-pending production chaser manager, networking, or worker message routing.

## One deliberate break per new symbol

All controls below temporarily changed only `src/twap_instance.rs` from the gate SHA, ran a named `twap_lifecycle` test, restored the original source in `finally`, and reran the same test. A source-to-working-tree `git diff --exit-code 6dbe5a4c91 -- sdk-rs/safe-cex-webworker/src/twap_instance.rs` exited 0 after restoration. Eleven controls are the stopping floor, not an exhaustive mutation inventory. **No survivor and no compiler-blocked control occurred.**

| Mutation | Named native assertion | Broken exit | Restored exit |
| --- | --- | --- | --- |
| `self.managed.borrow().state.clone() -> Rc::new(RefCell::new(self.managed.borrow().state.borrow().clone()))` | **TWAP native constructor state identity** | 101 | 0 |
| `            update_state_callback, ->             update_state_callback: { let _ = &update_state_callback; Rc::new(\|_,_\|Ok(())) },` | **TWAP native stored callback identity** | 101 | 0 |
| `.filter(\|id\| !id.is_empty()) -> .filter(\|_\| true)` | **TWAP native empty ID fallback** | 101 | 0 |
| `(self.update_state_callback)(self, state)?;         self.host.post_state(self.host.manager().serialize_state()) -> self.host.post_state(self.host.manager().serialize_state())?;         (self.update_state_callback)(self, state)` | **TWAP native callback before post** | 101 | 0 |
| `self.order(amount, price) -> self.order(amount + 1.0, price)` | **TWAP native placed amount** | 101 | 0 |
| `if action == TwapTimeoutAction::Market -> if action == TwapTimeoutAction::Next` | **TWAP native captured slice action** | 101 | 0 |
| `matches!(status, TwapStatus::Completed \| TwapStatus::Paused) -> matches!(status, TwapStatus::Completed)` | **TWAP native paused/completed fired guard** | 101 | 0 |
| `status: Some(TwapStatus::Paused), -> status: Some(TwapStatus::Running),` | **TWAP native pause status** | 101 | 0 |
| `let delay = next - self.host.now_ms(); -> let delay = next - self.host.now_ms() + 1.0;` | **TWAP native future resume delay** | 101 | 0 |
| `            self.host.manager().remove(&self.id());             Ok(()) ->             Ok(())` | **TWAP native terminate manager removal** | 101 | 0 |
| `mark_for_cleanup: Some(true), -> mark_for_cleanup: Some(false),` | **TWAP native cleanup marker** | 101 | 0 |

Control tests were `constructor_callback_aliases_and_failure_cutovers` (class/callback/constructor/update), `limit_prices_and_chaser_deadlines_pin_native_values_before_profiles` (placement/deadline), `native_lifecycle_matches_252_source_interleavings` (start/cleanup), and `pause_resume_clocks_and_termination_preserve_source_order` (pause/resume/terminate). Commands used the cargo test command above restricted to `--test twap_lifecycle`, then `TEST_NAME -- --exact`. The start-guard scratch wrapper initially expected `status before`; the mutation actually failed earlier at **TWAP native paused/completed fired guard** (two orders vs independent expectation one), exit 101, and restoration passed 0. The recorded assertion was corrected from the actual log; no mutation was dropped or claimed surviving.

## Explicit boundaries and review

State replacement uses owned nested Vec/Market snapshots; native retained old state stays independent while the actual source retains its array alias. The typed patch supports the five fields used by this lifecycle, not every Partial<TwapState> key/undefined/unknown property. Shared native String contracts, canonical Result errors and explicit host primitives are mapped in per-symbol buckets. Host setTimeout enqueues (never runs inline), preserves original binary64 delay and timer identity/truthiness, and owns platform delay normalization. Hosts must release retained callback cycles at teardown. Native future polling and exception reporting are not full JS microtask/unhandled-rejection/GC semantics.

Source behavior intentionally retained: pending placement rearms after pause/terminate while preserving changed status/removal; rejected slices still count; terminate does not cancel tracked chasers and already-completed tasks skip timer clears. New bindings are mandatory host dependencies, not fabricated manager implementations or automatically wired browser/venue execution.

Read-only sub-agent `context_review` reviewed design, implementation, fixtures and mappings at `7058c738cd`, plus the direct receiver proof at the gate SHA; no blockers remained, and it ran no gates. It identified borrows across host RNG/store calls, resume's live reread, the state patch restriction, exact post envelope and insufficient receiver-identity assertion. Each was corrected or explicitly bucketed before the final gates.

## Not run

No workspace-wide build/full monorepo suite, host clippy, wasm clippy/build, browser/SharedWorker deployment, live venue/network test or CI rerun. Scoped compilation does not prove workspace compilation, and source-only tests do not establish native parity outside the assertions named above.

## Refreshed integration merge gates

Fetched and merged reviewed integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444`, producing merge `aa14aa17a2c3e08ce5d6d79b0765566205f4c59a`; no conflicts occurred. At that merge, `git diff --exit-code 6dbe5a4c91 HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **exit 0**. Webworker source/tests are byte-unchanged. Rust dependency sources/manifests and Cargo.lock changed, so the scoped all-targets check and both selected targets (five native/oracle tests plus one source-floor test) were rerun at **`aa14aa17a2c3e08ce5d6d79b0765566205f4c59a`**, each **exit 0**. Extractor inputs changed and its fresh check also exited **0**, with inspected text `Would update 0 manifests; 0 added, 0 orphaned symbols.` and 7341 symbols / 512 source files. Original mutation controls remain bound to `6dbe5a4c91` and were **not rerun after the dependency merge**.

Tomllib comparison to merged integration again stopped at 38 worker manifests / 222 rows: exactly **1 pending -> ported, 10 -> divergent**, leaving **36 pending**. No imported worker movement is claimed. Read-only sub-agent evidence review found the remaining-size wording broader than the direct assertions; it now says native option/ID assertions and remaining-size policy outcomes. Review confirmed all eleven runtime failures/restorations and original gate SHA scope; it ran no gates. The delivery documentation commit changes only this tracked evidence file.

## Final merge before push

A second fetch merged reviewed integration `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a`, producing `f19b9a440386d7ae7a1d6df3831eff532e08ad84` with no conflicts. Scoped `git diff --exit-code aa14aa17a2 HEAD` over webworker source/tests, seven Rust dependency sources/Cargo/build inputs, workspace Cargo.lock/config and the TS oracle sources measured **exit 0**. The crate source/tests comparison to `6dbe5a4c91` also measured **exit 0**. The final scoped all-targets check, both selected test targets (five native/oracle tests + one source-floor test), and extractor check were each measured fresh at **`f19b9a440386d7ae7a1d6df3831eff532e08ad84`**, all **exit 0**; extractor output again reported 7341 symbols / 512 source files and zero drift. Controls were not repeated and remain attached to `6dbe5a4c91`.

Final tomllib comparison against this integration parent stopped at 38 manifests / 222 rows, confirming **1 pending -> ported, 10 -> divergent**, and **36 remaining pending**. The final evidence-only commit changes no gate input; tracked code/tests remain byte-unchanged. There is no unreviewed external branch ancestor.
