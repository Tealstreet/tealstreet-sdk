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
`adapter_classes::native_rest_array_host_retains_owner_and_inherited_getter_completion`
invokes the constructed Backpack exchange's real trigger-repair method. It checks
original array identity, inherited index getter receivers, holes, named aliases,
original getter rejection and the absence of HTTP writes. Its separate
`scripts/array-host-controls.py` removes the production host's index Get, requires
the named compiled failure, restores exact source and replays the same witness.
Borrowed `Array.prototype.toString` uses the shared intrinsic object-tag helper
when join is non-callable; nullish receivers fail before reading join. Six
non-array tags and both exact nullish errors compiled RED before this correction
and then matched Node, alongside the unchanged original-array tag.
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

The actual Wasm example also tests sparse length 2^32-1: push, splice at the
end and a splice that shifts a hole each compiled and failed on usize arithmetic.
Intermediate write indices now use u64 and final length uses exact f64 addition,
so the existing canonical ArraySetLength produces RangeError after precisely the
same named/indexed writes as JavaScript. The Node witness compares these partial
completions, own properties and negative-zero bits without cloning the huge sparse
array. All three independent old-body Wasm RED logs are retained with the proof.

## Final cutover validation

The replay after normal inheritance of master `f5309ca181` passes 15 native/Node
array tests, 102 runtime tests, 18 canonical Store tests, five retained-error tests,
22 Backpack adapter/rejection tests, two Orderly structural-alias tests, six
Orderly ticker row/subscriber tests and 17
Kraken member/page/receiver/boundary tests. The real binary-delivery test retains
Blob bytes. Cancellation (six) and override (seven) pass with only three existing
explicitly gated controls ignored across the core targets. The incoming Kraken
and Orderly fixtures unwrap checked array snapshots without changing their row
or subscriber assertions. The setSetting fixture observes the maintained Store
retaining its runtime key; the dial-header fixture waits for actual Open before
close and retains every header assertion. Earlier WOO history-row (two) and
Phemex lifecycle (eight) replays also passed on the prior merged parent.

Before the final master refresh, all 22 venue crates passed the all-feature,
all-target cfg(test) compiler check. Strict all-target Clippy passed for the
28-crate native matrix and the 28-crate Wasm matrix. Node FFI passed separately
with its node feature because enabling its node and wasm features together is
intentionally refused. Layer checks, manifest freshness and all 376 CI wiring
assertions also passed. After the final refresh, the touched four-crate strict
all-target Clippy, full parity index (7,563 rows / 16,353 public Rust items),
workspace formatting and all 376 wiring assertions pass before READY. The cached full compiler/native/Wasm matrices are replayed after
auto-merge is queued, following the maintainer's bounded-refresh instruction;
their earlier results are not relabeled as measurements of the refreshed head.

These compiler and lint checks supplement the behavioral controls above; they
do not count as behavioral RED evidence. Final accepted metadata is derived from
fresh compiler artifacts, with reviewed source-item fingerprints at each changed
receiver and the existing canonical contract runner for selected core decoders.

Fresh native compiler spans identify 62 changed receiver policy records across
ten reviewed implementation blocks: Backpack NativeValue (one), Kraken ReadHost
and SpotCancelHost (ten), Bybit SnapshotHost and SessionHost (18), Bitget
RestActionsHost, CancellationHost, OrderReadsHost and PrivateTopicsHost (27), and
Binance EventOps (six). Pins cover the complete enclosing impl, so unchanged
methods in a changed block share its refreshed fingerprint. Classifications stay
unchanged. Bitrue's registered impls and the two Binance trait-default bodies
match their existing pins and are left untouched.

The reviewed differences replace mutable Vec access with canonical owner reads,
Has/Get or checked snapshots, propagate original getter failures, and route
iterator state through the shared iterator record. Backpack returns the original
Js array from NativeValue::array. Existing scalar coercion profiles, raw-decoder
obligations, receiver binding and error selection remain independently classified.
