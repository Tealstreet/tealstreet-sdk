# GRVT public data and leverage slice

Implementation SHA: `5a2a4c55652300f26b8aa5261e7d298c7a35ed89`.
Actual initial parent: `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e`.
Fresh branch: `feat/rust-grvt-leverage-reads`.

Five rows move from pending: two ported (`fetchOHLCV`, `fetchOrderBook`), three CHOSEN divergent (`updateMarketLeverageLimits`, `fetchLeverageForSymbol`, `setLeverage`). Independent Python tomllib census of all nine GRVT manifests compares git objects at parent and implementation: 164 rows in each, 40/43/81 ported/divergent/pending becomes 42/46/76. The enumeration stops at those nine GRVT manifests; no project-wide completeness claim.

The public readers implement native typed requests, lite/full aliases, JavaScript numeric parsing, candle reversal, source-order book accumulation, and eager/catch logging. Leverage methods implement account guards, full/lite precedence, duplicate-last limits, default/cache handling, clamp requests, and cache notification before a fresh position lookup.

The three CHOSEN rows name real shared-value boundaries. Limits clone unmatched Market elements where source retains the original object. Fetch captures an owned Market across authentication and returns a separate owned leverage value from the cache entry. Setter captures an owned Market ID across logging and omits an own undefined SDK account slot if its live account getter becomes undefined. Actual source/native witnesses measure these differences. A parallel mutable JavaScript identity/undefined-slot model across shared types is intentionally avoided.

## Gates measured at the implementation SHA

All cargo invocations were sequential, jobs 2.

| Command | Exit and observed outcome |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0, scoped compilation finished |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --test data_reads --jobs 2` | 0, eight tests passed, Node 24.20.0 |
| Same test command with `/opt/homebrew/opt/node@26/bin` prepended to PATH | 0, eight tests passed, Node 26.7.0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; OUTPUT says `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

The extractor writer ran after manifest edits using the pinned Yarn JS entry, exit 0; output said one manifest updated and no added/orphaned symbols. Manifest arrays were emitted by the TOML serializer/extractor, not hand-spelled.

The oracle extracts the actual five source methods plus their interval helper/enum. The floor stops at 17 candle cases, 17 book cases, and 19 cases for each leverage method: 91 fixed cases plus six ownership/live-state witnesses. Independently fixed case/count expectations are asserted in the native tests. Native request/candle/level/leverage/cache/position values are asserted before wire assertions. Non-NaN f64 values are bit-compared; NaN classification is asserted. Engine/CPU-dependent NaN payload bits are not claimed equal.

## Deliberate compiling controls at the implementation SHA

Each control modifies only `src/data_reads.rs` temporarily. Tests use the preceding scoped test command plus `<test_name> -- --exact`. Every failure compiled, entered the named test, and panicked at the listed assertion; none is a compilation failure. Each original file was restored byte-for-byte before its passing target run.

| Row / mutation | Exact test | Named assertion | Broken exit | Restored exit |
| --- | --- | --- | --- | --- |
| fetchOHLCV: reverse twice instead of once | `ohlcv_native_requests_and_candles` | `CANDLE native timestamp` | 101 | 0, one passed |
| fetchOrderBook: accumulate prior rows before current amount | `book_native_levels_and_totals` | `BOOK native total` | 101 | 0, one passed |
| updateMarketLeverageLimits: matched max plus one | `limits_native_market_projection_and_fallback` | `LIMITS native max` | 101 | 0, one passed |
| fetchLeverageForSymbol: returned/cache leverage plus one | `fetch_leverage_native_cache_and_fallback` | `FETCH LEVERAGE native buy` | 101 | 0, one passed |
| setLeverage: clamp max plus one | `set_leverage_native_clamp_and_position_order` | `SET LEVERAGE eager effects` | 101 | 0, one passed |
| setLeverage additional native-value control: only SDK payload leverage plus one | `set_leverage_native_clamp_and_position_order` | `SET LEVERAGE native clamp payload` | 101 | 0, one passed |

The setter clamp control failed before reaching the initially intended payload assertion, because its eager log already contains the changed value. This was recorded rather than dropped; the extra payload-only control reaches the native payload assertion. No controls survived.

Sub-agent `review_order_helpers` approved the immutable implementation SHA and the two-ported/three-divergent classifications. It independently ran the source oracle, exit 0, at the stated floor, but did not run cargo, native tests, extractor, or controls. Review approval is separate from the measured gates above.

## Merge-down measurements

Fetched `origin/feat/safe-cex-rust` and merged integration SHA `ed6234e728291cd1abe8bffd4af446d43ceda243` down into this branch. Actual merge SHA: `aee74bb452335935e0837fb72c54ed732e2e548a`.

`git diff --exit-code 5a2a4c55652300f26b8aa5261e7d298c7a35ed89 HEAD -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts packages/safe-cex/src/exchanges/grvt/grvt.types.ts` exited 0 at the merge SHA. Reviewed venue code/tests and actual oracle inputs are byte-identical; the implementation-SHA behavioral gates above remain those measurements and were not rerun at the merge.

Shared dependencies changed, so the exact scoped cargo check was rerun at the merge SHA: exit 0, compilation finished. The pinned extractor-check command was rerun at the merge SHA: exit 0, output says zero manifest updates/additions/orphans. `git diff --check` exited 0. Independent GRVT census at integration and merge again measures 164 rows, 81 pending before versus 76 after, with the same five movements.

Initial ancestry consists of the supplied integration head, not another unreviewed feature slice. The merge-down parent is the fetched integration branch. The following evidence commit changes only this manually written document; no gate is claimed to have run at that evidence-only SHA.

Not run: workspace compilation, full suites, host clippy, wasm check/clippy/tests, live exchange requests, the pending authentication/client factories or whole venue initialization, engine-specific NaN-payload proof, or an exhaustive input enumeration. Inherited Rust parity-validator diagnostics were not rerun or claimed green. This is scoped compilation plus the extractor gate and the named native/source proofs above.
