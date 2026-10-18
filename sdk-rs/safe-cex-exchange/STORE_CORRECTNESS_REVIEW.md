# Raw Store correctness replay

The unchanged maintained `DefaultStore` and its real relative TypeScript dependencies
run in `tests/js_store_correctness_source.mjs`. Only external clock HTTP is disabled.
The existing singular/bulk/position source oracles remain authoritative (132/30/168
cases). The new Symbol witness also stores both a Symbol ID and its text spelling,
then observes independent removal, indexed updates and conversion counts.

Current master ada5a6c759 compiled all five reported targets RED, plus the setting
assertion in js_runtime. Receipts: `/tmp/store-correctness-five-master-red.log` and
`/tmp/store-correctness-master-red.log`. Contrary to the initial routing summary,
the three ID logs show native TypeError versus a successful source call.

* `removeOrder`: source store.base.ts:691 uses splice on the original array. The
  installed callable member was missing. This change follows #1110's canonical
  ArrayData cutover (92e26fa45f), using its single checked mutation operation.
  The caller passes length captured before argument coercion and deleted getters;
  native compiler-required callers pass their own current length. The Node splice
  corpus observes holes, coercion, deleted values, reentrant length changes and
  original references; frozen writes still fail. No alternate shifting body exists.
* `buildIdLookup`/`buildIdIndexMap`/`findIndexById`: source lines96–120 use computed
  property keys. ToPropertyKey preserves Symbol primitives, including ones returned
  by object conversion, instead of forcing them through ToString. Null-prototype
  dictionaries remain ordinary native property tables (no quadratic Map replacement).
* `setSetting`: source line1044 assigns arbitrary runtime keys and only notifies
  when the value changes. The old refusal assertion was stale. #1110 supplies
  that fixture correction; this branch adopts its exact js_runtime.rs version
  instead of carrying a second edit.
* Raw `updatePosition`: source line1054's for-in assignment retains own undefined,
  callable values, null and unknown keys. Typed reads still refuse unrepresentable
  fields, while a later valid raw patch restores typed readability. The numeric
  bit corpus and opaque native info assertions remain in place. This changes the
  fixture; it does not weaken typed PatchSource admission.
* Raw terminal memory: source markOrdersTerminal retains UTF16 IDs. The fixture
  now proves that D800 memory filters only that ID, leaves U+FFFD untouched and
  preserves the surviving original row. It no longer demands a typed String
  refusal from the raw facade. Typed String decoding remains a separate boundary.

Coordination: dwfl8p confirmed its #1131 owner/facade cache changes touch mod.rs,
not these graph algorithms. That owner capability is unchanged. yxekk5 approved
the direct captured-length splice signature cutover and prototype registration;
all other #1110 storage bodies remain unchanged.

## Compiled controls

All four independent production faults compile, fail the named native regression,
and pass after exact byte restoration: force Symbol through ToString; remove the
installed splice member; reread length after coercion/deleted-slot getters; move
upsert key coercion ahead of the source RHS length read. Logs
are `/tmp/store-correctness-controls.log` and its per-case directory. The source
methods and test expectations stay unchanged during these controls.

An additional actual-Store upsert profile pins RHS/key-conversion ordering: the
source reads the current array length before the final assignment-key coercion.
A key hook appends a row, and the next update exposes the resulting source index
choice. Native observations compare row IDs, amounts and both hook calls before
JSON. This preserves the maintained behavior rather than correcting its quirk.
