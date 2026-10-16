# GRVT public websocket subscriptions

Implementation SHA: `1a39d3674041cefe0a3db14cd1d9a3b4017971a1`.
Actual initial parent: `2c17c1976a94ff9b919ad0ac534dd77ea704960f`.
Fresh branch: `feat/rust-grvt-public-subscriptions`.

Nine rows move from pending: eight ported (`subscribeToKline`, `unsubscribeFromKline`, `subscribeToOrderbook`, `unsubscribeFromOrderbook`, `subscribeToTicker`, `unsubscribeFromTicker`, `cancelTickerBatches`, `ensureTickerSubscription`), one CHOSEN divergent (`mapIntervalToGrvt`). Independent Python tomllib census of git objects over the nine tracked GRVT manifests measures 164 rows in both parent and implementation: 42/46/76 ported/divergent/pending becomes 50/47/67. Enumeration stops at those nine GRVT manifests; no project-wide completeness claim.

Typed native RPC requests retain fields and f64 IDs until shared JS JSON serialization. Subscription keys use SameValueZero, whereas ticker-ID cleanup uses source strict equality. Source counter/insertion/send/deletion order, failed-send state retention, ticker guards/reuse and batch generation/cancellation are implemented. Timer slots stay tombstoned while cancellation iterators are live, including nested cancellation followed by insertion; unused slots compact after the outermost iterator ends.

The mapper's CHOSEN difference is explicit: native TableHit/ProtoMember retains inherited identity, arity and interpolation, while source returns an executable builtin or mutable prototype object. The actual source `hasOwnProperty.call` witness returns true; native returns the corresponding descriptor. An executable JavaScript object model is intentionally avoided. Unknown interval strings retain themselves, unlike exchange convertInterval's default interval. Full websocket class/constructor, connection, replay and batch scheduling remain pending.

## Gates measured at the implementation SHA

Cargo ran sequentially with jobs 2.

| Command | Measured exit and observed outcome |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0; scoped compilation finished |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --test public_subscriptions --jobs 2` | 0; nine tests passed under Node 24.20.0 |
| Same test command with `/opt/homebrew/opt/node@26/bin` prepended to PATH | 0; nine tests passed under Node 26.7.0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; OUTPUT: `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

The extractor writer ran after manifest edits through the pinned Yarn JS entry: exit 0; OUTPUT said one manifest updated, zero added/orphaned symbols. TOML arrays were emitted by the serializer/extractor, not hand-spelled.

The source oracle extracts the actual nine methods and interval enum. JSON.stringify observation captures native source requests before actual serialization; the original serializer still supplies wire strings. The fixed floor stops at 33 interval cases, 25 subscribe cases (9 candle, 8 book, 8 ticker), 21 unsubscribe cases (7 each), 6 cancellation cases and 7 ensure cases: 92 fixed cases plus one inherited-builtin execution witness. Independently fixed case, request, subscription, ticker, timer and effect counts are asserted. Native request/state/counter/key/error/timer values precede deferred wire comparisons. Non-NaN f64 bits and NaN classification are asserted; engine/CPU-dependent NaN payload bits are not claimed equal.

## Deliberate compiling controls

Controls were measured against the implementation SHA before merge-down. Each changes only `src/public_subscriptions.rs` temporarily and uses the scoped test command above with `<test_name> -- --exact`. Every failure compiled and entered the test, then panicked at the named native assertion. Restoration is byte-exact; each restored target passes once. No survivor or compilation-only failure.

| Row / mutation | Exact test target | Named failing assertion | Broken exit | Restored exit |
| --- | --- | --- | --- | --- |
| mapIntervalToGrvt: map 8h to CI_6_H | `interval_native` | `MAP native interval` | 101 | 0 |
| subscribeToKline: wrong stream | `subscribe_kline_native` | `SUBSCRIBE native stream` | 101 | 0 |
| unsubscribeFromKline: target ID plus one | `unsubscribe_kline_native` | `UNSUBSCRIBE native target` | 101 | 0 |
| subscribeToOrderbook: wrong stream | `subscribe_book_native` | `SUBSCRIBE native stream` | 101 | 0 |
| unsubscribeFromOrderbook: target ID plus one | `unsubscribe_book_native` | `UNSUBSCRIBE native target` | 101 | 0 |
| subscribeToTicker: wrong default stream | `subscribe_ticker_native` | `SUBSCRIBE native stream` | 101 | 0 |
| unsubscribeFromTicker: target ID plus one | `unsubscribe_ticker_native` | `UNSUBSCRIBE native target` | 101 | 0 |
| cancelTickerBatches: generation plus two | `cancel_batches_native` | `CANCEL BATCH native generation` | 101 | 0 |
| ensureTickerSubscription: refresh 501 instead of 500 | `ensure_ticker_native` | `ENSURE native selector` | 101 | 0 |

Sub-agent `review_snapshots` approved immutable implementation SHA `1a39d3674041cefe0a3db14cd1d9a3b4017971a1` with the stated parent. Review found Serde-vs-JS number spelling and nested cancellation cursor bugs during development; both were fixed before this SHA. It independently ran an actual-source nested probe, exit 0 (clear order 101,101,202,303; generation 2), but did not run cargo/native tests or mutation controls. Review approval is separate from the measured parent-agent gates.

## Merge-down measurements

Fetched `origin/feat/safe-cex-rust` immediately before merging integration SHA `3a4f19c35ab9a8a795bef393a969448fc2cad908` down. Actual merge SHA: `e6ec2fb8b05d94c70e1b2eafb8e0d9d82aed1d9c`.

`git diff --exit-code 1a39d3674041cefe0a3db14cd1d9a3b4017971a1 HEAD -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests packages/safe-cex/src/exchanges/grvt/grvt.ws-public.ts packages/safe-cex/src/exchanges/grvt/grvt.types.ts` exited 0 at the merge SHA. Reviewed venue code/tests and source oracle inputs are byte-identical. A separate git diff exit-code check over safe-cex-num/types/exchange/store/transport src, workspace Cargo.toml and Cargo.lock also exited 0: those dependency inputs did not change. The implementation-SHA compilation/native/control measurements stand under the preservation rule; they were not rerun at the merge SHA.

The pinned extractor-check command was rerun at the merge SHA: exit 0; OUTPUT says zero manifest updates/additions/orphans. `git diff --check` exited 0. Independent GRVT census at fetched integration and merge again measures 164 rows: 76 pending before, 67 after, with the same eight ported and one divergent movements.

The branch was created from the supplied integration head; no unmerged feature branch was used as its base. The merge-down parent is fetched integration. The following evidence commit changes only this manually written document; no gate is claimed at the evidence-only SHA.

Not run: workspace compilation, full suites, host clippy, wasm check/clippy/tests, live websocket/network requests, full venue construction/connection/replay/batch scheduling, an exhaustive input enumeration, custom/mutated Object.prototype execution, or engine-specific NaN payload proof. Inherited Rust parity-validator diagnostics were not rerun or claimed green. This evidence covers scoped compilation, extractor and the named native/source tests and controls.
