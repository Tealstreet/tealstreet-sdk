# Bitunix precision-axis audit

Audited code SHA: `406040dfffb8dcfde0845d503b1efdcfabdc5f90`.

Compiled venue inventory: **0 price-step reads, 0 amount-step reads,
0 two-axis formatter sites**. No production precision swap can be constructed
in this implementation, so no mutation/compile/test result is claimed.

Method: read all six `src/*.rs` files and Cargo.toml end to end, trace lib.rs's
five modules, imports, helper calls, Serde companions and numeric-output
registrations; enumerate crate Rust files and check build.rs/alternate targets/
generated includes. There are no hidden formatter wrappers or precision calls.
An independent sub-agent repeated that source and module-graph audit and found
zero sites. This conclusion is not based on a keyword search alone.

Real tomllib reconciliation of nine manifests: 122 rows, 13 ported, 7 divergent,
102 pending. The 20 mapped rows cover constants, wire declarations and scalar
utilities. Price-named wire fields and trigger-type constants do not perform
rounding or read market precision.

The actual TypeScript has separate precision axes in these pending rows:

- BitunixExchange#formatCreateOrder
- BitunixExchange#formatTPSLOrder
- BitunixExchange#updateStandaloneTpslOrder
- BitunixExchange.updateOrder

Before mapping those rows, use actual-TS fractional cases whose price-step and
amount-step results differ across several step pairs, both sides, limit and
stop-market branches, including attached and standalone brackets and amendments.
Assert direct native payload values. Independently swap each production price
precision read to amount precision (and applicable amount reads to price), prove
it compiles and exits 101 at a named assertion, then restore and pass. A value
such as 12.01 under steps 0.1 and 1 can collapse and is not a discriminating case.

This is an absence-of-sites result for this SHA, not swap immunity for future
formatters. PR 7260's published head remains pinned; this audit is local only.
