# Crypto.com order-write and listener bodies

Code/witness SHA `cafdd2d60130dfe99d937f4b62519600e5e93119`, actual fresh parent `198f871b9b004f6384d910a8c3af5909bd706b48`. Parent was integrated; no unreviewed other-lane ancestor. Source implementation commit1001271f8a followed by one atomic witness correction; no rewrite.

Tomllib census: **8 pending→ported/adapted, 0 divergent**. Venue92/18/27→100/18/19 (ported/divergent/pending). Census floor137 rows in6manifests stops public WebSocket dispose. Enumeration is a census, not exhaustive proof.

Rows: placeOrder, cancelOrder, cancelOrders, cancelSymbolOrders, updateOrder, placeOrders, listenOHLCV, listenOrderBook. Full source bodies ported in venue-local order_writes.rs with explicit native dependency interfaces; shared crates and TypeScript unchanged. Row targets include54 distinct real qualified body/helper/trait targets; static reviewer found missing/nonexistent0, stopped listenOrderBook. Class construction, runtime wiring, standard builtins/coercions/import implementations and host ABI remain pending; those are not claimed implemented by the body ports.

Preserves original native operands, repeated getters, callee/receiver lookup and coercion order, eager dependency invocation before awaiting, original failures and logging, live sequential batch iteration, caught per-order errors, iterator closing, and listener return/callback identity. Actual installed p-iteration implementation supplies the oracle dependency, including live array length, sparse and inherited slots. Native adapters preserve observed scheduling and opaque dependency results at this bounded fixture floor; no universal scheduler equivalence claim.

Actual TypeScript full arrows and original switch helpers execute in the oracle. Direct decoded native fields, UTF16 strings, finite f64 bits/signed zero/infinity, NaN classification, original opaque/error/argument/callback pointer identities precede event and inventory assertions. No Rust encode/reencode, roundtrip, or wire equality. Standard coercion returns are captured from the actual dependency after asserting original native input; no builtin implementation parity claim or NaN payload claim.

Independent fixture floor91 stops index90 listen-book-primitive-return. Expected posts/calls/callback counts are specified in fixtures, not derived from captured arrays. Includes market/type branches, trigger validation, omitted/null/update/inherited fields, native errors and getter failures, partial batches, live append, spread/close errors, replacement socket getter, Promise/thenable listener returns. These are bounded cases, not exhaustive inputs or schedules.

Historical1001271f8a witness finding is retained in the adjacent evidence directory: incorrect cancel_orders dependency-result forwarding survived0/restored0 because Host.series customSeries lost the opaque result. Corrected adapter now preserves that result. Controls attach runtime outcomes to their measured SHA; compiler-only failure remains disclosed.

Read-only sub-agent review receipt in review.md. Gate commands, logs, exits and exact source/input survival after integration merge are attached separately below when completed. No gate is claimed while running.

Eight sequential controls at cafdd2d601: each compiled and hit its named runtime assertion (101), then exact source restoration passed91 cases (0). Floor8 stops index7 listen-book; no survivors at this selected floor. controls-survival.json records exact source/test diff0. Historical survivor remains a finding at its old SHA; it is closed by the same forwarding mutation failing at the corrected SHA.

Final merged gate sheet: [order-writes-3f0d71ebf6](../order-writes-3f0d71ebf6/README.md). Own eight rows pending→ported; latest integration leaves5pending. Final check/two relevant test targets/fmt/writer/extractor0; host and wasm clippy101 inherited transport Arc lints. Full venue suite0 earlier; not rerun after final socket merge.
