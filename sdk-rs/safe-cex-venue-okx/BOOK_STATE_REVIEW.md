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

The first three native probes fail on master `523e1ebf0d`. All six source
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
vary. The coordinator reduced this residual slice to one full matched pair,
because #1218 owns the dominant non-book routing gain. Run the original
285713-frame OKX tape once per side, control followed by candidate, without
local Cargo or profiling during feeds. Keep both independent native repetitions
and both TypeScript controls in each report. CPU is the primary metric; report
all samples, medians and ranges, with wall time and shared-load bounds separately.

`tests/book-state-throughput.mjs` requires both reports, verified complete
physical/application drains, identical input hashes and complete observable
native graph identity across every control and candidate output, plus complete
TypeScript repeatability across all reports. It imports the
shared comparator, including its signed-zero and alias negative controls. Raw
V8 SHA values prove artifact integrity, not JavaScript semantic equality. No
field normalization, report projection or partial-drain comparison is allowed.

## Measured residual result

The approved single full pair completed on 2026-10-19. Control source
`6effe3eeb90e` and selected-state source `1e31216db438` differ only in
`src/binding/sockets.rs`; both include the same #1218 routing. Rust 1.99.0,
opt-level 3, debug assertions off, Node 24.20.0, lockfile and dependencies match.
Both release addons loaded and completed the original full tape.

Seconds below are process CPU and feed-to-drain wall time. Each cell retains
both independent worker samples, followed by their median in parentheses.

| Side | CPU seconds | Wall seconds |
| --- | --- | --- |
| Native control | 207.932, 202.031 (204.981) | 759.038, 733.627 (746.332) |
| Native selected state | 196.197, 195.757 (195.977) | 596.870, 586.730 (591.800) |
| TypeScript with control | 58.061, 58.935 (58.498) | 483.093, 492.720 (487.907) |
| TypeScript with selected state | 54.689, 54.807 (54.748) | 371.417, 367.163 (369.290) |

Observed native CPU median decreases 4.39%; wall median decreases 20.71%.
TypeScript CPU also decreases 6.41% between reports. This is one matched pair
under changing shared load, with two native samples per side, so these numbers
do not establish an isolated causal speedup or three-pair confidence. Candidate
CPU is 3.58x its TypeScript control; the <=2x CPU target remains open.

Every worker receives all 285713 frames with zero drops. Both native repeats per
side verify physical and application drains. All four complete native retained
graphs match, including ordered keys, descriptors, prototypes, signed zero and
bidirectional alias identity; all four complete TypeScript graphs repeat too.
Each graph contains 607770 objects/arrays. All eight comparator controls pass.
Native V8 bytes differ for equivalent positive-zero encodings; per-file SHA
checks prove integrity, while the unchanged observable graph comparator proves
identity without normalization. This comparison is between native control and
candidate, with TypeScript repeatability checked separately.

The conservative formal window is 08:58:00-09:23:40 UTC; actual feed/report bounds
are 08:58:13.766-09:21:49.063. No local Cargo or profiler runs during the feeds.
Neighboring Cargo gates and WOO/Backpack feeds overlap the control; Phemex/WOO
builds overlap the candidate. Their reported bounds and unknown starts remain
in the local evidence. This slice excludes #1218's non-book routing improvement;
the combined effect requires the post-merge full-master replay.

Local ignored evidence is under `.cache/okx-book-state-release/` (protocol,
shared-load notes, both original reports, retained-output integrity hashes and
complete-graph comparison) and `.cache/okx-book-state-artifacts/` (immutable
addons and matching release/source provenance). Generated reports and binaries
are not committed.
