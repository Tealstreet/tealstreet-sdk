# Blofin public subscription startup and connection watchdog

Ten pending rows moved: **9 ported / 1 divergent**. This slice supplies the frame
limit/builder, connect/startup, dynamic market additions, ping, subscription send,
and connection notice arm/reset bodies. The full public facade, constructor,
inbound dispatch, ticker/funding handlers, listeners and disposal stay pending.

The actual integration parent is **157f83240a85280c25df395ba6a51225813fe9bf**.
`tomllib` stopped after seven Blofin manifests / 125 rows: before **19 ported /
71 divergent / 35 pending**, after **28 ported / 72 divergent / 25 pending**.
Census is separate from behavioral proof. No private lane branch was inherited;
integration ancestors are not independently reviewed by this lane.

## SHA-bound checks

Initial implementation/source gates: `1a92bae2a9473d13c0478c0b468dba13f4ff6e34`.
Corrected implementation, Cargo/source gates, and clean static review:
**`e5492384223639eb1a6034052919dec0099aa670`**.

Exact commands, measured exits and raw output are in
`public-stream-evidence/corrected-e549238/`. Cargo commands ran serially, crate
`safe-cex-venue-blofin`, jobs 2:

| Gate | Measured exit/result |
| --- | --- |
| Cargo check, all targets | 0 |
| Crate-only host Clippy, all targets, `--no-deps -D warnings` | 0 |
| `public_stream_core` test binary | 0; 10 passed |
| New source/test Rustfmt check | 0 |
| Extractor check | 0; output confirms 0 updates/additions/orphans |
| Maintained WS-public declaration coverage | 0; stopped at 1 file / 18 declarations |
| Blofin-only Rust parity audit, existing binary | 1; exactly 3 inherited position-adapter findings |

The Rust parity binary was not rebuilt this slice. The three inherited targets are
`BlofinPositionAdapterDeps`, `map_blofin_position_history`, and `num`; parent diff
for that source exits 0. Production symlinks, empty exemptions, real qualified
Rust symbols and canonical extractor-owned TOML serialization were used. No
facade target or exemption was fabricated to hide pending bodies.

## Source and native evidence

`tests/public_stream_core_oracle.cjs` executes maintained TypeScript arrow bodies,
the actual framing helper, actual WS URLs and notice literals, and lodash flatten.
Base/socket/emitter/clock/deferred-timer capabilities are injected. The Rust host
interface supplies those same boundaries; this does not prove live socket
composition, stable callback wiring, or the pending facade/inbound bodies.

Enumeration stopped at **19 framing cases, 46 host cases, and 1 alias witness**.
Independent literal group floors and 10 named native tests assert retained topic
properties/order, socket assignment/endpoint, ping numeric bits (including -0),
notice handles/flags, decoded frame operations/fields, and failure state before
wire/call-trace comparisons. The emitted-frame decoder is test-only `Vec<Value>`,
independent of production `SubscriptionTopic`; ingress topics are also asserted
before framing. No production-schema roundtrip is used as decoded-value proof.

The 200-topic packing expectation comes from literal JSON vocabulary plus input
IDs 0..199, not builder output: 28 container bytes, 37 fixed bytes per topic,
decimal ID length and commas. It predicts two frames, 101/99 topics, 4058/4086
bytes. The expectation derivation is retained in
`independent-frame-expectation.json`.

Matching behaviors include UTF-8 strict byte overflow, NaN/infinite/fractional
budget comparisons, ordered greedy batches, build-all-before-send refusal,
duplicate additions, separate Store reads, integer-own-key enumeration,
subscribe-before-ping, off-before-on, guarded ping after disposal during send,
truthy-only timer guards/reset, and notified-before-log-before-forceToast warning.
Timer callbacks must run after timer allocation returns; host truthiness supports
both zero numeric and object-valued handles. The JSON data-object/shared UTF-8
boundary excludes getters/toJSON callbacks, cycles, BigInt and lone surrogates.

The one divergent row is **CHOSEN** `onMarketsChanged`: Rust copies owned topic
records, while JavaScript concat retains old topic-object aliases. The actual
source witness changes the old record to MUTATED and changes the new array's
first topic; native retains ORIGINAL. Owned records prevent external alias
mutation of an active subscription. Other matching rows are marked ported.

## Review corrections and retained attempts

Static sub-agent review initially found fixed topic field order, a same-schema
frame decoder, and an unmatched control pattern. The correction retains own
input key order and extra JSON properties, uses canonical shared JSON.stringify,
adds reversed-order/nested-extra/numeric-key witnesses, decodes emitted frames
independently, and preflights all control targets. Clean review is bound to e549;
reviewer ran no Cargo, oracle, mutations or live integration and made no edits.

Dirty development compile exit 101 was an import-path error; initial writer exit
1 was an unsupported layer string. Both were corrected and logs retained.
A development test run passed 10 tests before review corrections; it is not a
final SHA-bound gate. Early oracle binding/partition mistakes were corrected;
their stderr was read in tool output, not retained as raw log files. Initial
source gates are retained at their original SHA, never relabeled.

Not run this slice: dependency-inclusive host Clippy, wasm compilation/Clippy,
workspace builds/suites, full crate suites, live network/venue/socket tests or CI
reruns. The scoped check does not claim workspace compilation. Raw logs retain
original output, including terminal blank lines.

## Runtime controls and merge preservation

All **11** deliberate runtime controls at
`e5492384223639eb1a6034052919dec0099aa670` failed the named assertion with exit
**101**, then passed after restoration with exit **0**. Enumeration stopped at
control **11**, no survivors and no compiler-only catches. Commands, call sites,
results and broken/restored raw logs are in `public-stream-evidence/controls/`.
The additional paired deserialize-swap/encoder-inverse control failed
`native decoded own-property value` on native ingress, before framing or wire
comparison. Its wire invariance is by construction, not independently measured
under the broken state; no wire check was used to catch it.

Source/tests/crate manifest/parity mapping restoration diff against e549 exits
**0**, retained in `mutation-restoration.json`.

Integration **`5c3c55b7b5631e9d04b3d31bc98b213b5888f18d`** was merged down at
**`c09b7d3d7447b2c490981989bf40e234fd730f66`**. Scoped Blofin source/test/config/
manifest diff against e549 exits **0**. Shared crypto source and Cargo.lock
changed among inspected dependency inputs; exact scope and changes are in
`merge-preservation.json`. Earlier gate and mutation measurements retain their
original SHA. Changed-input checks are measured separately at the merge SHA.

Completed merge-SHA checks at c09: scoped all-targets check **0**, crate-only
host Clippy **0**, named slice tests **0 / 10 passed**, formatting **0**, extractor
**0** (output: zero updates/additions/orphans), source coverage **0** (18 rows),
and tomllib census **28/72/25**, ten moves. Blofin Rust parity remains **1** with
only the same three inherited findings. Exact commands, exits and output are in
`public-stream-evidence/post-merge-c09b7d3/`.

Static preservation review is clean at c09 and recorded in `review.json`.
Reviewer did not audit untracked documentation/evidence or independently execute
the controls. The delivery evidence commit must change only documentation and
retained artifacts; it does not relabel c09 gates or e549 controls as measured at
the later delivery SHA.
