# Public socket state access

The maintained `OKXPublicWebsocket.acceptOrderBookSequence` operates on one
handler's Map and Set entries. `recoverOrderBook` deletes that sequence and marks
it recovering before it reads the selected topic. Ordinary frames do not read
the topic dictionary. Rebuilding all three collections before and after each
frame adds work proportional to every subscribed topic and changes observable
getter, mutation and failure ordering.

The constructed binding probes in `tests/support/callback_binding.rs` and the
maintained-method oracle `tests/book-state-source.mjs` establish these boundaries:

- A snapshot ignores unrelated poisoned Map values and topic getters.
- Recovery commits its delete/add before a selected topic getter throws, keeping
  the original thrown value.
- A replaced selected Map method receives its original receiver and arguments.
- Sequence guards and String conversion retain their repeated getter reads.
- Recovery retains the original live topic across both sends, including later
  mutations, extra properties and source object-key order.

The first three native probes fail on master `523e1ebf0d`. All five source
observations pass against the actual TypeScript method bodies extracted through
its AST. The initial selected-method candidate passed six native tests, including its
maintained-source oracle. A further compiled public-frame RED pins the remaining
legacy adapter: a snapshot must commit only its selected sequence before calling
the actual book callback, without enumerating unrelated topics. After inheriting
#1218 and deleting the orphan adapter, all seven state tests and the combined
handler-registry library tests pass.

The implementation must keep canonical Js state as its only material owner.
Selected Map/Set calls use the existing shared runtime methods; there is no
cached typed collection or copy-back stage. Sequence properties retain the
source's repeated reads and String call. Recovery retains its original topic
reference across the two sends. Handler lookup and statistics are owned by
#1218, separately from this state access change. That routing change also avoids
`with_book` for non-book frames. The comparison below therefore builds on the
same #1218 source on both sides and measures only the remaining book/recovery
state-copy delta; it does not attribute the earlier non-book routing gain here.

## Release comparison protocol

Freeze matched optimized release addons with the same toolchain, lockfile,
profile and dependency source. Only the selected-state production delta may
vary. Run the original 285713-frame OKX tape in serial A/B/A/B/A/B order without
local Cargo or profiling during feeds. Keep both independent native repetitions
and both TypeScript controls in each report. CPU is the primary metric; report
all samples, medians and ranges, with wall time and shared-load bounds separately.

`tests/book-state-throughput.mjs` requires all six reports, verified complete
physical/application drains, identical input hashes and complete observable
native graph identity across every control and candidate output, plus complete
TypeScript repeatability across all reports. It imports the
shared comparator, including its signed-zero and alias negative controls. Raw
V8 SHA values prove artifact integrity, not JavaScript semantic equality. No
field normalization, report projection or partial-drain comparison is allowed.

The <=2x TypeScript CPU target remains open until measured. No performance
result is claimed before this protocol completes.
