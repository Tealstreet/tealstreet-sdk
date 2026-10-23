# Bitget history RequestContext adoption

Plan first, 2026-10-24. Owner: claude-4yjlw0. Branch
`feat/rust-bitget-history-context`, stacked on Bitget caller slice 1
(`feat/rust-bitget-caller-entry` `3459852f78f`, which carries #1915
`86f528aaef8`), with a plain merge of frozen #1945 `15df705c0b5`. This
phase commits the plan and original assertion REDs only. Implementation
follows plan review; nothing merges without an overseer window, and the
slice lands after #1915, caller slice 1 and #1945.

## Ruling

History ruling (c), all venues: one immutable Read root per command, whose
timeout is the caller's effective wait (`rustCommandTimeoutMs` or the per-call
override), with a 20 s page child minted when a page is dequeued. Bitget
fails strictly, so neither #1928 Plan custody nor an accepted-prefix
terminal is needed (see the history section of
`RUST_BITGET_CALLER_ADOPTION.md`).

## Inventory

At this base, Bitget has 24 fingerprints / 26 occurrences. This slice removes:

- **history.rs, all 11 / 12:**
  - the five `start_query` entry refusals (`23ef96f6`, `0c9d5d0f`, `32a96a9d` ×2, `461218ae`);
  - the unreachable `query_request` client-ID arm (`b9abde2a`, deleted);
  - the cursor-stall and page-cap fails (`5ecc36ad`, `5659fbaf`);
  - the final done and the encoder failure (`755a658b`, `d5e898e0`);
  - the retries-exhausted fail (`ea05be25`);
  - the detection-deadline fail (`7ab67768`).
- **lib.rs, 2 / 2:**
  - the funnel's streamed request (`a921be85`; history is its last caller);
  - the History `on_sign_error` terminal (`c873db6f`).

