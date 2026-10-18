# Exchange core FIX-audit

## Loop scheduling

Assignment: `utils/loop.ts::loop`, mapped to `loop_util::loop_once` and
`LoopHost`. Baseline `bc5017a625`; compiled RED checkpoint `b1dd79a25c`.

The actual exported Rust helper with NativeRuntime produced no sleep at call
time, while the maintained TS function called sleep immediately. Positive
infinity also slept and scheduled on the native baseline; current TS returns
without those effects. Both failures are assertions in `tests/remaining_utils.rs`.
`tests/loop_timing_source.cjs` executes the maintained function with only terminal
sleep/browser scheduling replaced, including invocation and completion traces.

The native cutover requires the owning runtime in LoopHost and uses canonical
`start_eager`. A source sleep Promise preserves the await fence for ready leaves.
Tests cover waiter-drop continuation, callback queue identity/count, hidden and
visible documents, fallback, rejection and positive infinity. Existing warning
key deduplication and closed-position assertions remain intact. This is the
mapped utility/host boundary. Its existing Breakout production adapter now
passes the owning runtime, invokes the helper before queuing its error observer
and reports the original ExchangeError. The real binding test advances the
virtual clock halfway before any task poll, then requires the callback at the
original deadline; merely repairing the ABI compiled but failed this assertion.
This does not add browser assembly or arbitrary callback semantics.

The explicit host ABI still supplies function-source text, infallible callbacks
and warnings, and String-valued sleep/scheduling failures. These are visible
native capabilities; arbitrary JS callback/property behavior is not claimed.
The separate warning rendering row is outside this timing assignment.

## Journal read ownership and settlement callback

Assignment rows 1 (`buildFoldedPositionRecords`) and 14 (`BuildRecordsOptions`)
from codex-l89ogr's zero-based handoff. The maintained TS function calls its first
income/fills reader synchronously, awaits the response, then completes the fold
independently of whether its caller observes the returned Promise. The original
borrowed Rust async function compiled RED on immediate entry. Independently,
polling it to a held read and dropping it compiled RED on the missing subsequent
settlement callback. The direct owned runtime/callback cutover uses shared
`eager::start_eager`; each child read keeps the source await fence even when ready.
There is no provisional spawn or JSON result/error projection.

Row 1 verdict: FIX for immediate entry and discarded-waiter continuation.
`tests/journal.rs::journal_read_starts_at_call_and_finishes_after_waiter_drop`
compares both first-read branches with `scripts/journal-oracle.cjs`, which executes
the whole maintained TS function and its actual interval/fold dependencies.
The preexisting 12 build scenarios compare call ordering, options, results and
wrapped income failures. Arbitrary malformed income/Execution objects and the
external JournalFolder installation remain separate unclosed input/composition
scope; this scheduling repair does not authorize them.

Row 14 verdict: ACCEPT for the claimed infallible-callback gap on current master:
it was already repaired when Store lookup became fallible. The actual options
callback returns Result, and `map_folded_interval` propagates it immediately.
`journal_ready_reads_defer_fold_and_preserve_settlement_rejection` compares the
maintained throwing callback and asserts exact name/message/original referent,
one callback, no later record and the ready-read await fence. Ownership of the
immutable account string is the accepted owned-snapshot boundary (decision 2),
not a lost option or diagnostic. The cutover preserves all five options fields.
