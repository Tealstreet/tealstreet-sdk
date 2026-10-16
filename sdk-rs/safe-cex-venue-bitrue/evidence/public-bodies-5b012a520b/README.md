# Bitrue public subscription, mapping and request bodies

Code/gate SHA: `5b012a520bada36a1af94ef3796773246f858281`. Actual fresh integration parent: `71ab6dd78100ff547dbfb64e2ded8d84196adcc8`. No unreviewed lane work is an ancestor of that code commit; the parent is the user-requested integration head. Evidence-only commits do not claim fresh Cargo measurements.

**8 pending → ported/adapted, 0 → divergent** from committed TOML compared by tomllib. Bitrue 33/25/49 → 41/25/41 (ported/divergent/pending). The manifest enumeration floor is 107 rows across six files, stopping `sdk-rs/parity/manifest/exchanges/bitrue/bitrue.ws-public.ts.toml` / `BitruePublicWebsocket.listenOrderBook`; no repo-wide census claim.

Eight full source arrow bodies are represented in `src/public_bodies.rs`, preserving source guard/evaluation order and original generic values, callees, receivers, callback handles/returns, and errors. No shared crate/type/trait is edited; the new host trait is venue-local.

| Actual TS method (BitruePublicWebsocket) | Native Rust body |
|---|---|
| `ensureTickerSubscription` | `ensure_ticker_subscription` |
| `connectAndSubscribe` | `connect_and_subscribe` |
| `onOpen` | `on_open` |
| `onMarketsChanged` | `on_markets_changed` |
| `subscribe` | `subscribe` |
| `mapKline` | `map_kline` |
| `handleTickersEvent` | `handle_tickers_event` |
| `makeRequest` | `make_request` |

`PublicBodyHost` and `NativeCallback` declare low-level property, operator, ordinary method/array/iterator, builtin, allocation, constructor and callback dependencies. The implementation of JavaScript builtins/Array/Set/prototype coercion/Date, Promise scheduling, JSON codec, timer/GC, class/constructor/base bindings and live transport remain pending. The host iterator owns IteratorClose and preserves the body error. No websocket listener lifecycle or whole-class completion is claimed.

`tests/public_bodies_oracle.cjs` extracts and executes each actual full TS initializer, with retained native test dependencies. The independent floor is **61 scenarios**, stopping index **60 `request:timeout-store-throw`**. Stock-boundary fixtures cover early returns, falsey fields, field replacement, callback execution, query URL/listeners, ordered numeric Object keys, empty/multiple candles, numeric NaN/Infinity/negative zero, getter/callee order and original errors, existing request lists, timeout5000, resolve, timeout, and resolve-then-timeout (source permits the callback to be manually fired after resolution). Date runtime is supplied explicitly; no dayjs implementation/timezone parser completion is claimed.

Native values are checked directly by variant, UTF16 code units, f64 bits, fields and pointers before opaque wire forwarding and final census. Original candle info, ticker info/resolved carrier, map/Promise returns, callback return, method receiver, handler and timer pointers are checked. Stringify returns an original opaque native object; native frame fields are asserted before the original stringify return is forwarded. There is no Rust JSON encoding, re-encoding, or roundtrip comparison. The fixture/error matrices are floors, not exhaustive inputs, host mutations or schedules.

Read-only sub-agent review: `review.md` binds eight rows and 38 qualified actual targets to the code SHA. Reviewer ran no gates. Provisional witness runs caught an extra observer get, null-default mismatch, numeric property-order/event-label mismatches; receiver/return proof gaps were fixed before commit. The provisional native witness also caught and corrected an actual Rust callee-order mistake (second m.symbol read preceded converter lookup). Those development run exits are not the gate exits below.

## Measured code-SHA gates

Exact commands, env, exits, durations and log paths are in `gates.json` and `baseline.json`. Cargo commands ran sequentially with jobs2; external TypeScript path was set. Exit values are process exits, not tail/pipeline statuses. Yarn output was inspected in addition to process exits.

| Gate | Exit | Raw log |
|---|---:|---|
| New native oracle baseline | 0 | `baseline.log` |
| venue-tests | 0 | `venue-tests.log` |
| check | 0 | `check.log` |
| host-clippy | 101 | `host-clippy.log` |
| wasm-clippy | 101 | `wasm-clippy.log` |
| fmt | 0 | `fmt.log` |
| writer | 0 | `writer.log` |
| extractor | 0 | `extractor.log` |

