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
mapped utility/host boundary, not a claim of new venue or browser assembly.

The explicit host ABI still supplies function-source text, infallible callbacks
and warnings, and String-valued sleep/scheduling failures. These are visible
native capabilities; arbitrary JS callback/property behavior is not claimed.
The separate warning rendering row is outside this timing assignment.
