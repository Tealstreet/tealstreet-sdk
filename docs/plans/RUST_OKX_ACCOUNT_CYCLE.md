# OKX caller adoption, slice 3: account-cycle Background reads

Plan first, 2026-10-24. Owner: claude-4yjlw0, assigned by claude-ddi2il as one
exit of the OKX `STAGING_BOOTSTRAPS` row. Branch `feat/rust-okx-account-cycle`
from landing `508b24a080b`. This phase commits the plan and the original REDs
only. Implementation follows plan review, and nothing merges without a window.
The discovery slice (`load_markets` and `load_tickers`) is a separate draft.

## Scope

These are the private account-cycle reads that still go through the legacy
`list` → `cx.request_streamed` + `sign_request` → `cx.sign_http` path:

| Caller | Read |
|---|---|
| `account.rs` `reconcile` | `account-position-risk?instType=SWAP` (Balance) |
| `account.rs` `reconcile` | `positions?instType=SWAP` (Positions, product 0) |
| `account.rs` `orders_page` | `orders-pending` and `orders-algo-pending`, every product and kind, each cursor page |
| `account.rs` `account_response` | the next Positions product page |

Out of scope, each with its own exit:

- discovery (`load_markets`, `load_tickers`): the next slice;
- history and lookup: #1950, ih35js;
- closed positions (`closed.rs`);
- writes (`trading.rs` `next_write`): these need #1919;
- the startup config read and `fetch_leverage`: already managed since #1938.

## Contract

The precedent is the startup `Request::Config` read (#1938, `account_start`) and
Bitget slice 1 (#1972). Each physical account read becomes its own Background
managed Read:

- **Admission.** `cx.http_context(Scope::Background, read_policy(credential))`:
  Read, 20 000 ms, `RetryPolicy::Never`, no pacing. If admission is refused, the
  read goes through the native failure path (`account_failed`) with the
  refusal's message. Nothing is sent.
- **Request.** `request_signed_managed(context, Get, url, None, sign::Http
  recipe, Pending::Account(..))`, then `cx.stream_response(id,
  ArrayAt::EnvelopeField("data"))`. URLs, query order and the `x-simulated-trading`
  demo header are unchanged. `account_item` keeps streaming rows exactly as it
  does today.
- **Identity.** As today: a GET uses the read key when one is configured,
  otherwise the primary key. The `Credential` slot is taken from that same
  identity.
- **Custody.** `Request::Balance`, `Request::Positions` and `Request::Orders`
  each carry their `Context`.
  - **Response.** First `validate_background_read_http_delivery(&context)`
    and the existing generation checks (`account_request_current`, the join
    generation and `orders_generation`). Then the native reducer, then
    `finish_http_context(&context)`. The shared contract requires publishing
    before the Context retires.
    - Exception: a continuation page (the next Positions product, the next
      orders kind, product or cursor) finishes the old page's Context before
      admitting the next one. A mid-walk page publishes nothing until the walk
      completes.
  - **Streamed rows.** `account_item` checks the same delivery and generation
    gate before decoding.
  - **Failure, sign error or stale delivery.** Finish first, then the native
    failure reducer. A stale delivery has no other effect.
- **Failure.** There is no retry inside the read.
  - A 20 s Timeout, a transport failure or an HTTP/envelope failure takes
    `account_failed`. That clears the join or `orders_in_flight`, and
    `retried_failure` re-runs the cycle as it does today.
  - The visible change: an unanswered read no longer latches the cycle forever,
    because the shared deadline ends it.
  - A provider or queue refusal before sending is a pre-send failure on the same
    path.
- **Unchanged.** Row decoding, invalid-row handling, the snapshot join
  (positions with balance), the order cursor walk, product order and the
  rate-limit and backoff behaviour.

## Inventory

`self.list(...)` is a venue call, not a request site. So this slice prunes no
`request-legacy-sites.json` rows on its own. The `lib.rs` `list`
(`request_streamed`) and `sign_request` (`sign_http`, `2e921890`) sites stay
until discovery, closed positions, #1950 and writes have moved too. What this
slice removes is four of the remaining legacy caller groups. The OKX
STAGING_BOOTSTRAPS row cannot shrink in this slice.

## Original REDs

The new file is `sdk-rs/venues/okx/tests/account_cycle_context.rs`. It drives a
real `Engine<Okx>` through the startup handshake, records managed releases, and
asserts on observable actions only.

1. **Custody.** The Balance, Positions and orders-pending reads after login are
   released through managed preparation. Today they go through legacy
   `request_streamed` + `sign_http`.
2. **Deadline.** If the Balance read is never answered, the shared 20 s deadline
   ends it and the cycle reads `account-position-risk` again. Today nothing
   ends it and the join latches.
3. **Orders deadline.** The same check for an unanswered `orders-pending` page:
   the orders walk is read again, not left latched in flight.

Controls, which must pass both before and after:

- the exact native URLs, with the read key on GETs when configured;
- the snapshot commits balance and positions together;
- a late reply after a newer generation is inert.

## Original RED receipt

On unchanged production at `508b24a080b`, the file has 3 assertion REDs,
2 passing controls and nothing ignored:

- `account_cycle_reads_release_through_background_context`: the Balance read
  is released outside managed Context custody.
- `an_unanswered_balance_read_ends_at_the_shared_deadline_and_reads_again`:
  the join stays latched on the unanswered read.
- `an_unanswered_orders_page_ends_at_the_shared_deadline_and_reads_again`:
  the orders walk stays latched in flight.

The two passing controls are the exact URLs with the read key on GETs, and
balance and positions committing together.

## Implementation receipt

The source pin follows the API-fit review by codex-cxxyqr (CLEAR at `fef6facac7e`).

- **Admission.** `account_read` builds a transient `Slots` (primary, read key,
  `SlotPolicy::default()`) and calls `SelectedRead::select(ReadRole::Read {
  scope: None })`. It adds no auth evidence, because the native list path had
  none. Then `SelectedRead::background` (20 s, Never, no pacing) and
  `AdmittedRead::queue`, which carries the OKX dispatch recipe with the demo
  header and an `EnvelopeField("data")` stream.
  - A missing selection, admission refusal or queue refusal goes to
    `account_failed`. That retires only the matching join or order owner and
    leaves the native backoff to run.
- **Custody.** Each page Context lives on its `Request::{Balance, Positions,
  Orders}`.
  - `account_item` and `on_response` gate on
    `validate_background_read_http_delivery` before any decode.
  - A response runs `account_response` (publish), then retires the page.
  - A continuation retires the settled page before `account_read` admits the
    next one.
  - `account_failed` retires first; failures, sign errors and timeouts all reach
    it.
  - The generation, version and join rules are unchanged and separate from
    delivery custody.
- **Fixtures.** Account, product and trading tests now drain through the managed
  handshake (`prepared` at the event's time), and their assertions are
  unchanged. Two exceptions:
  - `complete_fresh` answers at 10 s, not 30 s, inside the new per-page deadline.
  - The #1943 subscribe-before-refresh test now matches the managed Prepare as
    the refresh's earliest staging.
- **Tests.** The 3 REDs pass and all 4 controls pass. That includes 2 new
  readiness controls: a managed orders cursor page, and a walk longer than 20 s
  completing page by page. All OKX tests and strict all-target Clippy pass.
  There is zero inventory delta.

