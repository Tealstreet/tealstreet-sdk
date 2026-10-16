# OKX websocket startup parity evidence

Eight pending rows become ported; zero become divergent. Fresh branch `feat/okx-websocket-startup` starts at integration `b5ac6229e2502e48106b0a549da0524583b04627`, with no unreviewed feature branch used as its parent. Reviewed implementation and gate SHA: `51aa6bae3bae8f445cab9f8f3599fb949680bbb0`.

## Rows and contracts

All targets are in `safe-cex-venue-okx/src/startup.rs`.

| TypeScript symbol | Rust function |
| --- | --- |
| ORDER_CHANNELS | create_order_channels |
| OKXBusinessWebsocket.connectAndSubscribe | connect_business |
| OKXBusinessWebsocket.onOpen | on_open_business |
| OKXBusinessWebsocket.auth | auth_business |
| OKXPrivateWebsocket.connectAndSubscribe | connect_private |
| OKXPrivateWebsocket.onOpen | on_open_private |
| OKXPrivateWebsocket.auth | auth_private |
| OKXPublicWebsocket.connectAndSubscribe | connect_public |

These functions port native control flow, ordered data construction, disposal guards, authentication branches, narrow log catches, original error propagation, captured callees, and retained/reentrant public topic callbacks. ORDER_CHANNELS constructs a mutable shared Set through its initializer capability, rather than an immutable membership list. Its host module owns singleton initialization and lifetime. Public onMarketsChanged remains pending as a separate update unit.

The manifest notes name each exact Rust target and expression capability contract. The functions use opaque native values, Result-based expression capabilities and futures. JavaScript intrinsic semantics, bound receiver/callee capture, provider expressions, module/configuration initialization, immediate polling of futures, and concrete websocket class assembly remain host responsibilities. This is function-body parity under those explicit contracts; it does not prove a concrete runtime bridge. No shared trait or type changed. safe-cex-crypto and indexmap are test-only dependencies; the lockfile records these dependencies. Existing venue-oracle:okx all-target wiring includes the new target, but that full script was not run.

## Gates at the reviewed implementation SHA

Measured at `51aa6bae3bae8f445cab9f8f3599fb949680bbb0`, sequentially:

| Command | Exit | Observed result |
| --- | --- | --- |
| /opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 | Scoped crate check finished |
| /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test startup | 0 | startup_native_before_source passed |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Output: 7341 symbols, 512 source files; would update 0 manifests, 0 added, 0 orphaned |

The manifest writer parity:extract ran after manifest edits and completed with exit 0. Extractor output was inspected, not inferred from a Yarn shim exit.

## Independent expectations and enumeration floors

The focused test covers 111 independently enumerated fixtures: 48 connection, 28 open, 34 authentication, and one mutable Set initializer. Native value, field order, identity, error, and trace assertions run before any source comparison. Full injected error fields and original Rc identity are asserted. Native ordered packets are inspected directly; the native JSON capability retains an opaque original packet rather than implementing a JSON encoder. The source oracle executes actual JSON.stringify. Neither roundtrip equality nor a concrete Rust JSON encoder is claimed.

The connection enumeration stops at public map-reenter case 15; open at business snapshot-error case 13; authentication at business remote-opaque-time case 16; Set at initializer entry 2 (algo-advance), followed by duplicate custom insertion. These are witnessed floors, not exhaustive intrinsic, prototype, provider, or transport coverage. Retained and same-callback reentrant mapping witnesses assert native ordered channel/instId fields before packing or wire assertions. The source Set initializer's physical three-element AST is checked before Set projection, so duplicate collapse cannot hide an extra literal pair.

Independent Node HMAC measurements returned exit 0: SECRET / 7GET/users/self/verify gives `JwNTn66vBdBbr690CLCDQDG5isEgMKDEUzNXdH5gujc=`; SECRET / GET/users/self/verify gives `rB/8nmOI9+AwPl00mEIG4pXW7XeVm/afbGQT1X0gKY4=`. Native fixtures use actual safe-cex-crypto; the maintained TypeScript oracle uses its actual create-hmac dependency. Null timestamp join versus template conversion and opaque UTF-16 timestamps have distinct native expectations.

## Compiling mutation controls

