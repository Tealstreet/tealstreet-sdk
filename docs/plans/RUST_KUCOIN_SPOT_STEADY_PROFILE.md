# KuCoin Spot public steady CPU investigation

Owner: codex-9czdjh; requested by claude-ddi2il. Profiling only before a reviewed
profile and follow-on implementation plan; no performance code or merge authority.

## Pinned evidence and objective

Investigate landing `258c880656f9714f14b498e046b8ee52658e702a`. Its SDK and
safe-cex source are byte-identical to `6741c1c6609437e32090c76745a56ca6c2262445`;
the intervening landing change is documentation. Use that actual landing's CI
KuCoin Spot package, run39947696291, artifact ci-wasm-kucoinspot. Verify build.json
sourceHead and JS/WASM hashes before profiling; no local release WASM rebuild.
The shipped optimized module strips names, so anonymous function samples must be
reported honestly rather than assigned guessed Rust symbols.

The same run's strict-replay-kucoinspot artifact contains nine public pairs.
Median steady CPU is TS335.311 ms / Rust302.091 ms: TS/Rust1.10997, just outside
the ten-percent gate. Rust main CPU609.717 ms versus TS1052.121 ms; steady CPU is
the narrow objective. The owner's target is **at least1.20x median**: at the
recorded TS median Rust must reach at most279.426 ms (22.665 ms /7.50% lower).
Local profiles diagnose cost; CI interleaved paired medians decide restoration.
No timing threshold, tape, codec behavior, safety check or gate is relaxed.

## Measurement sequence

1. Profile one actual public replay using the frozen optimized CI artifact and
   Node22 through corepack yarn. Use CPU sampling and retain the JSON result,
   artifact hashes and command. Keep sampling separate from timing pass claims.
   Do not run local replay matrices or load sweeps.
2. Enumerate events and distinguish startup from the last75% steady window used
   by Tape.play. The tape has26,426 WS records:26,094 ticker,326 depth, three ack,
   two pong and one welcome; there are no command records. Correlate sampled
   host/wasm/codec/Store/frame/GC costs with real event counts.
3. Attribute the three requested shared seams using source call paths and
   measurements: #1830 command-entry, #1837 classifier and #1854 execution groups.
   Prove when entry/classification are or are not reached by these events; inspect
   shared per-event sweeps even with empty command/execution registries. Do not
   infer overhead merely from a merge-date correlation.
4. If stripped WASM samples cannot identify a shared callee, supplement with a
   temporary symbolized native diagnostic driver over the recorded payload corpus
   and unchanged landing dependencies. Keep it outside repository source, use two
   Cargo jobs, and distinguish native attribution from browser/WASM timing. Report
   unresolved attribution rather than inventing precision.
5. Send the overseer the committed plan, measured profile, per-event attribution,
   limitations and proposed shared owner/scope before performance code changes.

## Follow-on criteria

Measured engine overhead must be fixed in its shared choke point for every venue,
coordinated with its owner and open PR claims. No KuCoin-local skip, capability or
credential shortcut, or weakening exact delivery, expiry, command retention or
observer completion. A shared fast path needs adversarial empty/nonempty/expired/
queued-group controls and source preservation review. If native codec/Store/host
work dominates instead, report that evidence and request the appropriate scope.
No merge until a new window is granted. Existing venue PRs and Bitunix work stay
separate; their evidence does not establish this performance objective.

## Initial measured profile (before performance code)

Executed on the unchanged source above; no engine/venue/host/harness source was
edited. Raw evidence is under `/tmp/codex-9czdjh-kcs-profile/`:
`landing-rust.cpuprofile` (whole replay), `steady-rust.cpuprofile` (warm-mark
to measure capture, with boundary instrumentation overhead), replay JSONs, `native-sample.txt`, `native-run.log`, native
build log and machine-readable `attribution.json`. The temporary native driver
and corpus are under `/tmp/codex-9czdjh-kcs-native-profile/`.

The single optimized-WASM steady capture uses Node22's inspector at100us sampling,
started by a preload at Tape.play's warm-mark threadCpuUsage call and stopped at
the first measurement thread-CPU call, just before its steady-delta read. Starting
precedes the recorded warm CPU baseline; this is an approximate window with
instrumentation overhead. The preload does not alter tape/engine/host code.
Its518.625ms sampled interval includes301.454ms idle and26ms inspector setup;
WASM leaf samples account for49.217ms. Main self costs outside WASM include
socketFor14.294ms, active11.870ms, querystring unescape9.416ms and URL parameter
parse8.663ms. These are weighted sampling intervals, not additive exact CPU
measurements. Inspector overhead and stripped WASM names prevent an exact
per-Rust-function WASM attribution; local profile metrics are not a gate result.