That projects Bitget to 13 fingerprints / 14 occurrences. The #1915
`STAGING_BOOTSTRAPS` Bitget lib entry shrinks to `5b441164` and `6c77d608`
(the write slice's funnel `request` and `sign_http`). The checker test's
approved row changes with it. `send` keeps its `cx.request(method, url,
vec![], body, pending)` text byte-exact, so `5b441164` does not rename. The
`Payload::Stream` variant and its arm go.

## Root and page contract

**Entry (`start_query`, the genuine command entry).**

1. `cx.command_wait()`:
   - `Expired`: `cx.reject_expired_command(Operation::Read)` and return. No
     admission, Context, timer or request.
   - `Remaining(ms)`: the root's `timeout_ms = ms`.
   - `None`: `DEFAULT_COMMAND_WAIT_MS`, per the #1945 owner's clarification
     that `None` falls through normal admission with the shared default. This
     path is reachable only from an unstamped receipt.
2. `cx.admit_http_command(read_policy_with(timeout))`. The credential is
   `read_credential(path)` for private queries (Secondary when configured,
   else Primary) and `None` for PublicTrades. A `Rejected` admission already
   emitted its terminal; return.
3. The five native refusals keep their order and text, and end through
   `fail_context(&root, ...)`. The missing-credentials check stays first among
   them.
4. The `Request` carries the root `Context` in place of its `CommandId`.

**Pages.** Each physical send mints a child at its own dequeue. That is either
`query_request` called from `start_query` (detection already complete) or the
history timer's dequeue:

- `cx.http_leg_context(&root, read_policy(credential))`: 20 000 ms,
  `RetryPolicy::Never`, Read. Its deadline is capped by the root, and the
  child budget starts at dequeue, so native pacing never consumes a page's
  20 s.
- `request_signed_managed(leg, Get, url, None, sign::Http step, pending)`
  for private queries, or `request_managed` for PublicTrades. Then
  `cx.stream_response(id, at)` keeps `query_item` streaming rows into the
  page buffer exactly as today.
- The root is never renewed. Children always derive from the root, never from
  a prior leg.
- If the leg refuses because the root expired, or for any other reason,
  `fail_context(&root, error)` runs with that typed error. Staged rows are
  discarded.

**Delivery.** `on_response` for a history leg first calls
`validate_read_http_delivery(&root, &leg)`. On error, the page is dropped and
the query fails through the root. Then the native body checks run unchanged:

- 40085 promotion re-detects and re-queues; a fresh leg is minted at the next
  dequeue.
- Envelope/HTTP status failure goes to `query_failed`.
- The cursor is followed, with `seen` dedup and `MAX_PAGES` 1000.

**Native geometry is kept exactly:**

- 30-day oldest-first windows, at most 1000;
- 100 ms spacing between pages and windows;
- the 10 s account-detection wait with its 100 ms poll and native failure text;
- three timer retries at 250/500/750 ms, each a new leg at dequeue;
- strict failure texts: `Bitget history incomplete: {error}`, cursor stall
  and page cap.

`http_retry_context` is not used. A native retry is a fresh physical page
after a timer, not a same-leg resend.

**Terminals.** These all go through the root:

- `done_context(&root, rows)`;
- `fail_context(&root, ...)` for the cursor stall, the page cap, the encoder
  error, exhausted retries and the detection deadline.

`on_sign_error` for History becomes `fail_context(&root, error)`, preserving
every typed field (strict NotSent, no retry), as #1942 does for OKX.
`on_http_failed` keeps native retry for transport and child timeouts. A child
Timeout is a page failure. It retries only while the root is live: the next
dequeue's `http_leg_context` refuses on an expired root and ends the query
with that typed error.

**Root expiry while queued.** There is no automatic root timer. A query
waiting on its 100 ms or retry timer observes the expiry at that wake-up,
through `http_leg_context` refusal or `validate_http_context`, and fails
then. In-flight children are already root-capped. Exact-at-root cleanup of
queued work would need a shared seam, and is not claimed.

## Native invariants kept

- Credential absence precedes range validation; PublicTrades needs no
  credential.
- Single-bound ranges keep their 30-day completion and `to = now` default.
- UTA versus classic endpoints, `category`/`productType`, `cursor`/`idLessThan`,
  the limit clamp 1..=100 and the symbol omission for UTA fills.
- Row mapping (`query_item`, `query_uta_bill`), identity dedup and the
  newest-first sort.
- The detection wait uses the account-detection state machine; it never sends
  before detection, except for PublicTrades.

## Original REDs (after plan review)

New file `sdk-rs/venues/bitget/tests/history_context.rs`: real
`Engine<BitgetFutures>` through `handle_command`, observable actions only, no
ignored tests. Assertion REDs on unchanged production:

1. A private Fills query releases a managed signed GET bound to a command
   Read Context (today: a legacy `request_streamed` plus `sign_http`).
2. An explicit `timeoutMs: 1500` caller wait. The account is undetected for
   2 s, the query fails with a typed Timeout at the first dequeue after 1.5 s,
   and sends nothing (today: the native 10 s detection text).
3. With the default 120 s and 30 windows that each answer in 5 s, the root
   expires mid-walk: a typed Timeout and no later page (today: completes).
4. A page child born at dequeue. A page dequeued at t+0.9 s whose response
   arrives at t+20.5 s is still eligible; one arriving at dequeue+20 s is not
   (today: no child; transport timer only).
5. An `Expired` Fresh entry (receipt held past its wait) gives one typed
   Timeout terminal, outcome None, zero HTTP/timer (today: a native query
   starts).
6. A sign failure for a history page ends with the typed error fields
   preserved (today: `cx.fail` drops kind/outcome).

Compatibility controls that must pass both before and after:

- the five refusals' texts and order;
- strict cursor-stall and page-cap text;
- three retries, then `Bitget history incomplete: ...`;
- 40085 promotion re-issue;
- PublicTrades unsigned, with no credential;
- window walking oldest-first;
- the dedup and sort result;
- a longer explicit wait (300 s) that completes a 130 s walk, ruling out a
  hidden fixed 120 s root.

## Gates

- Before implementation: plan review by codex-uufub4 (API fit for root plus
  timer-issued legs and stream registration) and claude-ddi2il.
- After implementation:
  - strict all-target Bitget Clippy and fmt;
  - both ratchets with the exact prune above;
  - `check-request-context.test.py` with the updated approved row;
  - Bitget/WASM tests;
  - two independent reviews;
  - full CI.
- The 9czdjh selected-read helper is not used here. It adopts in a follow-up
  once it has a per-page leg method (requested in that review); this slice
  keeps `read_credential`/`signing_identity` selection, captured once at
  entry for the whole query.
