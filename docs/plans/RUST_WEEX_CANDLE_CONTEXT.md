# WEEX candle query Read custody

Status: draft production source; scoped controls pass, source reviews pending.
Base: actual `be710b3f2d7692244144b3f8f10810dc52790796`.

## Scope and train separation

Convert only command-owned `FetchCandles` in WEEX. The prospective source file
is `sdk-rs/venues/weex/src/lib.rs`; new tests live in
`sdk-rs/venues/weex/tests/candle_context.rs`. This checkpoint changes only this
standalone plan and the new test, with no shared README index edit.

The local `train/combo-5b` three-dot diff was checked against the base:
`git diff --name-only be710b3f2d7...train/combo-5b`. Its only BingX
or WEEX source path is `bingx/src/account.rs`; WEEX lib and the two new files are
disjoint. Recheck the auditor's actual frozen file list before implementation.
Other pending WEEX slices (#2003 and #2035) do touch lib.rs: they remain separate
dependencies to integrate by ordinary merges when their actual landings exist.
Neither supplies new engine authority needed by this plan.

Exactly three legacy completion occurrences in `weex/src/lib.rs` are in scope:

| Native location | Fingerprint | Count |
| --- | --- | --- |
| Generic failure, `Task::Candles` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 |
| Candle response success | `43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62` | 1 |
| Unsupported REST interval | `cd66da21afeeef22b98026c1d44238cf77b537c7b5ea67ee5d4fb4464905d893` | 1 |

The first fingerprint also belongs to unrelated native paths. Prune one
occurrence, not the entire hash. The shared `get` streamed and non-streamed HTTP
sites still serve other callers and are not counted as removed. No checker,
STAGING, engine, account, trading, signing or socket-policy edits are proposed.

## Native contract

Public unsigned GET, empty headers and no body. No market lookup or credentials
are required. Preserve the raw symbol and exact query ordering; do not add
encoding, range validation, row filtering, deduplication or truncation.

REST supports M1/M5/M15/M30/H1/H4/H12/D1/W1. H2/H6 are streaming-only and remain
unsupported for FetchCandles. Preserve the existing unsupported message.

If either bound is present, use `/capi/v3/market/historyKlines`, default limit
100, clamp 1..100, append supplied startTime/endTime and `priceType=LAST`.
Otherwise use `/capi/v3/market/klines`, default limit 500, clamp 1..1000. These
are single-response queries, with no pager, retry, signer or re-sign.

Keep Envelope/ArrayAt::Root streaming, variable-length CandleRow decoding,
timestamp conversion and numeric handling. Keep malformed-row rejection,
ascending timestamp sort and the exact serialized Candle output. Empty arrays
succeed. Querying candles does not subscribe or publish candle-cache/WS state.
Existing native HTTP and malformed-row error messages remain unchanged.
For every live native candle failure, retain Warn REQUEST_FAILED with message
`WEEX: {error}`, including the 429 failure before another query parks. No warning
is emitted by a stale/revoked callback that the engine has already discarded.

## Landed authority and ordering

Use only landed #1945/#1976 and the original Read completion bridge. In a genuine
entry, Expired uses `reject_expired_command(Read)`; missing receipt uses
`reject_unstamped_command(Read)`. Admit one `admit_caller_read_root(None, None)`
and bind `ReadCompletion::from_context`. Entry refusal precedes a native local
reply; supported/unsupported precedence is otherwise unchanged for valid entry.

The request owns that original completion and an optional physical Context.
Retain the root while parked behind the native shared pause, with no child or
Prepare. At eligible dequeue validate the original root, then mint a 20 s Never
unsigned child capped by it and queue managed streamed HTTP with empty headers.
Never renew the receipt deadline or admit Background/Plan work for this command.

Gate exact root, leg and delivered Attempt before row decode, invalid-row flags,
429 pause learning, envelope handling, sort/encode or completion. Success uses
the original Read completion once, then retires the child. Failure, preparation
or local child/queue refusal retires the child before strict root rejection.
Only proven already-revoked authority drops inertly. No partial rows/marker or
native numeric terminal remains. A queue refusal must take back its Pending.

The no-Cx `read_root_retired` hook removes only the matching Owner/generation's
deferred candle query. Its old venue timer cannot be cleared by that hook and
must later no-op. An old DATA/error/timer must not affect reused IDs or peers.

## Caller-visible changes for RUST_CUTOVER_CALLERS.md at production

Parked commands were previously unbounded inside the venue; they now end at the
original configured caller receipt deadline (default 120 s). The physical call
remains 20 s at most. A shorter receipt caps it; a raised receipt does not extend
that physical budget. Expiry at equality is strict Timeout with no rows, marker
or encoder invocation. A fractional live root remains live before equality.

Only exact live deliveries can teach the shared 429 pause. Expired/revoked late
429s are discarded by the engine and cannot preserve native late-pause learning.
Keep unrelated discovery/account seed-staleness handling unchanged.

Native string failure paths currently lack an explicit transport kind. Managed
released transport/physical expiry preserves Network/Timeout and Read outcome
None; pre-release typed failures retain their exact fields/NotSent where supplied.
No credential auth evidence, signer call or deactivation is introduced. Host
listener timeout does not grant engine cancellation or detach authority.

Streamed failures are distinct from transport. A complete 2xx envelope without
the expected root array reaches `on_stream_error`: validate exact live root,
leg and delivery before warning/rejecting with Internal, outcome None and the
native `response did not contain the expected list: {body}` message. A broken
stream reaches `on_stream_malformed` and then typed `on_http_failed`: retire its
physical Context and strictly reject with the supplied Internal/outcome None and
unchanged engine error message. Do not require a still-live physical deadline
for a typed failure callback; reject the exact retained root while it is live,
and let already-retired generations drop inertly. Root expiry never recovers.
The ordinary response gate still precedes malformed-row handling
(`malformed REST row`) and non-429 HTTP handling (`HTTP {status}: {body}`).
Retain each of those native messages and warnings. The shared ReadCompletion
projection supplies Internal when a native string failure has no kind; this
also affects native row/status and local unsupported-interval errors. Existing
typed causes and outcomes are preserved. Record this classification in CALLERS
and obtain native source review rather than claim the untyped shape unchanged.
No streamed failure can publish staged candles.

## Original assertion checkpoint

Use genuine handle_command receipts and a local real Prepare/Bind/Claim drain
that also observes legacy HTTP. Independent tests first prove native wire/output
positives, then assert the missing contract. No missing-method/compile-error REDs.

Expected REDs: live and historical managed release (two separate cases), short
receipt timeout on the physical request, DATA exactly at root expiry, expiry
while paused before dequeue, and a late expired 429 leaving a live peer unpaused.
Expected GREEN controls: native variable-length rows sorted/serialized unchanged,
empty live query, native unsupported interval, a fractional live response, and
effective native 20 s timeout under a raised receipt (None already defaults to
20 s on the base, so this is explicitly a preservation control).

After cutover separately certify streamed rows cannot stage after revocation,
held Bind/Claim refusal, physical Timeout, local capacity and queue refusal,
same-ID generations, unrelated peers and stale pause timer cleanup. Original
tests stop at their first assertion; do not claim unexecuted suffixes as REDs.
Run only the changed target with CARGO_BUILD_JOBS=4; broad checks belong in CI.

Production remains held for bounded native/bridge review and a new instruction.

## Executed original evidence

The plan was committed first at `22051e3953b`. On the unchanged native source,
`cargo test -p safe-cex-venue-weex --test candle_context` executes six assertion
REDs and six GREEN controls, with zero ignored tests or setup/compile failures.
The two release assertions first pass exact URL/method/empty-header/body checks,
then fail `(1, 0, 0)` versus `(0, 1, 1)`. The short receipt sees timeout None
instead of Some(1234). Equality and parked expiry see no typed Timeout. A late
expired 429 parks a live peer, yielding zero requests instead of one.

The six controls cover exact sorted variable-length row output, empty output,
fractional live delivery, unsupported REST interval, effective native 20 s
physical timeout, and successful eligible dequeue after the native pause.
Each variant is an independent test. The stale parked-timer and strict-timeout
suffixes after a first failed assertion remain post-cutover execution gates.
The drain expects successful Claim and does not certify held-claim refusal.
Only this plan and the new test change; source, existing tests, manifests,
checker/STAGING and combo-5b files remain base-exact. No native adoption or CI
claim follows from these original assertions.

The native review follow-up adds two independent streamed-ingress assertions
and two preservation controls. Missing-array and broken-stream routes first
pass their exact native message and warning assertions, then fail only on the
missing Internal kind: `(None, None)` versus `(Some(Internal), None)`. Malformed
row and non-429 HTTP message/warning controls pass. The pause helper additionally
pins one native Warn REQUEST_FAILED. Total: eight assertion REDs and eight
GREEN controls, zero ignored or setup failures; production remains base-exact.

## Local production source checkpoint

After native plan CLEAR at `e8e39b7b135`, the local source uses one original
ReadCompletion, an optional physical Context and the native staged candle rows.
Every callback validates delivery before row decode or response side effects;
typed failures retire the child and reject the original live root. The shared
ReadCompletion preserves typed fields and supplies Internal for native untyped
string failures, including the local unsupported-interval message.

The original 16 assertions are unchanged and now GREEN. Five added controls
cover delivered non-UTF8 bytes, staged rows discarded at root expiry with late
End/same-ID reuse, physical Timeout/reuse, held final Claim at expiry, and exact
typed preparation refusal without signing/auth/deactivation. The UTF8 control
certifies real non-UTF8 ingress, not a claim about one particular internal branch.
Native public candle fixtures use managed drain and live monotonic timestamps;
their exact financial output and WS interval assertions are retained.

Only WEEX lib, the two candle-related test files and this standalone plan change.
At source checkpoint `90cd9c79528`, shared inventory and CALLERS were held under
the initial train-file constraint. The overseer clarified that this applies only
to SOURCE files: the exact source-derived three-occurrence prune, CALLERS entry
and index row are now included for mechanical train composition. Source still
has zero overlap with combo-5b. This draft is not READY pending source review
and fresh CI; no landing clearance is claimed.
The original source compile import and a missing preparation-fixture argument
were corrected; neither is counted as assertion RED evidence.

Scoped execution: candle_context 21/21 and public 15/15 pass, zero ignored;
Clippy on those two changed targets with warnings denied and scoped rustfmt pass.
All builds use CARGO_BUILD_JOBS=4 and run serially. No local all-feature,
workspace, CPU benchmark or WASM build was run for this checkpoint.