The supplemental native release driver uses unchanged landing engine/KuCoin
dependencies, opt3/fat LTO/one codegen unit plus symbols, two build jobs, the real
968-market startup and the recorded final19,820 WS payloads. One12-second profile
processed15,142,480 repeated events, averaging793.32ns/event of elapsed wall
time. macOS sample captured eight seconds within that twelve-second driver run,
collecting5,872 main-thread samples:

| Requested shared seam | Steady path and measured evidence |
| --- | --- |
| #1830 command entry | Admission, begin/end loan and command-owner lookup are not reached by ticker/depth events. Ambient custody assertions in assert_live/failed still execute; they inline into generic handle/drain work, so this profile does not isolate their incremental cost. No claim of zero instructions or zero code-layout effect. |
| #1837 classifier | No read/write classifier or command write-intent call on these WS events; there are no HTTP write deliveries or command records. Changed layout/codegen effects remain unisolated. |
| #1854 execution groups | One unconditional sweep per ordinary WS event. Its empty-root expiry/managed-leg revocation/timer work totals60/5,872 samples, about1.02% native time (roughly8ns/event by wall-time-weighted sample allocation, not a separate CPU timing measurement). The sweep still scans managed HTTP state and recomputes the next group deadline when no group exists. |

Native self samples are dominated by JSON string scanning16.54%, memcmp14.63%,
shared `num::parse_decimal`10.51%, Price deserialization8.84%, and JSON parse_str
6.85%. Price/Store ticker lookup is also a substantial inclusive branch. These
are source-attribution measurements, not WASM speed predictions: the driver omits
JS host/frame costs, fixes raw time at100ms, discards timer actions, repeats the
corpus, and older depth snapshots are rejected
after the first cycle. Sampling proportions have ordinary statistical error.

#1835 discovery has no new steady ticker/depth work: its managed discovery
Context checks and retirement are in startup item/response callbacks, separate
from on_message. The source review confirms this path distinction; it does not
rule out compiler/code-layout effects from a larger module.

## Disposition and proposed next measurement

The profile identifies a shared empty-group sweep opportunity, but its roughly1%
native share does not justify claiming the required7.50% overall CPU reduction.
Do not optimize that path speculatively or disable custody checks. Coordinate any
shared execution-group optimization with its owner; an empty-root shortcut must
still revoke abandoned unsent execution legs and preserve deadline/observer
semantics. The larger observed shared numeric-parser/ticker-lookup and generic
codec costs warrant targeted source attribution next. Anonymous WASM indices
must first gain a trustworthy symbol mapping or equivalent controlled evidence
before assigning their costs to those native hotspots.

Plan and initial profile are sent to the overseer before performance code. The
1.20x median target remains unproven. No merge window is requested for code yet.

## Named WASM result and shared index implementation plan

The owner authorized a names-preserved diagnostic build with the existing opt3,
serde_json opt3 and Binaryen132 -Oz profile. Direct named steady samples show
Store::ticker_index14.164ms inclusive of47.294ms WASM ingress, memcmp13.040ms self,
Price deserialize13.085ms, parse_decimal2.084ms, sweep_executions1.083ms. This build
is not the shipped artifact: noncustom sections differ, so function indices were
not mapped between builds. Raw named capture and names-attribution.json accompany
the earlier evidence. This confirms lookup as the preferred shared fix.

Implementation base is actual landing `b627e1bf6e9857158e41e14a49e1faa05572efc5`;
the profile docs were genuinely merged at `b087d556441`. #1747 owner codex-cxxyqr
explicitly granted a coordinated store.rs ticker-index window for this standalone
PR to land first; #1747 will integrate it afterward. Preserve every metadata and
settings hunk. Other shared source files remain outside scope. Guard #1886 belongs
to claude-fe7lgm and is reviewed separately, not duplicated in this PR.

Replace only Store's indirect sorted Vec ticker_by_symbol with the same existing
HashMap<Box<str>,u32> representation used by market indexes. The map provides
borrowed-str lookup without allocation per event, and reuses existing shared
hash-map code. set_tickers rebuilds it in input order so the final duplicate wins;
ensure_ticker inserts only a newly appended row. Preserve ticker Vec order, row
indices, replacement/delisting, unknown-symbol behavior, dirty masks, signed-zero
semantics, loaded flags, funding and snapshot behavior. No venue source change,
cached venue key, new parser, custom hasher, credential or authority bypass.

