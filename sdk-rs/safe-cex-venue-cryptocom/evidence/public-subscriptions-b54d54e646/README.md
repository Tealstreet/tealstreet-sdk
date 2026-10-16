# Crypto.com public websocket subscription family

Moved **8 pending → ported, 0 → divergent**, verified independently with Python3.11 tomllib: [census.json](census.json), [merge-census.json](merge-census.json). Rows: replay, frame batching, deduplicating registration, orderbook, ticker, tickers, trade and candlestick subscriptions. Resulting Crypto.com counts: {'ported': 40, 'divergent': 18, 'pending': 79}. Census floor: six manifests, 137 rows, stopped at `CryptocomPublicWebSocket.dispose`; census is distinct from proof.

Fresh actual parent: `c4c1ec196d794b431bed97b2ede5eff17a41c6f5`. Code/gate SHA: `b54d54e646fb15e46b644fd792c6cdbc7bceab08`. Integration merged down: `200ef7e5dd8ead4ef9998682f302d272ca8a0dae`. Post-merge measurements: `fe7284543d609d33de379d010a25fd673aabb9f3`. Only this reviewed slice is an own ancestor outside integration; no unrelated unreviewed branch was imported. Evidence-only delivery does not relabel measurements as gates at the delivery SHA.

All Cargo commands were sequential, scoped to `safe-cex-venue-cryptocom` and used jobs2 where applicable. Venue oracle environment: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Raw logs and ledgers bind commands/exits to exact measured commits; mutation logs explicitly identify the sole intentional variant of code SHA.

## Code SHA gates

| Gate/control | Measured exit | Raw log |
| --- | --- | --- |
| `native-baseline` | 0 | [native-baseline.log](native-baseline.log) |
| `batch-mutation` | 101 | [batch-mutation.log](batch-mutation.log) |
| `batch-restored` | 0 | [batch-restored.log](batch-restored.log) |
| `registration-mutation` | 101 | [registration-mutation.log](registration-mutation.log) |
| `registration-restored` | 0 | [registration-restored.log](registration-restored.log) |
| `channel-format-mutation` | 101 | [channel-format-mutation.log](channel-format-mutation.log) |
| `channel-format-restored` | 0 | [channel-format-restored.log](channel-format-restored.log) |
| `array-error-mutation` | 101 | [array-error-mutation.log](array-error-mutation.log) |
| `array-error-restored` | 0 | [array-error-restored.log](array-error-restored.log) |
| `venue-tests` | 0 | [venue-tests.log](venue-tests.log) |
| `check` | 0 | [check.log](check.log) |
| `host-clippy` | 0 | [host-clippy.log](host-clippy.log) |
| `wasm-clippy` | 0 | [wasm-clippy.log](wasm-clippy.log) |
| `fmt` | 0 | [fmt.log](fmt.log) |
| `writer` | 0 | [writer.log](writer.log) |
| `extractor` | 0 | [extractor.log](extractor.log) |

## Runtime discrimination

| Family control | Named native assertion | Broken → restored exit |
| --- | --- | --- |
| batch | `CRYPTOCOM-SUBSCRIPTIONS-NATIVE-CHANNELS frames-101 frame0` | 101 → 0 |
| registration | `CRYPTOCOM-SUBSCRIPTIONS-NATIVE-STATE register-duplicates` | 101 → 0 |
| channel-format | `CRYPTOCOM-SUBSCRIPTIONS-NATIVE-STATE book-default` | 101 → 0 |
| array-error | `CRYPTOCOM-SUBSCRIPTIONS-NATIVE-RETURN frames-index-throw` | 101 → 0 |

Each mutant compiled and reached the named runtime assertion, then exact source restoration passed both new witness tests. Batch control changes the slice end from100 to101; registration omits the Set insertion; channel-format control changes book. to wrong.; array control swallows a slice/index-getter exception as success. Four controls were run in that order and stopped at array-error, index3. No survivor in that floor. This is one control per newly ported family, not exhaustive mutation coverage or a new audit of older rows.

## Merge verification

| Gate/comparison | Measured exit | Raw log |
| --- | --- | --- |
| `source-unchanged` | 0 | [merge-source-unchanged.log](merge-source-unchanged.log) |
| `inputs-comparison` | 0 | [merge-inputs-comparison.log](merge-inputs-comparison.log) |
| `lock-comparison` | 1 | [merge-lock-comparison.log](merge-lock-comparison.log) |
| `venue-tests` | 0 | [merge-venue-tests.log](merge-venue-tests.log) |
| `native` | 0 | [merge-native.log](merge-native.log) |
| `check` | 0 | [merge-check.log](merge-check.log) |
| `host-clippy` | 101 | [merge-host-clippy.log](merge-host-clippy.log) |
| `transitive-transport-comparison` | 1 | [merge-transitive-transport-comparison.log](merge-transitive-transport-comparison.log) |
| `wasm-clippy` | 101 | [merge-wasm-clippy.log](merge-wasm-clippy.log) |
| `fmt` | 0 | [merge-fmt.log](merge-fmt.log) |
| `writer` | 0 | [merge-writer.log](merge-writer.log) |
| `extractor` | 0 | [merge-extractor.log](merge-extractor.log) |

