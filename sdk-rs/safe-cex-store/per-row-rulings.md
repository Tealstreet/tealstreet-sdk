# Non-UTF-8 Store audit rulings

The frozen census names the retired lower-crate DefaultStore implementation.
The production constructor and JS facade now share the canonical graph in
`safe-cex-exchange/src/js_runtime/native_store`. These verdicts reconcile the
original claims against that shipped binding, rather than treating the historical
notes as evidence of a surviving divergence.

| Census row | Verdict | Production observation and reason |
| --- | --- | --- |
| 235 `DefaultStore.addOrUpdateOrders` | ACCEPT | Actual incoming objects remain the inserted objects; existing rows keep identity and accept unknown properties, own undefined and opaque references. The four stored/incoming cardinality combinations preserve first-versus-last duplicate-ID selection. The earlier typed-overlay claim no longer describes the native binding. |
| 464 `DefaultStore.updateOrder` | ACCEPT | Raw patches retain arbitrary fields, own undefined, UTF-16 text, nonfinite numbers and nested object identity. Earlier writes survive a later throwing getter; the exact thrown object escapes without notification or revision increment. Explicit typed Rust patches still use model admission. |
| 465 `DefaultStore.updatePosition` | ACCEPT | The same open native mutation and partial-write observations hold on positions, including the original row and array references. Typed owned projections do not replace these referents. |
| 466 `DefaultStore.updatePositions` | ACCEPT | Real batch calls retain open fields and shared objects, preserve duplicate-ID selection at all four thresholds, leave unmatched rows untouched and notify once. Throwing getters preserve earlier writes and suppress the later revision/notification. |
| 467 `DefaultStore.updateTicker` | ACCEPT | Raw ticker patches retain arbitrary non-info fields and native values, update the original dictionary/array row, and strip info exactly as TS does. Getter failure preserves the earlier field write and skips notification. |
| 468 `DefaultStore.updateMarket` | ACCEPT | Actual market patches retain arbitrary fields and opaque references on the original array/dictionary row; partial writes and thrown-object identity match TS. The old unknown-key omission describes the separate typed patch API. |
| 469 `DefaultStore#updateInArray` | ACCEPT | Exercised through the public order, position, batch and market methods. The canonical native algorithm performs reached property reads and writes directly, retaining arbitrary keys and partial writes. `model_patch::apply` is the separate staged typed-input admission API, not a precondition on raw native mutations. |

The existing `canonical_native_store` target now includes
`audit_mutation_rows_preserve_open_fields_identity_and_partial_writes`. It runs
38 independently counted profiles against the maintained TS Store via the existing
`js_store_live_view_source.mjs` loader: 25 number/operation combinations, five
throwing-getter paths, and eight batch threshold combinations. Observations inspect
own-key order, own undefined, UTF-16 units, binary64 bits, object/array identities,
dictionary referents, subscriber counts and update revisions before JSON output.
Inserted order identity is asserted independently of the comparison descriptor.

These are ACCEPT verdicts because the claimed native-binding loss is absent on
current master; there is no production FIX in this batch. Explicit typed Rust
model projection/admission remains separate and fallible. This does not close
other Store rows, arbitrary descriptor/proxy semantics, or other mutation paths.

The witness is controlled by separately compiling three production corruptions:
drop own-undefined extension fields, copy a nested extension object, or read every
patch getter before writing. Each must fail the named Store audit assertion, then
pass after byte-exact restoration of `native_store/graph.rs`.

This batch reconciles 7 of the 27 Store rows. The other 20 Store rows and the
separate num row remain assigned for later batches.
