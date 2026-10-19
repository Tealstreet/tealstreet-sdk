# Client input boundaries

This batch classifies ten exact pending owners under `js_runtime::client`.
Two production fixes have independently compiled original-body RED and restored
GREEN receipts. The other eight are bounded native bridge classifications;
none grants another helper, alias, DTO or callback an exemption.

| Owner | Ruling and reached behavior | Executed proof |
| --- | --- | --- |
| `axios_headers_from` | ACCEPT: the no-proxy factory's plain-record constructor adapter normalizes native cells and installs inherited methods. This corresponds to `new AxiosHeaders`, not its separate static identity overload. | Three actual constructor profiles, including signed zero, UTF-16, null, false and arrays. |
| `header_key` | ACCEPT: reverse case-insensitive own-key selection reads names without reading unrelated header values. | Three direct native/installed Axios `findKey` profiles, including a throwing unread value getter. |
| `normalized_header_value` | FIX: invoke the original array's live `map` rather than densifying a snapshot. | Five direct profiles plus actual public AxiosHeaders construction: sparse indices, custom method/receiver, getter throw, noncallable map, nested scalar/UTF-16 values. |
| `set_header` | ACCEPT: assignment preserves disabled false, rewrite tri-state, original key casing, null and string sanitization. It is the private assignment helper, not the public overload dispatcher. | Three direct native/installed AxiosHeaders profiles; existing four header-corpus profiles and seventeen operations remain intact. |
| `js_params_serializer` | FIX: invoke `serialize` without a bound receiver and pass Axios' options as argument two. Function form gets its wrapper; object form keeps its original options referent. | Five direct native/installed `buildURL` profiles plus actual client/REST-host terminal requests for both forms. Original thrown object and UTF-16 return remain visible. |
| `factory::status_of` | ACCEPT: selected status view of the typed transport response model. Real HTTP producers provide a numeric status; synthetic non-number/zero/NaN cells do not widen that producer domain. | Nine direct model observations, including infinity and signed zero; existing HTTP outcome probes exercise the enclosing native factory. |
| `factory::same` | ACCEPT: reconciliation compares already materialized typed-provider model projections. This is neither JS reference equality nor wire-text equality. Undefined differs from null; signed-zero model cells differ; nonfinite JSON projections coincide with null. | Five direct native model observations use Node deep model comparison, separately from installed Axios source expectations. Existing real-host config witnesses preserve unchanged native cells and callback identities. |
| `create` | ACCEPT, non-decoder: constructs the native client and captures typed host/default/state capabilities. Request interpretation is delegated to its independently owned pipeline. | Actual client creation, canonical Promise result and three installed-Axios completed pipelines. |
| `spawn` | ACCEPT, non-decoder: wraps an existing typed future in the runtime-owned canonical Promise, without reading its result graph. | Actual client calls return a Promise and retain result/rejection identity through completion. Existing eager/pending/drop tests cover the shared Promise owner independently. |
| `chain_step` | ACCEPT, non-decoder: selects a callable Promise continuation and forwards the native settlement unchanged; callback bodies own their own interpretation. | Three completed pipelines cover fulfillment, original rejection passthrough and recovery, in source LIFO/FIFO order. Existing client rejection-chain tests remain unchanged. |

The canonical driver executes 33 cases across seven production helper functions
and records three explicit non-decoder classifications. `scripts/client-input-observers.rs`
is included only by tests and the canonical instrumented production copies.
`tests/client_input_oracle.cjs` runs installed AxiosHeaders, the installed `findKey`
declaration, installed `buildURL`, and actual installed Axios client pipelines.
Its status/equality rows are explicit Rust bridge-model observations, not a second
Axios implementation or a claim that its private helpers exist in TypeScript.

The array regression fails on the original production helper because holes become
own undefined properties. The serializer regression fails through the real native
client and terminal host before send because argument two is absent. Restoring
the fixes passes those same tests. No identity/alias guard or input discovery
policy changes. Remaining client owners and all shared numeric-depth findings
retain their independent pending/FIX status; these shallow profiles do not close
a general recursion, wire-frame or complete-Axios-surface claim.
