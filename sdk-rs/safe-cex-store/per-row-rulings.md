# Non-UTF-8 Store audit rulings

The seven mutation claims in the frozen census name the retired lower-crate
DefaultStore implementation.
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

## Request-outcome accounting

| Census row | Verdict | Production observation and reason |
| --- | --- | --- |
| 133 `recordRequestOutcome` | ACCEPT | Both actual native Axios factory paths increment the canonical Store counter for ordinary success, rate-limit, auth, server and network outcomes. Missing counters and a frozen counter object do not change request settlement or rejection identity. The old claim of no equivalent fallible Store/catch path does not describe the shipped binding: counter writes can fail and the factory suppresses those telemetry failures, as TS does. Arbitrary replacement of the typed Store protocol is not a missing production capability. |
| 446 `isSigningCaptureRequest` | ACCEPT | The actual factory reads the native config marker with JS truthiness before accounting. Capture-only requests execute their adapter without creating phantom venue outcomes; the marker stays outside headers. The typed HTTP host predicate is composition of that explicit capability, not a different policy. No capture-marker or counter loss is observed through either native factory. |

`safe-cex-exchange/tests/js_runtime.rs` contains
`audit_http_outcomes_keep_capture_guard_and_counter_failure_isolation`.
Its 98 profiles execute the canonical factory and Store on NativeRuntime, against
both maintained TS factories with installed Axios, the actual signing-capture
helper and maintained DefaultStore. Only the terminal adapter and external hosts
are injected. Both routed/direct factories cover ten HTTP statuses and network
failure with absent/false/true markers, eight further marker truthiness cases,
and four ordinary/capture/error cases for each disabled/frozen counter mode.

Counter values are observed as native binary64 bits before JSON, along with
original counter-reference and rejection identity, one terminal dispatch, no
Store notification, and no capture-marker header. TS independently asserts its
counter values and observations before emitting the comparison receipt.
The baseline already passes; these are ACCEPT verdicts, not production repairs.
Three separately compiled corruptions must fail the named native comparison:
count capture-only requests, skip counter writes, or notify Store subscribers
while recording. Byte-exact restoration must pass after each control.

## Request-start and socket auth diagnostics

| Census row | Verdict | Production observation and reason |
| --- | --- | --- |
| 455 `markRestRequestStart` | FIX | Both native Axios factories omitted the source request-start property before terminal dispatch. They now stamp the original native config after request accounting, using integer Date.now milliseconds. A read-only timestamp keeps its prior value and does not fail the request, matching the source telemetry catch. The typed diagnostic adapter is not used to project the live config. |
| 451 `isAuthOperationObject` | ACCEPT | The actual socket diagnostic send/receive paths parse a wire string into own JSON data before testing the four operation fields. With the stock object prototype none of those four keys is inherited, and wire JSON cannot encode getters or custom prototypes. All source auth decisions survive; the core redactor now calls the existing shared predicate instead of maintaining a duplicate condition. This does not authorize arbitrary-object projection elsewhere. |

The native `js_runtime::audit_request_start_stamp_reaches_adapter_and_preserves_best_effort_writes`
witness compares both real factory paths with maintained TS and installed Axios
across 32 profiles: four epochs and fresh, writable, read-only and own-undefined
read-only properties. It observes the timestamp before dispatch and on the
original interceptor config, exact binary64 bits, one send and interceptor order.
The compiled baseline fails on the absent property before the production repair.
The source adapter receives Axios's flattened config; the test does not claim
adapter/config identity or arbitrary accessor traversal equivalence.

`websocket::tests::audit_auth_operation_own_data_reaches_real_socket_diagnostics`
compares 44 maintained-source profiles through both actual BaseWebSocket emitters.
All four operation keys cover auth/non-auth strings, absent-string shapes and
nested argument redaction. The 88 emitted channel/message pairs preserve the
public sibling and source auth scope. The baseline already passes, supporting
ACCEPT rather than claiming a repaired predicate decision.

Four separately compiled controls remove the routed timestamp, remove the direct
timestamp, propagate a read-only telemetry error, or disable the shared auth
predicate. Each fails its named binding assertion, and byte-exact restoration of
the production file passes before the next control.



## Compressed Blob input

| Census row | Verdict | Production observation and reason |
| --- | --- | --- |
| 445 `readBlob` | ACCEPT | The real Bitrue binary socket path invokes the shared inflater through the native Blob/arrayBuffer binding, then executes its ordinary parser and ping handler. Maintained TS widens every inflated byte to a single U+0000..U+00FF code unit; Rust does the same. UTF-8 multibyte text therefore produces the same byte-widened text on both sides, rather than a Rust-only decoding loss. JSON surrogate escapes are still interpreted by the subsequent parser. The typed byte-future capability does not change the observed socket replies. |

The existing Bitrue `exchange_binding` target includes
`audit_compressed_blob_bytes_reach_real_socket_pongs_without_utf8_decoding`.
It starts the actual exchange with its canonical Store and production public
socket. Its 28 profiles compare terminal pong UTF-16 units against the maintained
TS `onMessage`, `handlePingEvent` and `readBlob` bodies with installed pako and
Node Blob. Seven payloads cover ASCII, multibyte UTF-8, every high byte, escaped
lone surrogates, escaped controls, negative zero and an unsafe integer. Each runs
as gzip, zlib, concatenated gzip members and zero-padded gzip. All replies are
observed after the actual async Blob continuation; no inflater is injected.

The baseline already passes, so this is an ACCEPT verdict. Separate compiled
controls replace byte widening with UTF-8 decoding or paired-byte reinterpretation.
Each must fail this actual socket assertion; byte-exact restoration must pass.
This verdict covers the compressed binary input boundary, not arbitrary Blob
subclasses, malformed-stream error wording or other socket handler policies.

Twelve of the 27 Store rows are reconciled here. The other 15 Store rows and the
separate num row remain assigned for later batches.
