# Structured-clone native-input classification

The recorder calls `js_runtime::structured_clone::structured_clone` from
`PerfFixture::record` at the actual worker post choke point. The entry allocates
one memo table; private `copy` fills that table before descending into children,
so cycles and repeated references resolve to the same detached output graph.
Private `hidden` only installs writable, nonenumerable, configurable data slots
on the newly allocated Error. It neither interprets raw values nor admits a
second codec, and its target is never caller supplied.

The two graph readers have exact compiler API and ordinary executed decoder
contracts. `hidden` has an exact API non-decoder classification as the internal
descriptor writer. Error profiles execute that writer through the same clone
entry and inspect actual descriptors, presence, UTF16 and reference identity.
No pending-row record supplies execution permission or bypasses discovery.

## Node observations

`scripts/structured-clone-oracle.cjs` calls Node's actual `structuredClone`.
Eight profiles independently cover primitives and BigInt, both zeros, NaN and
infinities, lone-surrogate strings, cycles/shared Map and Set references, sparse
arrays and named fields, live getter effects/order and original getter failure,
intrinsic Errors, own undefined, and callable/Symbol refusal. Values are observed
before JSON; JSON carries only the resulting units, flags, bits and identity
comparisons. All non-NaN bits remain exact. Every NaN compares as NaN, consistent
with the approved engine-dependent NaN-bit ruling.

The existing `worker_structured_clone_matches_node_graphs_and_refusals` witness
also covers Promise refusal and Error-cause cycles. The actual listener
`native_perf_record_dump_replay_matches_maintained_listener_roundtrip` reaches
the same entry through recording, dump and replay. These finite witnesses do not
claim support for arbitrary host/platform objects: the approved recorder scope
returns visible DataCloneError for unsupported native kinds; the recorder drops
clone failures exactly as perfFixture.ts does.

## Accepted stack representation

Node 24.20.0 / V8 exposes a lazy getter/setter for cloned Error.stack. Native
stores that supplied value as a writable data property. Overseer approval accepts
only this engine-specific descriptor-kind difference; no recorder consumer
branches on it. Profiles still require exact supplied UTF16 values, replacement
writes, own undefined, nonenumerability/configurability and exact message/cause
data descriptors. This does not close the separate missing engine-generated
stack residual. The ruling is recorded beside that residual in the audit ledger.

The three introduced-on-master rows are ACCEPT for these reviewed roles and
bounds, with ordinary contracts and native observations. No production behavior,
input guard, depth limit, receipt policy or JSON boundary is changed.
