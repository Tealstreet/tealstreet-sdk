# Selected public ticker lookup

The maintained WOO public socket reads one normalized symbol from
`store.tickersBySymbol`, then calls `updateTicker` with the captured value. The
native writer previously projected every ticker into typed records for each
eligible input row. Besides unnecessary work, this invoked unrelated getters and
could abort an otherwise valid update.

The writer now uses the existing `Store::lookup_ticker_by_symbol` capability.
The canonical Store captures the selected native value without a dictionary copy
or DTO conversion. The writer checks its truthiness and reads only its selected ID. An ID getter
throw still takes precedence over a deferred typed-row refusal. No Store API, additional cache, runtime task or typed
field admission rule is introduced.

The actual constructed native exchange has three named regressions in the
existing `binding_streams` target. Each installs an enumerable throwing getter on
an unselected ticker key, sends a real socket frame, and checks that the getter
stays unread, the selected row retains identity, its field updates and no error
is reported. Direct access afterward proves the same getter throws the original
opaque value. All three compiled and failed on unchanged master `890f341bb7`
with one unselected getter read; they pass with the selected lookup.

The independent source probe constructs the complete maintained WOO exchange and
DefaultStore. Its three observations are fixed rather than inferred from the
native result:

| Topic | Unselected reads | Selected alias retained | Updated field |
| --- | ---: | --- | --- |
| tickers | 0 | yes | last = 117.5 |
| bbos | 0 | yes | bid = 116.25 |
| markprices | 0 | yes | mark = 119.125 |

A separate constructed-socket control checks that an ID getter throws its same
opaque object exactly once, including when the incoming row has an invalid
numeric field. All three maintained-source topics throw the original value; the
unchanged-master native control and the selected-lookup implementation agree.
The runtime test observer clones the reported error without reading its cause.

The complete constructed stream target passes 41 tests, including the existing
72-case ticker coercion corpus and its 54 explicit malformed-row refusals.

## Release measurement protocol

The original WOO tape has 260 frames, including 60 each of tickers, BBO and mark
updates. Those three topics contain 44,880 input rows in total; the recorded
initial futures response contains 222 tickers. No tape scaling or response
substitution is used.

Both immutable addons use Rust 1.99.0, `CARGO_INCREMENTAL=0`, the same Cargo.lock,
`cargo build --release --locked -p safe-cex-ffi --no-default-features --features node --jobs 1`,
opt-level 3 and disabled debug assertions. The control is source `890f341bb7`,
addon SHA256 `e8020b05fa3174ed408d65e175617e0300a19062c4d9eed1b3fd28aaab4607fe`.
The candidate is source `c7b52975ac83`, addon SHA256
`76e22c417665eae022ad340b677c287f063255f3d214803c6c9a895a0ef6e2be`.

The maintainer-approved timing gate is one complete control/candidate pair,
each with two fresh native and two fresh TS workers. CPU is primary; all CPU and
wall samples and shared-load bounds remain in the local reports. This sample
count does not support a three-pair inference. The complete observable-graph
checker validates retained outputs across repeats and both native variants,
including ordered keys, descriptors, prototypes, signed zero and bidirectional
alias identity. SHA256 checks fixture integrity, without conflating V8 number
encodings with observable differences. Parser, physical transport, classified
raw-task and pending-job drain receipts must all qualify before timing is used.

Release profiling, timing and graph results are pending the priority formal OKX
pair window. No WOO performance improvement is claimed yet.
