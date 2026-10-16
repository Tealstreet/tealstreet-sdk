# MEXC heartbeat/lifecycle method bodies

**10 pending → ported; 0 pending → divergent.** MEXC triple: **102 / 40 / 43** (ported / divergent / pending). [Census](evidence/ws-lifecycle/census.json) compares all six venue manifests to requested integration parent **`1f6e115b0cb3aec1c42860a0f97cf71bd84376d8`**; this is a census, not runtime proof.

Fresh branch `feat/mexc-ws-heartbeat-lifecycle-1f6e11` starts directly at that parent. No unreviewed private lane work was introduced as an ancestor. Before gates, integration `7fd5102999754d0a1d95910af1fb540c3bffb94e` was merged down.

## Callable body ports

Public: startHeartbeat, stopHeartbeat, handleClose, onOpen, ping, dispose. Private: startHeartbeat, stopHeartbeat, handleClose, dispose. Each has an individual real Rust function in src/ws_lifecycle.rs. The public start mapping also explains every HeartbeatHost trait/method/associated-type target as the source runtime/receiver boundary, without adding exemptions or claiming a class constructor.

Both stops call clearInterval even for an absent handle, then clear the property only after success. Both starts stop the old timer, invoke the receiver ping method, then unconditionally register the exact 8000ms interval. Scheduled callbacks read current disposed/connected flags. The timer host retains handles/callbacks independently until cancellation; registration does not hand timer ownership to a temporary method return.

Public open guards once, then calls start/reconcile/resubscribe/reconnected in source order. Public ping assigns the native performance Number before sending the original ping text. Close handlers stop before forwarding the unchanged event. Public disposal reads and removes the current handler after stop, before parent disposal. Private disposal reads the current refresh handle after stop, clears that timeout and retains its property. Synchronous failures propagate with original native identity and cut off subsequent effects; parent return values are discarded.

**Existing production composition remains FINDING.** The websocket classes/constructor are not registered by MexcExchange. The legacy `MexcPublicProtocol::on_open` still uses its partial ping/resubscribe path; the newly ported public_on_open is not claimed wired there. Native method ports are exercised at the injected host boundary. Whole websocket classes, timer-host implementation, parent implementation and live network behavior are not proved by this slice. Class FINDING notes now explicitly distinguish these new callable bodies from unresolved composition.

## Gates attached to SHA

All following exits were measured at **`1adc99c4df78e9ce6a3bcd56643f393b79b00918`**. [gates.json](evidence/ws-lifecycle/gates.json) retains exact commands, and each raw log is tracked beside it. Venue oracle environment uses the specified isolated TypeScript installation. Cargo commands ran sequentially.

| Gate | Measured result |
| --- | --- |
| Scoped cargo check, MEXC, all targets, locked, jobs2 | 0 |
| Scoped MEXC tests, locked, jobs2 | 0; 122 passed, zero failed; doc tests ran zero cases |
| Scoped cargo fmt check | 0 |
| Official MEXC-only invention/status checker | 0; 185 manifest rows, 156 public Rust items; zero venue exemptions |
| Actual Yarn parity:extract:check | 0; inspected output says would update 0 manifests, zero added/orphaned symbols |
| Ten isolated method-body control script | 0; shared ten-test baseline0, each compiled mutation101 and restored exact-test0 |

The first dirty-draft test compile failed101 due to a u64-vs-Option assertion in the test harness. It was fixed, and the focused dirty-draft run passed ten tests before later strengthening callback checks. These were development runs, not gates covering this SHA. The committed full venue run includes the strengthened callback assertions.

## Native assertions and controls

The oracle extracts and executes original TypeScript method bodies and the original heartbeat constant. Runtime timer, receiver child methods and parent calls are injected at explicit boundaries. Native tests directly inspect call order, field values, opaque interval/refresh/event/handler identities, original errors, conditional presence, current registry counts and native f64 ping timestamp bits. Reentrant cleanup replaces heartbeat/refresh/handler properties; tests require post-stop reads and no borrow retained during host calls. Source callback flags/errors are compared to native replay inputs/results. A fifth live callback throws the original failure and verifies its native identity, so callback failure propagation is witnessed.