Source/test comparison covers the whole venue src/tests tree against code SHA. Dependency comparison covers actual venue TypeScript, venue/workspace manifests and direct crypto/exchange/num/store/types sources/manifests; lock comparison is separate. **Post-merge host and wasm clippy both FAILED exit101**, at safe-cex-transport/src/tealstreet.rs:157 and :177 (Arc<TealstreetAuthorizer> / Arc<TealstreetPrivateProxy> neither Send nor Sync; arc_with_non_send_sync). The initial direct-input comparison omitted this changed transitive transport crate; the separately recorded transitive comparison exits1 and preserves its actual diff. Both code-SHA clippy results were0; they do not establish clippy success at the post-merge SHA. Cargo.lock comparison also exits1 (Apex/Blofin dependency entries). Post-merge scoped tests/check/fmt/writer/extractor passed0. No upstream lint suppression or transport fix is bundled into this slice. Host clippy, wasm clippy and fmt are only claimed at the commits in their logs. An unchanged input comparison is not a rerun. Writer/extractor output was read and required zero changed manifests and zero added/orphaned symbols, in addition to process exits.

## Native proof and scope

Actual complete TypeScript arrow initializers and the actual source chunk constant are evaluated; no copied function bodies replace them. The fixture expectation file is independent input data with explicit expected frame counts. Floor68 ends case67 `replay-replace-set`; a separate opaque-map identity case stops at index0. These are bounded fixture floors, not an exhaustive enumeration.

Decoded native UTF-16 strings, holes versus present undefined, insertion-ordered Set contents (including retained old Set objects), id/nonce f64 values, native void/original error identity, prepared callee identity and effect order are asserted before inventories. Signed zero is compared by bits and NaN by native NaN class. TypeScript checks original exceptions and map result identity before transport records. Rust performs no wire encode or decode/re-encode comparison in these witnesses.

The bodies preserve frame call resolution before id++, slicing before clock, live array-length checks,100-slot batches, guard/dedup order, source-native return discarding and retained registration/id changes after failures. Native Set handles can be replaced without a borrow guard spanning callbacks. Source/native cases include send reentry, growth/shrink, throwing accessors/custom iteration, inherited slots and opaque map carrier identity.

`SubscriptionArray` admits length/slice/map/iterator resolution-next effects/errors and associated opaque Slice/Mapped carriers; the ordinary RefCell/SlotIterator adapter is implemented. `SubscriptionTransport` explicitly owns callee resolution, clock, JSON encoding and prepared send binding. Full JavaScript host-array adapters, class/constructor field binding, builtin lookup/encoding, socket transport and market-data cap orchestration remain separate pending work. This is a function-body claim at declared boundaries, not end-to-end adapter proof. All introduced helpers/types/traits/methods are explained exact-qualified split targets in the eight real source rows.

[Read-only sub-agent review](subagent-review.md) cleared `b54d54e646fb15e46b644fd792c6cdbc7bceab08`. It found and prompted correction of the earlier uncommitted array-accessor/Set-replacement gaps; no human review or Cargo run by reviewer. Uncommitted development attempts (including a missing witness trait import and a stale60 inventory count after eight fixture additions) are not counted as mutation controls or SHA-bound gates; root diagnostics remain retained.

Not run: workspace/repository builds or suites, Bitrue checks/tests, canonical index gate, wasm runtime, live venue/network behavior, CI or standalone prior-row control scripts. Cargo all-target tests cover this Crypto.com crate, not those wider claims.

## Final merge-down receipt

Latest integration `dc74077e6165f04841de6a6bbd9d73c55afc58db` merged down at `f0c93a59c8360f4149ea92b7137902280a1e13c3` immediately before delivery. Venue src/tests are byte-unchanged from code SHA (exit0); all listed direct/transitive transport inputs and Cargo lock/manifests are byte-unchanged from the previous measured merge SHA (exit0). See [final-gates.json](final-gates.json) and [final-census.json](final-census.json). Final extractor check exited0 with zero changed manifests/added/orphaned symbols.

No Cargo gate was rerun at this final merge: Crypto.com tests/check/fmt remain measured0 at `fe7284543d609d33de379d010a25fd673aabb9f3`; host/wasm clippy remain measured101 there on unchanged upstream transport lints. These failures remain failures. Evidence-only commits do not relabel those SHA-bound measurements.
