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

The original four independent production faults compile, fail the named native regression,
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

Wasm validation uses the portable health feature set without the native host.
The exchange Cargo manifest explicitly excludes its native runtime on wasm32;
native all-features and portable Wasm builds are separate supported branches.

## Final merged-head checks

After normal inheritance of merged #1110 and master4541bd2b33, the owned
Store/splice/library/client bodies remain byte-identical to the restored proof
snapshot. The scoped replay passes 38 native checks, both real Store composition
tests and the source-backed setting assertion. Strict native Clippy covers all
features and targets of exchange, Binance, Bitget, WOO and webworker with warnings
denied. Formatting, cancellation inventory (6 checks; the explicit exporter is
intentionally ignored), CI wiring (376 tests), extractor check (zero drift), and
the full parity index (7563 rows /16351 public Rust items) all pass.

Reproduce the independent compiled controls from sdk-rs with
`CARGO_INCREMENTAL=0 python3 safe-cex-exchange/scripts/store-correctness-controls.py`.
The runner rejects compiler errors as RED evidence and restores exact source
bytes before each GREEN replay. The PR body records the final Wasm lint receipt.

## Terminal-memory controls

Five additional controls target the canonical graph through the actual Store
facade. Existing terminal witnesses retain status/commit ordering, stale-snapshot
filtering, subscriber response/cause/origin metadata and unrelated row identity.

| Control | Required compiled failure |
| --- | --- |
| `terminal-registration` | Facade no longer exposes callable `markOrdersTerminal` |
| `terminal-memory` | Suppressed terminal-memory write forgets an ID before its order arrives |
| `terminal-empty-store` | Empty-Store shortcut loses the remembered ID |
| `terminal-subscriber-error` | Discarding notification failure returns success |
| `terminal-id-replacement` | Lossy UTF16 replacement filters the wrong original row |

Each requires its specific native assertion, rejects compiler errors, restores
the production bytes and passes the unchanged witness again. The UTF16 control
uses #1180's source-proved raw-ID behavior rather than typed String refusal.
All five produced named compiled REDs and exact-byte restored GREENs; receipts
are in `/tmp/okx-terminal-core-controls/`.
The OKX private-frame terminal/fill ordering witness remains in its CI-wired
`exchange_binding` target.

From sdk-rs, run only these five controls with:

```sh
CARGO_INCREMENTAL=0 python3 safe-cex-exchange/scripts/store-correctness-controls.py \
  terminal-registration terminal-memory terminal-empty-store \
  terminal-subscriber-error terminal-id-replacement
```
