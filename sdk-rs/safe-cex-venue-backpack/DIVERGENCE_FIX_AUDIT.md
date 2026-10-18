# Backpack five-row production audit

The exact five de-duplicated codex-0efu4x assignment rows are enumerated in
`tests/divergence-rulings.json`. All five contain an observable production FIX.
Existing typed/prepared input restrictions are independently retained.

| Source row | Real binding observation | Repair |
| --- | --- | --- |
| getAccount | Drop the SDK waiter while the actual HTTP response is held; its later failure never logs | Shared eager continuation |
| fetchBalance | Drop the owned method waiter while the actual HTTP response is held; its later failure never logs | Shared eager continuation |
| BackpackPublicWebSocket | Held exchange correctly retains trailing throttle and candle receiver, but final drop leaves transport/health work active and shared disposed flag false | Silent complete native teardown on final destruction |
| fetchThrottledOrders | Actual malformed REST response rejects instead of entering source silent catch; dropped waiter skips Store commit | Silent production catch and shared eager continuation |
| executeBalanceFetch | Actual malformed collateral response rejects instead of entering source silent catch; dropped waiter skips Store commit | Silent production catch and shared eager continuation |

`tests/divergence_binding.rs` constructs the existing production owned adapter,
shared HTTP client, DefaultStore, public/private receivers and SDK Exchange
entrypoint. Only transport responses, socket events and time are controlled.
Original production compiled RED on six named assertions; the same probes pass
after repair. The held-receiver control already passed before repair and retains
original Rc identity, native ticker bits and listener callback checks.

`tests/divergence_source.cjs` executes maintained exchange/private-socket modules.
Its two held-response cases pin eager GET plus later catch/log without retaining
the caller's waiter. Six factory/rejection/Store-update cases pin private Promise
fulfillment despite failure. The existing full private-event corpus and every
native identity assertion remain unchanged; its generic helper Result is still
available to internal diagnostics, while production consumes it at the one bound
receiver. No parallel reconciliation algorithm is introduced.

The wrapper census classifies Backpack as **mixed native task**: creators can
start HTTP before boxed continuations. All 23 owned async Task entrypoints in
adapter.rs, adapter_orders.rs and adapter_runtime.rs now pass their existing
future to `safe_cex_exchange::eager::start_eager`. Existing creators, arguments,
request ordering and native outputs are unchanged. No serde result cell, per-venue
spawn helper, or polling shim is added. SDK conversion futures await this already
started operation; synchronous unsupported-mode helpers and inherited base
refusals have no deferred source prefix.

Final drop follows the coordinator's accepted native lifetime rule. It clears
source socket state, cancels ticker/depth/adapter tasks, then closes the shared
base's owned tasks, signers, sockets, listeners and Store. Explicit source dispose
retains its separately tested close order and source superclass-state behavior.
Drop suppresses cleanup errors and completes other cleanup paths regardless.
The native task and transport realms must honor cancellation/close; no real
credentials or network trading are used.

Independent production controls are in `tests/divergence_controls.py`: every
mutation must compile, fail the named behavior, restore exact source bytes and
pass the same probe. Full parity index, all-features cancellation, override/live
TypeScript, scoped strict host/wasm Clippy and formatting are required before READY.

The pending two-second all-market subscription and recurring tick timers also
retained strong Exchange frames, preventing final destruction. The shared source
subscription callback body is now reused by the native weak-owner timer. The
native recurring timer retains only a weak owner and invokes the existing
repeat_tick host capability. Real held-owner and final-drop tests cover both;
subscription/loop retention controls compile and prove the release assertions.

Coordinator-directed shared follow-up: final destruction enters the single
`safe_cex_exchange::native_final_drop` primitive, also adopted by BingX Drop in
this PR. Its core classification is native-only, with no per-venue AST exception
and no change to existing no-super/chosen-base rejection controls. The real core
NativeRuntime test verifies flag order, venue-error continuation, shared task
closure, and Store reset despite a notification error. The independent core
teardown fault must compile, fail that named test and restore GREEN.

Verification: all eleven independent production controls compiled and failed their
named assertion, then passed after exact restoration. Fourteen new binding/source
checks, all four existing disposal checks and eight startup/recovery checks pass.
Existing authored source corpora and all identity/alias guards are retained.