There is no native encode/re-encode or roundtrip value comparison. Source Number bits are captured before oracle JSON output. Independent native root-tag sequences, fixed failure-point configuration and callback expectations supply counts; expected trace lengths are not taken from the produced native trace or source result.

| Isolated production missing-step/guard mutation | Named native failure | Baseline / mutation / restored exits |
| --- | --- | --- |
| `public.startHeartbeat` | `native lifecycle trace:public.startHeartbeat` | 0 / 101 / 0 |
| `public.stopHeartbeat` | `native lifecycle trace:public.stopHeartbeat` | 0 / 101 / 0 |
| `public.handleClose` | `native lifecycle trace:public.handleClose` | 0 / 101 / 0 |
| `public.onOpen` | `native lifecycle trace:public.onOpen` | 0 / 101 / 0 |
| `public.ping` | `native lifecycle trace:public.ping` | 0 / 101 / 0 |
| `public.dispose` | `native lifecycle trace:public.dispose` | 0 / 101 / 0 |
| `private.startHeartbeat` | `native lifecycle trace:private.startHeartbeat` | 0 / 101 / 0 |
| `private.stopHeartbeat` | `native lifecycle trace:private.stopHeartbeat` | 0 / 101 / 0 |
| `private.handleClose` | `native lifecycle trace:private.handleClose` | 0 / 101 / 0 |
| `private.dispose` | `native lifecycle trace:private.dispose` | 0 / 101 / 0 |

[Control summary](evidence/ws-lifecycle/controls/summary.json) plus 21 raw logs record the shared baseline and individual broken/restored runs. All ten faults compiled and reached their named runtime assertion. Each mutation changes only its selected function span and restores source in finally. No survivor was observed in this ten-control floor.

Coverage floors: **10 method bodies × 10 scenarios = 100** source fixtures; each root is called twice (**200** native root calls in the passing corpus). Per-method enumeration stops at scenario **9**, call iteration **1**. Whole corpus stops at fixture **99**, private.dispose/scenario9. Four dynamic disposed/connected combinations plus a fifth failing live callback run for each successful registration; callback enumeration stops at the fifth replay (disposed=false, connected=true, original ping failure). Source timing is simulated by captured callbacks: actual wall-clock cadence or autonomous timer progression is not claimed. Mutation enumeration stops after private.dispose. Census enumeration stops at mexc.ws-public.ts.toml, the sixth manifest. These are explicit floors, not complete venue coverage.

A sub-agent reviewed production bodies, source, native assertions, oracle, control script and mapping notes. A handler-read reentrancy issue and unchecked source tick metadata were fixed before committed gates; final review found no remaining actionable issue in this slice. Reviewer ran no gates.

## Not run

No full workspace compilation/tests, host clippy, wasm clippy, CI, Polymarket gates or real-network/timer execution were run. Existing class/composition FINDINGs remain; the other 43 MEXC pending rows remain pending.

## Final merge preservation

Last pre-push merge includes integration `1273115ab915ad59ed9b478c48ca6356d8ac03f0` at comparison head `66fd168caf8ee8a386be7b005bdfc96bd290e9d3`. [Receipt](evidence/ws-lifecycle/merge-preservation.json) proves MEXC src/tests plus helper scripts, target manifests, TS source, dependency sources/Cargo.toml, Cargo.lock and checker inputs unchanged (diff exit0). Venue-filtered exemptions remain the independent expected0. Prior scoped Cargo/test/control exits stand at `1adc99c4df78e9ce6a3bcd56643f393b79b00918`; no fresh scoped gate run at this merge head is claimed. A fresh global extractor at `66fd168caf8ee8a386be7b005bdfc96bd290e9d3` exited0 with zero manifest changes; exact command/output are tracked. Following evidence-only commit does not change gate inputs.
