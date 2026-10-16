# Polymarket ping timer cleanup

**2 pending→ported, 0 pending→divergent**. Python 3.14 tomllib census over seven manifests, stopped after polymarket.ws-public.ts.toml: **21/18/102**, 141 rows. Original direct integration parent `ed8336d5835189fd470040fbe62756ff1b60a0ae`; implementation commit `496e071111`; integration `e150dc59a2d5db2af0ff1875c92f291f2f32b9f4` merged down at `a033123358b2beda3f3b810a42bbb4a0cea731ce` with Polymarket source/tests/mappings byte-unchanged (diff exit0). Final code and gate SHA: **`8bbaee3c80152f03eadbcecc62aae129944f1058`**. Branch `feat/polymarket-ping-timer-clear-ed8336`; following evidence commits change documentation only. No additional unreviewed branch ancestry; inherited MEXC work is already in integration.

Ports the public and private `#clearPingInterval` method rows to real free functions in src/ws_timers.rs, over the shared runtime ExchangeTask cancellation boundary. The source declares `NodeJS.Timeout | undefined`; present opaque Node timer handles map to a present task, absent to None. Cancellation happens before clearing the caller-supplied slot. Absent and repeated cleanup do nothing; a throwing cancellation hook leaves the original slot present. These helpers do not claim WebSocket classes, constructors, scheduling, ping or full disposal; those rows remain pending.

Actual-source oracle extracts/transpiles each maintained original cleanup body, asserts the optional NodeJS.Timeout field declaration, and checks original handle/error identity and cancellation-before-clear at its host boundary. Native Rust assertions inspect cancellation count, timer identity17, retained Option and caught hook failure before source comparisons; no re-encoding or wire equality is used as proof. Independent expectation floors: **two venues × three ordered scenarios (absent/present/cancel-throws) × two calls**, stopped at private/cancel-throws/iteration1. Twelve native cleanup invocations. Each source capture performs12 cleanup invocations; the two Rust tests invoke that complete source capture independently, thus24 captured source invocations total. Independent cancellation counts per iteration: absent0/0, present1/1, failure1/2. This is a finite host-boundary witness, not a running socket or timer scheduler proof.

## Initial compile failure and repair

At a033123358, both timer production controls passed their0/101/0 proof, but scoped all-targets check exited101: the integration core introduced required `ExchangeRuntime::monotonic_now_ms`, which existing HTTP Runtime fixtures in tests/support/mod.rs and tests/public_reads.rs lacked. Later gates were not run at that SHA because the runner stopped at the failed check; failure output and control summary are tracked in evidence/ping-timer-clear/initial-compile-failure/.

8bbaee3c80 repairs both existing test runtimes with a default-zero Rc<Cell<f64>> clock, advanced when their ExchangeRuntime sleep and RateLimitClock sleep futures are polled. Two direct native fixture tests assert0→13→20. Fixed wall now_ms remains its original value. Existing HTTP request/response/error assertions remain unchanged. Static sub-agent review found no weakening. No production core or HTTP behavior was edited.

## Measured gates at8bbaee3c80

All commands used one Cargo at a time. Exact command arrays/results and raw output are tracked in evidence/ping-timer-clear/. Node oracle invocations use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Exit | Scope |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --all-targets --locked --jobs 2 | 0 | Polymarket and dependencies |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --locked --jobs 2 -- --nocapture | 0 | All18tests; zero unit/doc tests |
| cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket -- --check | 0 | Polymarket formatting |
| python3.14 sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py | 0 | 141rows,46public Rust items,zero exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Read output:7341symbols,512files,zero stale/added/orphaned |
| python3.14 sdk-rs/safe-cex-venue-polymarket/tests/ws_timer_clear_controls.py | 0 | Both row controls, each baseline0/mutation101/restored0 |

One real production mutation per new row, stopped after private: replace that wrapper's call to clear_ping_interval with a no-op. Each fails the named assertion **Polymarket native timer cancellation count:** with101, then returns0 when restored. Production source restored exactly. No survivors. Panics printed by the baseline failure-hook scenario are caught intentionally; the tests assert their native payload and slot retention and finish with exit0.

Sub-agent static review found no actionable cleanup mismatch and checked both actual targets, native/source floors and order of side effects; a separate review covered fixture-clock repairs. Reviewer ran no gates. No workspace/monorepo build, full core suite, host clippy, wasm build/clippy, CI rerun, live exchange call, socket scheduler test or PR creation was performed. MEXC's separately delivered two helper row moves are not counted as new Polymarket branch work.

## Final merge preservation

Fetched integration immediately before delivery and merged `61c78429c39fc37b14d3fb5d479572ec74ab9c29` at `d9d0a7592d8115b3870af4908f0cdd2619bedbc1`. Required `git diff --exit-code 8bbaee3c80152f03eadbcecc62aae129944f1058 HEAD -- sdk-rs/safe-cex-venue-polymarket/src sdk-rs/safe-cex-venue-polymarket/tests` exited0; extending it to venue mappings and all seven dependency src/Cargo.toml inputs also exited0. Cargo.lock changed only by adding indexmap to the unrelated Coinbase dependency list. The check18-test/mutation exits remain measured at8bbaee3c80, preserved by byte-identical relevant inputs, not claimed as freshly re-executed atd9d0a759. Refreshed extractor check atd9d0a759 exited0, with7341symbols/512files/zero stale/added/orphaned. SHA-bound raw output and preservation receipt are in evidence/ping-timer-clear/final-merge/.
