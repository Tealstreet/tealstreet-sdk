# Final Crypto.com order-write slice gate sheet

Measured final gate SHA `3f0d71ebf61f4eaa665fc223ec685a5f32c1835c`. Actual fresh branch parent `198f871b9b004f6384d910a8c3af5909bd706b48`. Latest merged integration parent `a553ce92e78a0559194e953f569bf4fe42e6db8c` at7428e40272. Other-lane ancestors arrived through integration; no direct unmerged other-lane branch ancestor. This branch is feat/cryptocom-order-write-bodies.

**8 pending→ported/adapted, 0 divergent.** Compared with merged integration104/20/13, final venue112/20/5 (ported/divergent/pending). census.json uses tomllib, floor137 rows across6manifests stops public WebSocket dispose. Integration independently moved fourteen socket rows (twelve ported, two divergent); those are not this slice's work. Against original parent, our historical census92/18/27→100/18/19 is retained separately.

Rows: placeOrder, cancelOrder, cancelOrders, cancelSymbolOrders, updateOrder, placeOrders, listenOHLCV, listenOrderBook. Full source bodies and native value/error identity, eager dependency calls, original callee/receiver lookup, repeated getters, sequential live iteration and listener return/callback forwarding are retained in order_writes.rs. Native builtins/imports/ABI and exchange runtime construction/wiring remain explicit dependencies, not claimed fully implemented. Source and test proof boundaries are in [implementation evidence](../order-writes-cafdd2d601/README.md).

Own order-write source/oracle/fixtures/witness diff0 against corrected witnesscafdd2d601. Whole crate diff1 after integration due socket additions; lib.rs conflict retained both registrations. Reordered declarations to satisfy rustfmt in3f0d71ebf6. Final affected gates were rerun below. Exact commands, environment, SHA, exits, raw logs in gates.json; one Cargo at a time, jobs2, locked. The command runner stopped at intermediate7428 format1; writer/extractor were not executed there. Historical failed format and compiler-only mutation are retained honestly.

| Gate at3f0d71ebf6 | Exit | Log |
|---|---:|---|
| order-writes-tests | 0 | order-writes-tests.log |
| socket-handlers-tests | 0 | socket-handlers-tests.log |
| check | 0 | check.log |
| host-clippy | 101 | host-clippy.log |
| wasm-clippy | 101 | wasm-clippy.log |
| fmt | 0 | fmt.log |
| writer | 0 | writer.log |
| extractor | 0 | extractor.log |

Writer and extractor actual output both show0 manifest changes/0added/0orphaned. Compile floor preserved by scoped all-target check0. Host and wasm clippy each101, blocked by inherited safe-cex-transport/src/tealstreet.rs:157/177 Arc-with-non-Send/Sync diagnostics; neither is a lint pass and later venue diagnostics may be preempted.

Eight selected mutation controls at corrected code/witnesscafdd2d601 each compiled, hit the named native assertion101, then exact restoration passed0. Floor8 stops index7 listen-book. Independent fixture floor91 stops index90 listen-book-primitive-return; assertions inspect decoded native values before event/inventory comparisons, no Rust roundtrip/wire/reencoding. Counts come from fixture expectations. Static sub-agent review enumerated54 valid qualified targets across8rows, missing/nonexistent0, stopped listenOrderBook; it ran no gates.

Historical finding at1001271f8a: forwarding cancel_orders dependency result survived0/restored0 because custom Host.series discarded its opaque return. Witness corrected; the same forwarding mutation now fails the named native assertion101/restores0. Logs and patches attach both outcomes to actual SHAs. An earlier append mutation failed compilation and is not runtime proof. No survivor is omitted.

Full scoped venue suite exited0 at earlier4b726ad13e (eighteen integration targets plus library unit target; stops ws_policy.rs). After the latest socket merge, only the own order-write and integrated socket-handler test targets were rerun, both0. **Not run after latest merge: full venue suite. Not run at all for this slice: workspace builds/suites, live venue API, wasm runtime tests, CI, exhaustive arbitrary host/coercion/schedule/input proof.** Controls were not rerun on the final merge SHA; own implementation and witness bytes are unchanged and final order-write baseline0 confirms the fixture floor. No blanket completeness claim.

Gate-input preservation covers recursive local dependency source/Cargo manifests, lock/toolchain, actual TypeScript and parity/oracle inputs. Final commits add evidence only; measured exits remain attached to3f0d71ebf6, not relabeled as later commit executions.