Each control was applied to the reviewed implementation SHA, compiled, failed the named native assertion at exit 101, then was restored byte-for-byte and the same startup target passed at exit 0. Four families were exercised, not eight independent symbol mutations. Enumeration stopped after the Set control. Zero survivors.

| Family / call site | Mutation | Named assertion | Broken / restored exits |
| --- | --- | --- | --- |
| connections / connect_public | Swap channel and instId construction order | CONNECT native topic field order | 101 / 0 |
| open / on_open_private | Replace private-snapshot with emitWsReconnected | OPEN native auth await recheck failure suppression | 101 / 0 |
| auth / auth_private | Change verify signing path to wrong | AUTH native toSign payload | 101 / 0 |
| Set / create_order_channels | Replace orders-algo with orders | SET native independent initializer | 101 / 0 |

Read-only sub-agent review found that the original FnMut callback could not represent active same-callback reentry and that native public key order needed an explicit assertion. Commit 51aa6bae3b resolves both with shared Rc<dyn Fn> callbacks and paired reentry/order witnesses; follow-up review was clear. The earlier precommit unused-helper compiler error was a development failure, not a mutation proof. Earlier 108-fixture gates at 63ae09388d do not cover the final 111-fixture target.

## Census

An independent Python tomllib census at `63ae09388d12214f47d9e35ebc9804abe56e557b`, compared with actual parent `b5ac6229e2502e48106b0a549da0524583b04627`, enumerated 12 OKX manifests and stopped at `okxeudemo.exchange.ts.toml`: 71 ported / 19 divergent / 73 pending became 79 / 19 / 65, with exactly the eight pending-to-ported rows above. This is a status census, not proof of all venue behavior. A fresh merge-head census and preservation receipt follow below.

## Unmeasured scope

No workspace compilation, full venue suite, KuCoin check or tests, host or wasm clippy, CI rerun, live network test, full Rust-index audit, concrete websocket class/module assembly, or exhaustive language/provider correctness was run or proved in this slice.

## Integration merge receipt

Fetched and merged integration `08e5cc0cce0cd909aedfa8d2dcde1a9ce448b2f5` cleanly. Actual merge head: `0149d6a0f787092bf394b27299b7026f93338b9f`.

At that merge head, measured `git diff --exit-code 51aa6bae3bae8f445cab9f8f3599fb949680bbb0 HEAD -- sdk-rs/safe-cex-venue-okx/src sdk-rs/safe-cex-venue-okx/tests sdk-rs/safe-cex-venue-okx/scripts sdk-rs/safe-cex-venue-okx/Cargo.toml sdk-rs/parity/manifest/exchanges/okx package.json`: exit 0, byte-unchanged. Separately measured the same diff for sdk-rs/Cargo.toml, sdk-rs/Cargo.lock, safe-cex-types, safe-cex-num, safe-cex-store (including the source loader), safe-cex-exchange, safe-cex-transport, safe-cex-crypto, packages/safe-cex/src/exchanges/okx, and parity/extract-ts: exit 0. The scoped check, focused test, and four mutation controls remain measured at 51aa6bae3b; they were not remeasured at the merge SHA because their inputs are unchanged.

Fresh `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` at the actual merge head: exit 0. Inspected output: 7341 symbols across 512 files, would update 0 manifests, 0 added and 0 orphaned. This is a fresh metadata gate, not a workspace compile.

Fresh independent Python 3.11 tomllib census at the actual merge head versus merged integration: exit 0, exactly 8 pending-to-ported changes and zero other status changes. It enumerated 12 OKX manifests, stopping at sdk-rs/parity/manifest/exchanges/okx/okxeudemo.exchange.ts.toml. Integration 71 ported / 19 divergent / 73 pending becomes 79 / 19 / 65. KuCoin is untouched. The first default-python census attempt failed because Python lacked tomllib; Python 3.11 produced the measured census above.

The delivery receipt commit changes only this manually authored evidence file. Its source and gate-input preservation diff is measured separately below.

Measured before the receipt commit: `git diff --exit-code 0149d6a0f787092bf394b27299b7026f93338b9f -- . ":!sdk-rs/safe-cex-venue-okx/STARTUP_EVIDENCE.md"` exit 0, and `git diff --check` exit 0. No source or gate inputs changed in the receipt edit.
