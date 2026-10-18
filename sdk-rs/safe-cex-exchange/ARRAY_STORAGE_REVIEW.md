# Canonical native arrays

`Js::Array` retains one `Rc<ArrayData>` containing indexed and named property
slots, virtual uint32 length and its writable flag, the supported symbol slot,
prototype and extensibility. There is no mutable Vec API, alternate array case,
per-array property mirror or compatibility dereference. Holes have no descriptor;
own undefined has a descriptor. The realm cache retains weak prototype roots,
not copies of array values.

Get/Set/Define/Delete, integrity and for-in use these slots directly. Inherited
accessors keep the original receiver. Defining length performs the separate
ToUint32 and ToNumber conversions before reading the current length descriptor,
so conversion reentry is visible. Shrink deletes indexed properties in descending
order and preserves a blocked index and the requested final readonly flag.
4294967295 is a named key; the greatest index never requires allocating a dense
vector. Frozen setters can run, while failed strict data writes/deletes escape.

Installed Array methods and prototype aliases use the passed receiver. Callback
algorithms capture length, then read each reached property with live Has/Get;
sort snapshots its values before comparison and performs checked final writes.
Iteration retains the canonical receiver and reads its current length on each
step. Symbol.iterator lookup observes its own/inherited descriptor and original
receiver. Fallible array snapshots are explicit DTO/owned/codec boundaries; they
are not a second live storage representation.

The atomic compiler migration includes Backpack's associated REST array host,
Binance's EventArray and public event reads, the HL DTO bridge, checked native
orderbook splices, custom iterator adapters and the browser DATA materializer.
Backpack retains Js itself and exposes checked length/element/Set operations.
The browser creates/caches the destination before reading enumerable children,
including named array fields, preserving holes, original aliases and cycles.

`tests/js_array_storage.rs` uses the actual installed Object, Array and iterator
bindings. Its independent Node program covers descriptors/key order; native
NaN/infinities/-0 and identity; frozen mutations; holes/inherited getters;
symbol overrides; double length coercion/reentry/original throws; sparse uint32
bounds; inherited setters; hidden for-in shadows; live map/slice/sort/JSON reads;
actual prototype method aliases; and live ToString/ToNumber/default-hint hook order, original receivers and thrown identity. Test-only master checkpoint614ad01765 has
compiled behavioral RED receipts; no compiler error is counted as behavioral RED.
`scripts/array-storage-controls.py` independently faults readonly growth, named
properties, inherited receivers and the iterator override, requires a compiled
named assertion failure, restores exact source bytes and replays that same test.
The worker-data-host Wasm example independently compares the real browser
materializer and native structuredClone for the same array identity guarantees.

Array coercion now goes through the existing OrdinaryToPrimitive operation, including
actual Array.prototype methods and inherited overrides. The added coercion witness
compiles and fails before that cutover, then matches independent Node UTF-16 units,
negative-zero bits, hook order, receiver identity and original rejection. The
canonical Store target retains its 18 generation/alias/poisoned-row controls.
The browser `safe-cex-webworker/scripts/array-data-controls.py` independently drops
a named property in the production materializer, requires the real Wasm/JS
structuredClone assertion to fail, restores exact source and repeats it.