Host clippy101 and wasm clippy101 both stopped on inherited `safe-cex-transport/src/tealstreet.rs:157,177` `arc_with_non_send_sync` diagnostics. Neither is a venue or workspace lint pass. Writer/extractor output must state Updated/Would update **0 manifests; 0 added, 0 orphaned symbols**.

## Discriminating controls at the code SHA

One body mutation per new row plus the reviewer-requested source receiver control. Each documented variant compiled and failed at runtime with its named assertion, then its exact committed bytes were restored and all61 source cases passed. `controls.json` has exact base SHA, command, env, file and patch. Control floor9 stops index8 oracle-receiver; no exhaustive mutation claim and no survivor in this selected floor.

| Variant | Named assertion prefix | Runtime exit | Exact restoration exit |
|---|---|---:|---:|
| ensure | `BITRUE-PUBLIC-NATIVE-NUMBER ensure:normal:topics` | 101 | 0 |
| connect | `BITRUE-PUBLIC-NATIVE-STRING connect:normal:listeners` | 101 | 0 |
| open | `BITRUE-PUBLIC-NATIVE-STRING open:normal:emitter` | 101 | 0 |
| markets | `BITRUE-PUBLIC-NATIVE-STRING markets:normal:frames` | 101 | 0 |
| subscribe | `BITRUE-PUBLIC-NATIVE-STRING subscribe:normal:frames` | 101 | 0 |
| kline | `BITRUE-PUBLIC-NATIVE-NUMBER kline:normal:result` | 101 | 0 |
| ticker | `BITRUE-PUBLIC-NATIVE-NUMBER ticker:normal:updates` | 101 | 0 |
| request | `BITRUE-PUBLIC-NATIVE-NUMBER request:normal:timer-delay` | 101 | 0 |
| oracle-receiver | `BITRUE-SOURCE-NATIVE-RECEIVER symbols:has` | 101 | 0 |

The source receiver control changes the actual test dependency invocation receiver (symbols.has) to the original parent object, and the source exact-receiver assertion fails; it does not mutate the assertion. The Cargo test fails101 because the source oracle reports that assertion, not because of compilation. Restored source oracle passes with all61 scenarios.

## Explicit non-runs and limits

No workspace build/test/clippy sweep, canonical-index/exhaustive census, full monorepo suite, wasm runtime test, live websocket/API, CI rerun, human review, PR creation or integration merge was performed. Host and wasm clippy are separate compile/lint claims. External builtin/Promise/JSON/Date/runtime implementations are dependencies and remain pending.

Measured full venue target enumeration: 18 executable targets, stopping `tests/ws_sync.rs`; no workspace scope claim.

## Merge-down and refreshed gates

Fetched and merged integration `600bbd302bdd09d20f938650e360385e68f55cba` cleanly (fetch0, merge0); actual merge/gate SHA `dbe78e0dd13ffe8058b9faf69063001280fcec72`. Source/test byte-survival diff from code SHA exited **0**; toolchain/config comparison0. Recursive input comparison exited **1**, limited to `Cargo.lock` and Crypto lib/Poseidon example/loader source. No full recursive-input byte-unchanged claim. See `merge.json` and the three raw survival diffs.

All seven scoped gates were rerun sequentially at the actual merge SHA because recursive dependency inputs changed.

| Refreshed gate | Exit | Raw log |
|---|---:|---|
| venue-tests | 0 | `merge-venue-tests.log` |
| check | 0 | `merge-check.log` |
| host-clippy | 101 | `merge-host-clippy.log` |
| wasm-clippy | 101 | `merge-wasm-clippy.log` |
| fmt | 0 | `merge-fmt.log` |
| writer | 0 | `merge-writer.log` |
| extractor | 0 | `merge-extractor.log` |

Host and wasm clippy both remain101 on inherited transport `tealstreet.rs:157,177` Arc diagnostics. Tests/check/fmt/writer/extractor0; both writer and extractor output explicitly report0 changed manifests/added/orphaned symbols. Source control/native witness bytes are unchanged, so the nine controls are measured at the code SHA and **were not rerun after merge**; the full61-case native baseline did run again in the full venue suite at the merge SHA.

Merge-parent tomllib comparison independently confirms eight pending→ported, zero divergent, 41/25/41 across107 rows/six manifests; see `merge-census.json`. Evidence-only commits after the merge do not claim new Cargo measurements or source changes. Workspace, wasm runtime/check, live API, CI, canonical-index/exhaustive proof and human review remain unrun.
