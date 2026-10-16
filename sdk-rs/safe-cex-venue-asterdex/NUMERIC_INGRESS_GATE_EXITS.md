# Asterdex numeric ingress evidence

Measured production/test SHA:`f537e927156e358414fbe202ab807349c7251014`. Fresh integration parent:
`ac0f5fccf24207b26c279e67da2a88f2c72a7aae`. The GRVT and Asterdex stream
feature branches are not ancestors.

Tomllib:130 ported/92 divergent/9 pending ->131/91/9. Eighteen numeric
FINDINGs close; the twelve-field kline tuple becomes ported, while seventeen
object records retain measured CHOSEN depth costs. Zero pending rows moved.
FINDING-containing notes:30 ->12 on this branch. Seven of those twelve are
addressed separately by reviewed PR7859; this branch does not claim its changes.
Census and proof are different claims.

The row notes cite [numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect)
and [the taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and name the
opaque `extra` fields plus shared js_deserialize_passthrough operation.

## Measured gates

- Scoped all-target `cargo check -p safe-cex-venue-asterdex --jobs 2`:0.
- `live_types` target with ignored live-source tests enabled:0,51 tests, Node24.20.0.
- `live_exchange_helpers order_mapper_`:0,two tests, including direct ordinary
  overflow ingress. Its former refusal control is now a positive fixture, not a
  claimed regression guard.
- Native numeric floor:68 fields across22 source shapes(last field67,shape21),
  six literals(last5),seven routes(last6):raw str/slice,owned/borrowed Value,
  shared str/owned/borrowed APIs. Native f64 bits are checked before output wire.
  The explicit native Value(-0.0) paths remain measured too.
- Opaque extension floor:six maps(last5),six literals(last5),seven routes(last6),
  using js_value_as_f64 on the decoded native Value before output wire.
- Depth cost:17 object rows(last16),seven routes(last6). Actual-source JSON.parse
  and AST-derived record contracts accept140-level extensions; all measured native
  routes refuse with recursion-limit errors. The source generator creates21
  object shapes(last20),but the native cost claim stops on the named17 roots.
- Isolated oracle copy with no ancestor node_modules,empty NODE_PATH and global
  search disabled:0;22 shapes/68 fields. Missing compiler control:1,
  MODULE_NOT_FOUND. Its first copy omitted shared source types and failed1 with
  ENOENT; completing the four-file input copy produced the reported pass.
- Rustfmt on production types and the two touched native fixtures:0.

## Compiled controls and restoration

At1ed456f483,three isolated temporary production mutations failed101:

- Flattened amount.abs():`AsterDEXPlaceOrderOpts.amount raw-str -0` and
  `numeric ingress changed Number bits at $.amount`.
- Replacing the named opaque capture with Value::deserialize:
  `AsterDEXMarketResponseFiltersItem opaque raw-str 1e400` and
  `numeric ingress lost Number at $.opaqueNumericExtension`.
- Restoring the actual legacy websocket decoder:
  `native depth guard must refuse AsterDEXWebSocketMessage/owned`.
  Before that failure,the decoded native known number,140-level payload and leaf1
  are asserted. This is a measured legacy acceptance fixture,not a numerical guard.

Every temporary production change was restored byte-for-byte. Production types
compare unchanged from1ed456f483 to the measured SHA(exit0); restored51-type and
2-order-mapper targets passed0. Existing source negative-zero/finite-payload
controls also pass by observing their named child assertion failures.

Initial all-target compilation ata88f353763 failed101 because one mapper fixture
still constructed a raw tuple;f537e92715 changes that call site to the owned tuple
constructor and the scoped all-target check passes0. No earlier failed check is
reported as green.

## Not run

Workspace/full-crate/monorepo suites,clippy,wasm,live account/API calls,load tests
and CI reruns. No exhaustive arbitrary-input equivalence claim follows from these
finite floors. Node26 and extractor exits are recorded separately after completion.

Atf537e927156e358414fbe202ab807349c7251014,Node26.7.0 native corpus target:0,
one test with all68 fields/seven routes. This is separate from Node24's51-test
file claim. Extractor check:0,Would update0 manifests,0 added/orphaned.
The writer after the eighteen-row reconciliation exited0,Updated0 manifests,
0 added/orphaned. No rows or rust_files arrays were regenerated or hand-spelled.
