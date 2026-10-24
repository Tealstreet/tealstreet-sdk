# Kraken Futures public candle custody

Plan-first base: actual `10d01de5279`. Branch:
`feat/rust-kraken-futures-candle-context`. No combo-5b source or key-selection
changes are included. Kraken owner codex-hcmq3f released the candle regions;
open #1876 owns managed-write plan/tests and #1798 owns Spot book/CRC32 paths.
Neither intersects this slice. Preserve actual #2000 discovery/local custody and
#2014 fixed settings; do not compose their obsolete branches.

## Exact inventory and source boundary

Frozen checker inventory has Kraken27 occurrences/23 fingerprints. Futures
lib.rs has four occurrences/three fingerprints, all belonging to candle work:

| Native site | Count |
| --- | ---: |
| candles_page: cx.request_streamed | 1 |
| public_failed: numeric command failure | 1 |
| final candle response: numeric done/fail | 2 |

Intended shrink is family27->23 and lib.rs4->0. No manifest changes until actual
production adoption; then prune only this exact key. Remaining account/signing,
private history, trading and test inventory is outside scope. The raw streaming
funnel cannot be renamed or hidden behind another helper to claim a prune.

Source scope is Futures Pending::Candles and its request/page/stream/response/
failure ownership, plus root retirement if needed. Add an isolated native test
`tests/futures_request_context_candles.rs`. Keep Spot, account.rs, history.rs,
trading.rs, sign.rs, secondary tests, wire codecs, source key choice and shared
engine/registry/crypto code unchanged. Existing native assertions stay intact.

## Native geometry and behavior

Command FetchCandles and subscription polls share an unsigned GET chart route:
`/api/charts/v1/trade/{symbol}/{resolution}?from={floor(ms/1000)}&to={floor(ms/1000)}`.
Headers remain empty, body absent, Public egress, no credential or signer.
Unknown market, unsupported interval and invalid range keep their existing local
Read behavior and messages. Preserve read-only validation ordering; issue no
second root after those already-managed local terminals.

Preserve default limit500 and clamp1..5000 for deriving the default window;
explicit ranges are not locally truncated by limit. Resolution, row numbers,
ms-to-seconds conversion, response-envelope validity, sorting and duplicate
winner/order remain native. The next page starts at the last row timestamp in
ms plus the interval, strictly advances, stays <=to and obeys ten pages total.
A more_candles response with no viable cursor or exhausted budget fails the
whole command; staged pages cannot become a successful prefix. No new retry.

Background subscriptions keep their existing generation/subscriber/in_flight
checks and native five-second poll/backoff. Only a current generation may emit
a candle. A stale success OR failure cannot change a recreated subscription.
Retain the native latest-bar/change condition and no command terminal for polls.

## Existing shared authority, without 5b

A valid command walk admits exactly one original caller-wait Read root with
`admit_caller_read_root(None,None)` in genuine Fresh entry, after the existing
read-only local validation and before remote work or result mutation. It keeps
the exact raw caller receipt deadline, with no guessed root timeout. Rejected
already emitted one refusal; Revoked uses the existing expired-command helper
only while genuine entry remains Fresh. Missing receipt stays the shared typed
refusal, with no default timeout or numeric completion fallback.

At each eligible command page dequeue derive a fresh Read/Never physical child
from that ORIGINAL root via http_leg_context, timeout20000ms, no credential,
no pacing. Child deadlines cap at the original root; pages never derive from a
previous child or renew the root. Queue through request_managed and retain
stream_response(ArrayAt::Field("candles")); exact root+leg delivery is checked
before every streamed row, native envelope handling and result publication.
Only original-root done_context/fail_context terminates the command.

Each background physical page uses its own Background Read Context with the
same explicit20000ms/Never/None/no-pacing policy. This retains the native per-call
bound; it does NOT impose a new20s logical deadline across a poll's pages.
Validate exact background HTTP delivery plus subscription generation before
staging/publishing, and finish that physical Context once. Preserve next_poll
and in_flight cleanup on every queue/preparation/network/timeout/native failure.
No command root, Plan, Session or selected-key wrapper is fabricated for polls.

Retire completed physical contexts with finish_http_context, never a child
command terminal. Read DATA validation does not make a codec-valid candle.
Queue/preparation/provider errors remain typed and strict, never transport retry
evidence. Keep native Warn diagnostics and text alongside the typed Read failure;
no Read carries a financial Outcome. Command root expiry is engine-owned strict
Timeout: discard all staged pages, no candle/result/marker/continuation and no
local stop flag or root renewal. Pending command rows are owned only by the
retired root; use the existing retirement hook if any additional native parked
work is discovered. No shared authority/lifecycle edits are authorized here.

## Original controls before source

Compile and execute against unchanged10d production; count only assertion
failures whose setup and working controls succeed. A real thin Venue probe can
retain genuine Command Read peers for capacity and retire exact owners; it may
forward native callbacks and seed normal fixture data, never construct Context,
Attempt, permit or provider success. Managed lifecycle controls must traverse
real Prepare/Bind/Claim, and use the same harness on native raw HTTP for working
wire/codec controls. Missing future preparation stops that test at its first
assertion; do not claim suffix execution.

Cover command and background prepare-before-I/O, genuine capacity/refusal and
recovery, typed route preparation refusal/late success, original fractional
caller cap, live-root physical timeout versus strict original-root expiry,
second-page root retention and expiry with staged rows, and current-generation
publication only. Pin native single/multi-page wire/codec/defaults, ten-page and
no-progress failure, sort/dedup, five-second repoll, unknown/unsupported/range
precedence and unsubscribe/recreate behavior. Duplicate command IDs, unstamped
entry and invalid event clocks are separate shared-boundary controls.

Plan commit precedes the new target; original assertion checkpoint precedes any
production. Ask codex-hcmq3f for native fit and the root owner for bounded API fit;
production waits for those reviews and the original-control review. Overseer
approved REDs, review, production and parallel CI. Local cargo is only this
crate/target with CARGO_BUILD_JOBS=4, one build at a time; no workspace/all-feature
sweep. Full CI owns WASM, replay and workspace proof. Do not touch frozen2033.
