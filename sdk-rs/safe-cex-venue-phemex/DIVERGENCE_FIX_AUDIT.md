# Phemex non-UTF-8 FIX-audit

Authority: exact `assignments/codex-0efu4x.json` rows under the coordinator's
`non-utf8-assignment-index.json`; `divergence-cluster-rulings.md` async ruling.
This closes two assigned rows, not the venue's other historical findings.

| Source owner | Verdict | Production observation |
| --- | --- | --- |
| `PhemexExchange.fetchTickers` | FIX | The typed SDK common `invoke` deferred the actual source member until waiter polling. An unpolled discarded waiter sent no request and lost the later error emission. Invoke the actual member before boxing and await its existing Promise on the owning runtime. |
| `BinancePrivateWebsocket#syncPositionsAndBalance` | ACCEPT | Already repaired on the installed production receiver: begin_sync resets the timeout, clears queued symbols and invokes the original parent read before return. The retained Promise finishes after waiter abandonment and silently catches Store errors. The generic helper's diagnostic Result is not a caller-visible production rejection. |

The wrapper census family is **deferred JS invocation**. Both common `invoke`
and `invoke_unit` now invoke before constructing their waiter, sharing the
existing Promise driver. No extra spawn or per-method scheduling helper is used.
All typed SDK methods using those helpers inherit the boundary; inherited
unsupported methods retain their original synchronous refusal construction.

The real constructed receiver probes observe the original client method with a
transparent forwarding observer: exact receiver, argument vector, return value
and thrown value pass through unchanged. HTTP interceptors still execute;
terminal transport dispatch follows their microtasks. Source `start` first awaits
initExternalPublicData, so the unit-family probe pins that initializer before
return and the subsequent parallel requests after pumping microtasks. It does
not pull transport work through an actual source await fence.

`binding/divergence_regressions.rs` tests SDK ticker prefix and discarded-waiter
error emission, unit initializer prefix, private receiver/parent/Store identity,
real two-settlement HTTP, pending Store commit and binary64 balance totals,
a failing original Store subscriber, post-await disposal, and pre-try clearing
when disposed. The maintained-source AST oracle checks ticker eager invocation,
original error and fallback references, the unit prefix and three reconciliation
continuations (commit, Store failure, disposal). Existing decoded model, raw
response, source corpus, cardinality and identity tests are unchanged.

`tests/divergence_timing_controls.py` mutates production independently for typed
and unit deferral, discarded ticker continuation, private prefix deferral,
visible reconciliation rejection, missing reconciliation continuation, and the
post-account disposal guard. Every selected test must compile and fail at its
named assertion, then pass after an exact byte restoration. Compiler errors are
not RED evidence. Logs are under `target/phemex-divergence-controls`.

Run from sdk-rs (pinned toolchain; use --jobs 2):

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/node_modules/typescript cargo test --locked --jobs 2 -p safe-cex-venue-phemex --lib divergence_regressions
python3 safe-cex-venue-phemex/tests/divergence_timing_controls.py
```

Required final gates: full parity index; all-features cancellation inventory and
Node controls; live override contract; scoped fmt check; host all-targets and wasm
lib strict all-features clippy; full Phemex suite. Per-row rulings are recorded in
`tests/divergence-rulings.json`.