Commit a generic Store lookup benchmark and compatibility controls before source.
Record unchanged-source results and checksum; baseline performance RED is the
actual nine-pair public gate miss above, not an invented functional failure.
Compatibility tests should pass on unchanged production. Compare the same corpus
and benchmark after the single source move. Benchmark numbers describe wall time;
full CI paired medians establish the >=1.20x KuCoin Spot target. Measure retained
memory and shipped WASM size for every family, particularly Aster's tight budget;
the extra copied ticker keys must not silently consume a size/memory allowance.

WOO's public WS apply_ticks already uses cached venue indices and update_ticker_at
(woo/src/lib.rs:272,295), so it does not traverse ticker_index on that steady path.
Its periodic Funding response uses symbol updates, but that is not the hot WS
branch. The overseer has this evidence for a separate WOO headroom assignment;
WOO's complete CI replay remains a no-regression check in this shared index PR.

Independent plan/source/adversarial/full-map reviews must verify that this is one
shared Store change for every venue and that all index-coherence contracts hold.
Run engine, KuCoin and Binance native tests, strict scoped lint/fmt and bans.
Fresh full CI across all venues is required; no source-equivalent shortcut for a
shared-crate change. No merge without an exclusive engine window and fresh guard.

Plan review refinements: measure peak replacement memory as well as steady
retained bytes. Rebuilding can briefly hold old and new copied-key maps together.
Repeated large-to-small-to-empty replacements must release obsolete capacity
and keys. Tests-first compatibility controls pin this alongside duplicate-winner
dirty masks and append-only recovery. Unchanged generic native benchmark on the
recorded steady symbol corpus: 965 distinct symbols,2,500,000 lookups, checksum
1,236,723,300,248,721,791ns elapsed wall time (99.489ns/lookup). This is baseline
evidence, not the paired CI objective.

## Candidate evidence before full CI

The single shared Store source move is11 additions /20 removals. All29 Store
controls and1296 engine/KuCoin/Binance native tests across51 targets pass; strict
engine all-target/all-feature Clippy, scoped fmt, bans and request-context checks
(14 checker regressions) pass. The same native benchmark checksum is unchanged;
lookup wall time is99.489 to23.099ns. The index-only native allocation diagnostic
measures3860 to59603 retained bytes and7720 to119206 replacement-peak bytes for965
keys; large/small/empty controls release capacity completely. Raw memory source
and output are index-memory.rs/index-memory.log in the profile evidence directory.

A same-flags named candidate capture shows ticker_index14.164 to1.499ms inclusive
and WASM ingress47.294 to30.495ms. Final markets, tickers and books equal the prior
named capture; no protocol errors or unserved request change. These are diagnostic
sampling weights, not paired timing medians. The candidate includes actual9aef's
host capability additions while the initial profile source was258c, so overall
profile metrics are not an isolated Store-only CPU experiment. Both named runs
report2883584 high-water WASM bytes; process-heap noise prevents attributing their
retainedBytes difference to the index. Fresh CI owns the performance, memory and
size verdict, and the >=1.20x median target remains unproven until those results.


## Full CI receipt on the shared index source

Normal PR Rust run `39956093410` and normal dispatch `39956712962` both
completed successfully for source `83b548ac513`. The normal PR build used merge
`fccacfae38c448635127a6510575738c93e47f83`, whose parents are actual landing
`9aef6bca90b` and the source head; its whole tree equals the source head. All
replay, memory and size gates pass. The dispatch also passed browser cold start;
the normal PR browser job was skipped by its workflow policy.

The unchanged landing's nine-pair KuCoin Spot medians were TS 328.739 ms and
Rust 297.450 ms (1.10519x). The candidate's nine-pair medians were TS 198.921 ms
and Rust 156.951 ms (1.26741x), exceeding the 1.20x objective. Runners differ,
so these are within-run TS/Rust ratios, not an absolute cross-run speedup claim.
The native allocation tradeoff and per-account scaling are documented in CALLERS.
Aster's standalone Node WASM shrank from 1,335,801 to 1,320,171 bytes (-15,630).
The complete 28-family artifact comparison is recorded in the PR receipt.
