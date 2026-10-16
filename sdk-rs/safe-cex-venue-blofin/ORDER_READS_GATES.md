# BloFin order-reader slice

Eight rows moved from pending to CHOSEN divergent: fetchOrders,
fetchOrdersWithRetry, fetchPaginatedOrderRows, fetchNormalOrders,
fetchAlgoOrders, fetchTriggerOrders, fetchHistoricalOrders, fetchHistoricalFills.
Each row names its actual Rust function and has its own representation/host note.
This is the configured REST reader unit; API construction and websocket rows stay pending.

Original integration parent: `3e4ab045924dd879fa2e3dfa0519d647808e99cd`.
Implementation: `2e8d57b6873647b7148b71cd9aafdf31f60c0eaa`.
Final code, gate, controls and clean sub-agent review SHA:
`afcc17b10fd1197eb75cedf63190215afe1b080c`.
Integration `b5ac6229e2502e48106b0a549da0524583b04627` was merged via
`b66e543b77b30cdc4214801f61804f5e505fdd80` before those gates; changed exchange
dependency inputs were built in the final gates, rather than inheriting earlier results.

Final integration merge: `ef054ba0b8e9038241576f44c09f652348f2f4bb`, via
`599cddb713f58c5c03f37a1fe800b4a7922203f5`. Scoped source/test/dependency/Cargo/lock/
source-oracle/extractor/manifest input diff versus the gate SHA exited0.
Cargo inputs are byte-unchanged, so the Cargo gate SHA remains afcc17b.
The global extractor is checked again at the merge SHA; see merge-extractor.json.
Integration ancestors are inherited and were not separately reviewed by this lane.
No private lane branch was merged. Following evidence commits change documentation only.

## Census

Tomllib measured14 ported /51 divergent /60 pending at the original parent,
and14 /59 /52 at the code SHA. Eight independently selected rows moved;
seven manifest files and125 rows were visited, stopping after file7.
This census is not behavior proof. See order-reads-evidence/census.json.
Source declaration coverage separately stopped after4 files /92 declarations
with independent floors20 /3 /3 /66.

## Measured gates

All commands are recorded with exact SHA and exit in order-reads-evidence/.
Cargo invocations ran serially with jobs2; the source scripts contain no Cargo calls.

| Gate | Exit | Evidence |
| --- | --- | --- |
| cargo check, safe-cex-venue-blofin, all-targets, jobs2 |0| check.log; cargo-gates.json |
| Host Clippy, same crate/all-targets, no-deps, -D warnings |0| host-clippy-crate-only.log |
| Host Clippy including dependencies, -D warnings |101| host-clippy-with-dependencies.log; two inherited arc_with_non_send_sync diagnostics at safe-cex-transport/src/tealstreet.rs:157 and177 |
| cargo test, safe-cex-venue-blofin, --test order_reads, jobs2 |0| tests.log;9 passed |
| rustfmt check, new order_reads.rs source and test |0| fmt.log |
| parity:extract:check at code SHA |0| extractor.log; output would update0, added0, orphaned0 |
| parity:extract:check at final integration merge |0| merge-extractor.log; output would update0, added0, orphaned0 |
| Actual-source declaration coverage, Blofin scope |0| coverage.log; independently pinned four-file counts |
| Existing Rust parity-check binary, Blofin only, empty exemptions |1| rust-parity.log; only3 inherited position_adapter exports lack mappings |
|13 production mutation controls, broken/restored |101 /0 each| controls/results.json and paired logs; stopped after control13, no survivors |
| Source/test and all scoped input preservation after final integration merge |0 /0| merge-preservation.json |

The inherited Rust parity findings are BlofinPositionAdapterDeps,
map_blofin_position_history, and num. The inherited transport lint and
position_adapter source files are byte-unchanged versus the original parent;
see inherited-preservation.json. The existing parity-check binary was not rebuilt.
Dependency-inclusive lint and scoped Rust parity are explicitly failed gates.

Not run: workspace compilation or suites, other venue suites, wasm compilation/
Clippy, full-crate test suites, live venue/network tests, CI reruns.

## Native assertions and discriminating controls

Nine named tests execute39 actual-source fixtures across six native adapter paths:
Encoded, Untransformed and Transformed bodies, each with pending and immediately
ready responses. Enumeration stops at fixture39 /native comparison234; there is
no exhaustive input or host scheduling claim. Three additional source boundary
witnesses are checked against three native body phases, stopping at boundary3.
The source header records arm64 /Node v26.7.0.

Native Order, HistoricalOrder and Execution fields, optional-field absence,
retained rows and unchanged Store values are asserted before any request/clock/log
wire comparison. Expected enums use literal-to-enum matches; outputs are never
re-encoded for assertions. Wire numbers compare native binary64 values rather
than JSON integer-versus-float spellings. Source input is captured before source
Store mutation. Fixed source anchors include amount0.12, remaining0.1, fill
qty0.07, price101.5, timestamp1700000001, duplicate fill fee0.55, the page50 guard,
and the independent numeric-unmapped/string-unmapped/numeric-own-lookup0 /1 /1 counts.

All13 controls modify production source temporarily, require a named runtime
assertion failure (not compilation failure), restore original bytes, and require
the same test to pass. Controls cover await scheduling, numeric fallback type,
fetchOrders delegation, retry backoff, pagination limit, normal endpoint, TPSL
cursor, trigger query, historical sort, fill contract multiplication, per-fill
ID deduplication, NaN query limit and negative time overflow. No survivor was
observed through control13. Source/test byte preservation was measured afterward.

## Review and deliberate boundaries

Static sub-agent review at2e8d identified two P2 gaps: unmapped numeric instrument
IDs were coerced before strict symbol comparison, and immediate adapters could
complete one family before starting others. Both are corrected at afcc17b with
independent source/native regressions and broken/restored controls. The final
sub-agent review is clean and bound to afcc17b; the reviewer ran no gates.
See review.json for findings, resolution and review limits.

Per-row notes retain the CHOSEN distinctions: owned retained snapshots and typed
closed models replace JS aliases/open fields; generic host failures use Display;
JSON-only response ingress refuses unsupported values; complete retry failure
returns an owned Store snapshot. Outstanding native futures drop on rejection,
whereas JS promises can keep running. A fulfilled/rejected REST await now yields
before continuation, including immediately-ready adapters.

Historical fills preserve raw instrument fallback types, field aliases, quantity
contract multiplication, time normalization, API-code checks, unsupported-operation
fallback only on empty raw rows, per-fill ID last-value deduplication and stable
newest-first ordering. Source witnesses for NaN limits, negative-time overflow
and numeric commission assets establish explicit native representation refusals;
Nonfinite query values fail before requests; unsupported commission assets are caught, emitted and return empty fills. They are boundaries, not silently normalized JSON values.
