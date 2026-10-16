# Polymarket disposal rows

**3 pending→ported, 0 pending→divergent**. Python3.14 tomllib census over seven manifests, stopped after polymarket.ws-public.ts.toml: **24/18/99**,141rows. Branch `feat/polymarket-disposal-17b6` began directly at requested integration parent `17b6bcd0db760cfb175d1f5e21e581b5d29bd2a5`; implementation commit b303bf3cbe. Integration `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e` merged down at9b177d63798026522c999ad3325c3ca59e933549; crate source/tests/mappings survived byte-unchanged (diff exit0). Final code/gate SHA **`5d91da4f5db0a1e42c0b6381d2b39b7d8b8bacd8`**. Later evidence commits change documents only. No additional unreviewed branch ancestry.

Ports actual method bodies for `PolymarketExchange.dispose`, `PolymarketPrivateWebSocket.dispose`, and `PolymarketPublicWebSocket.dispose` into src/disposal.rs. Exchange delegates to the shared parent boundary; private clears its timer before parent disposal; public also clears the subscribed asset set and order-book callback map in source order. Caller-supplied parent callback represents `super.dispose` and its native Result preserves the supplied failure without recreating it. Mutable RefCell guards end before parent invocation, so the parent can inspect caller-owned state. Whole class/constructor, scheduler, parent implementation and network/socket lifecycle remain independently tracked, not claimed by this slice. Generic collection contents are cleared without parsing/inspecting keys or values.

Actual-source oracle extracts the maintained dispose arrow initializer into a class extending a recording Parent, preserving super dispatch. Socket clearPingInterval dependency is also the original maintained body. Parent and timer cancellation are explicit host boundaries, not hand-rewritten disposal logic. Source confirms call order, void return, original timer/error identity, public state cleared before parent invocation, and cancellation failure preventing later operations.

Native assertions run before source comparisons and inspect trace, timer presence, public collection sizes/retained values and unchanged Rc parent-error identity with codePARENT/marker73. No re-encoded bytes or roundtrip proof. Independent fixed floors: **10 scenarios** (exchange2/private4/public4), **two calls per scenario**, **20 native disposal invocations**, stopped at public/cancel-fails/iteration1. OnceLock obtains one complete20-call source corpus per Rust test process. Scenarios cover empty/live state, repeated disposal, parent failure and cancellation failure. These are finite method-body witnesses, not exhaustive parent/socket behavior.

## Gates measured at5d91da4f5d

Exact command arrays, SHA, exits and raw outputs are tracked in evidence/disposal/. Oracle/control runs used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. One Cargo at a time.

| Gate | Exit | Scope |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --all-targets --locked --jobs 2 |0| Polymarket and dependencies |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --locked --jobs 2 -- --nocapture |0| All21tests; zero unit/doc tests |
| cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket -- --check |0| Polymarket formatting |
| python3.14 sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py |0|141rows,49public Rust items,zero venue exemptions; invention/status gates passed |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check |0| Read output:7341symbols,512files,zero stale/added/orphaned |
| python3.14 sdk-rs/safe-cex-venue-polymarket/tests/disposal_controls.py |0| Three real production mutations, each0/101/0 |

Fixed three-control floor, stopped after public-assets; no behavioral survivors. Every baseline and restored run exited0; each compiled mutation exited101 at its named native assertion:

| Row/control | Mutation | Named assertion |
| --- | --- | --- |
| exchange-parent | Skip invoking parent closure | Polymarket native exchange parent dispose count |
| private-timer | Reference the cleanup function without invoking it | Polymarket native disposal call order: private |
| public-assets | Skip asset-set clear | Polymarket native disposal assets |

All source mutations restored exactly. Expected cancellation-hook panics are deliberately caught in positive tests, with native payload and retained state assertions, so their printed panic text is not a failing gate.

## Rejected initial control, review and limits

At9b177d6379, exchange-parent had0/101/0, but the initial private-timer mutation removed the only use of an imported cleanup helper and failed compilation with unused-import error101. It reached **no native assertion**, so it was rejected as behavioral evidence. Restoration passed0. The control script exited1 and later check/tests/fmt/parity/extractor gates were not run at that SHA. Raw initial attempt and caught=false summary are retained in evidence/disposal/initial-inadmissible-control/. No runtime survivor claim is made for a mutation that never compiled. 5d91da4f5d revised only the control to reference both argument and function without calling it; all three controls and all six gates then ran afresh at the committed SHA above.

Native sub-agent static review found no actionable mismatch in the three method adapters, verified actual targets and guarded scope claims, and checked cleanup/parent order, temporary borrow release and native assertions. Reviewer ran no gates. No workspace/monorepo build, full core/MEXC suite, host clippy, wasm build/clippy, live exchange/socket call, CI rerun or PR creation was performed. No MEXC row movement is claimed in this slice.

## Final merge preservation

Fetched integration and merged `ed7ac407c716cea8b8115aea53e466c9ac69d129` at `338577203280223f33954e36d2b6e3630ebeb33c`. Required `git diff --exit-code 5d91da4f5db0a1e42c0b6381d2b39b7d8b8bacd8 HEAD -- sdk-rs/safe-cex-venue-polymarket/src sdk-rs/safe-cex-venue-polymarket/tests` exited0. Extending to venue mappings and all seven dependency src/Cargo.toml inputs also exited0; SDK Cargo.toml/Cargo.lock were unchanged. Earlier check21-test/three-control exits remain measured at5d91da4f5d with byte-identical relevant inputs preserved, not freshly re-executed at3385772032. Refreshed extractor check at3385772032 exited0 with7341symbols/512files/zero stale/added/orphaned. SHA-bound output/preservation receipt tracked in evidence/disposal/final-merge/.
