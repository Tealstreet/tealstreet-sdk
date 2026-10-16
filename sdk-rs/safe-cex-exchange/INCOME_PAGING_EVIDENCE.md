# Shared income paging evidence

The existing feeder is `packages/safe-cex/src/__tests__/audit-regressions/income-paging.test.ts`:
438 lines, 17,508 bytes, SHA-256
`676b632262daa2632a202dc89dd91d332cef8285160fea9328fce9f35833d248`.
It remains byte-identical. Its 18 actual-source tests execute unchanged under
real Vitest. The labeled supplement captures inputs before execution, every
request, response/failure and final result. It adds three tests, producing 33
pager runs and 130 identity/direction helper cases in one Node process.
Supplements are new evidence and claim no retroactive independence.

Each capture records architecture and Node version (initial arm64 / Node 26.7.0).
NaN alone has a canonical capture payload; every other number retains its bits.
Rust reads eight native IncomeRecord fields, all three request fields and every
result field. Numeric options include distinct fractional values, signed zeros,
infinities and NaN. A source alias witness observes a producer mutation through
the consumer; a double-escaped surrogate witness retains source acceptance and
output alongside Rust UTF-8 refusal. Null identity is captured as `|||`, outside
the typed Rust input domain.

No source corpus or generated dump is committed by this change. The supplement
runs in the native core include-ignored gate. Both native dependency installs
select Vitest and its Vite peer from the owning package and copy the root frozen
lockfile. An isolated tracked-source mirror with no node_modules ancestor ran
all 21 tests using only the selected isolated runtime. Initial isolation failed
with ERR_MODULE_NOT_FOUND for the absent Vite peer; adding the declared selector
repaired it without falling back to the root installation.

The callback, options, result, identify, pageDirection and paginateIncomeRange
rows map to the existing six symbols, including the two private source helpers.
IncomePageParams names the callback's inline parameter and PageDirection names
its existing literal return union; neither creates another extracted row.

The host timer is explicit rather than cast to u64. A native test pins original
negative fractional delay, failure identity before the next fetch, and owned
fetch cancellation. Source Promise abandonment and native host timer coercion
are not claimed. Typed options and records exclude getters, dynamic coercion,
non-array callback results and live object aliases. Source nonprogress arithmetic
is retained; no infinite-loop workload is executed on this machine.

## Cardinality and control evidence

The source inventory is asserted: 33 pager runs, 130 helper inputs, 1,193 result
records and 80 requests, with fixed per-case record and request counts. Native
loops use the fixed 33/130 expectations; independent native lengths precede each
zip, and performed native comparisons are asserted as 1,193 records/80 requests.
Empty and subset source corpus controls fail `PAGER corpus admission count`;
subset output fails `PAGER source per-case output counts`; empty/subset attempts
fail `PAGER source per-case request counts`. Each scoped Rust target exits 101,
then restores to 2/2. The reviewer independently cleared and truncated native
output; both compiled and failed `PAGER native record count` at 101.

Eight production mutations compiled and failed at 101:

| Mutation | Named assertion |
| --- | --- |
| tran-ID-only identity | PAGER native record count, shared ledger-row test |
| FIFO pending windows | PAGER request startTime, saturated instant test |
| subwindow direction bounds | PAGER consumed all source requests |
| chunk size substituted for limit | PAGER request limit |
| request/exhausted counters swapped | PAGER native requests |
| discarded delay failure | PAGER no fetch after rejected chunk delay |
| fallback timestamp/income swapped | PAGER identity helper 112 |
| duplicated successful delay | PAGER successful delay/request trace |

The global-bounds mutant survived an initial supplement: both filtered pages
classified unknown. Its distinguishing supplement now uses global [8,2,3]
versus subwindow [2,3]. The reviewer found two further actual survivors: fallback
identity timestamp/income were equal, and successful delays were not counted.
Five unequal helper inputs, five scalar-Unicode IDs (including astral, FEFF and NEL), and a captured successful timer/request trace repair
those gaps. This is coverage now, never a claim that the first corpus was immune.
Initial identity/FIFO mutation drafts failed compilation and are not evidence;
the corrected compiling controls listed above are the measured proofs.

A real missing-Vitest import fails MODULE_NOT_FOUND at Rust exit 101; an actual
source interface heritage injection fails `PAGER unexpected heritage
IncomeRangeOptions` at 101. Both restore to 2/2. The source AST census checks
all three exported type shapes, member optionality and callback result type,
and refuses heritage rather than counting only directly declared members.

## Reviewed capture and gate boundary

The reviewer independently reproduced loss of the worker stdout capture despite
Node exit 0. The supplement now writes a temporary file synchronously; the parent
reads it after Vitest closes and awaits its own stdout write with backpressure.
The file is removed in finally. A missing-worker-write control exits 101 naming
`PAGER worker capture must exist`; no missing capture can be treated as success.
Direct synchronous pipe-write candidates truncated or raised EAGAIN and were
rejected; the retained implementation awaits the parent stream callback.

Independent extra-native output, and empty/subset native output with the source
output-count guards removed, compile and fail `PAGER native record count`.
An initial bypass-control draft failed an unused-variable lint and is excluded;
the corrected controls reach the named assertion. Restored target: 2/2 tests,
zero ignored, 33 pager runs and 130 helpers. These are output-loss controls,
not consuming-serde-decoder controls; the pager has no production serde decoder.
The broader Bitget collection audit remains separate and incomplete.

Scoped strict host and wasm clippy each exit 0. The real Yarn entrypoint reports
parity 7,339 rows / 3,369 public Rust items and layers 78/78. Real tomllib parses
359 exemption blocks / 359 distinct targets. Extractor exits 1 only for the two
added chaser symbols: two manifests, two added, zero orphaned. The inherited
core CI audit also exits 1 at `P4a pushes must trigger the workflow`; its workflow
and audit files are unchanged from integration ba6468e0076f. These failures are
reported rather than suppressed; this slice is not a full merge-readiness sheet.
