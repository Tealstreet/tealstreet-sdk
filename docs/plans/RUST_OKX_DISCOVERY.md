# OKX caller adoption, slice 4: discovery under Background Context

Plan first, 2026-10-24. Owner: claude-4yjlw0, assigned by claude-ddi2il as one
exit of the OKX `STAGING_BOOTSTRAPS` row. Branch `feat/rust-okx-discovery`
from landing `ba3d7da1d3a`. This phase holds the plan and the original REDs
only. It is separate from slice 3 (#1991, account cycle).

## Scope

The market catalogue and ticker discovery still go through the legacy `list`
path (`cx.request_streamed` plus `sign_request`):

- `load_markets`: `/api/v5/public/instruments?instType=…`, unsigned. For an EEA
  account with credentials it becomes `/api/v5/account/instruments?instType=…`,
  signed. It walks the product chain SWAP → FUTURES → ….
- `load_tickers`: `/api/v5/market/tickers?instType=…`, unsigned, with the same
  product chain and the `seeded` count.

## Contract

The precedent is Bitunix public reads (#1947): one Background managed Read
per physical read.

- `http_context(Scope::Background, Policy { Read, credential, 20 000 ms,
  pacing None, RetryPolicy::Never })`.
  - The credential is `None` for the public reads.
  - For the EEA private markets read it is the selected identity's slot and
    reference: the read key when configured, else the primary.
- Public reads use `request_managed_with_headers` with the native static
  headers, including the demo `x-simulated-trading` header. The EEA private
  read uses `request_signed_managed` with the OKX dispatch recipe. Both then call
  `stream_response(id, EnvelopeField("data"))`.
- `Pending::Markets` and `Pending::Tickers` carry their Context.
  - **Delivery gate (cxxyqr API fit).** `validate_background_read_http_delivery(context)`
    runs in both `on_item` and `on_response`. It runs before codecs, skipped-row
    or cache side effects, envelope handling, the reducer, continuation and
    publication.
  - Stale or refused authority only retires the Context: no failure, no
    `public_failed`, nothing done to a replacement read.
  - The native 429 discovery pause is kept separately.
  - The signed EEA read uses `SelectedRead::background` with `ReadRole::Read`
    and no auth scope. The legacy list path had no account-auth tag, and none
    is added.
  - A response runs the native body checks and the 429 discovery pause, then
    the product-chain reducer and publication, then retires the Context.
  - A next product in the chain retires the old Context before admitting the
    next read.
  - A failure or sign error retires first, then goes to `public_failed`.
- An admission or queue refusal is a prompt pre-send failure through
  `public_failed`, so the native startup backoff applies. A 20 s Timeout takes
  the same path. There is no retry inside the read.
- Unchanged:
  - URLs, the product order and the partial-catalogue rules (an earlier
    product's rows are never republished on a later failure);
  - row codecs, the `seeded` tickers count, the discovery pause and the startup
    backoff;
  - the TS-visible results.

## Inventory

`self.list` is a venue call, so there is no tracked-site delta. `list`'s
`request_streamed` and `sign_request`'s `sign_http` stay for the closed-positions
and #1950 history callers. After this slice and slice 3, the remaining exits of
the OKX row are #1950, closed positions and writes (#1919).

## Original REDs

The new file is `sdk-rs/venues/okx/tests/discovery_context.rs`.

1. The public instruments read is released through managed preparation.
   Today it goes through legacy `request_streamed`.
1b. The public tickers read, after instruments, is released through managed
   preparation. This is a separate RED, so the receipt doesn't overclaim the
   path.
2. An unanswered instruments read ends at the shared 20 s deadline, and startup
   discovery reads instruments again after its native backoff. Today nothing
   ends it.
3. For an EEA account with credentials, the signed `account/instruments` read is
   managed and signed with the selected key.

Controls, which pass both before and after:

- exact public URLs with no `OK-ACCESS-KEY`;
- the demo header is kept;
- the catalogue publishes after the full product chain.

## Original RED receipt

On unchanged production at `ba3d7da1d3a`: 4 assertion REDs, 2 passing
controls, nothing ignored. The tickers custody RED is now its own test.

- Public instruments are released outside managed custody.
- Public tickers, reached after an answered instruments read, are released
  outside managed custody.
- The unanswered instruments read stays latched; no read follows in 50 s.
- The EEA `account/instruments` read is signed with the primary key but is
  released outside managed custody.

The two passing controls are unsigned public URLs with no private read, and
the demo header being kept.

## Implementation receipt

This is stacked on #1991, whose managed-drain fixture conversion it reuses.

- **Admission.** `discovery_read` gives each physical instruments or tickers
  read one Background Read (20 s, Never).
  - Public reads use `http_context(Scope::Background, credential None)` and
    `request_managed_with_headers`, with the native static demo header (`[]`
    otherwise).
  - The EEA `account/instruments` read uses
    `SelectedRead::select(ReadRole::Read { scope: None })`, then `background`,
    then `queue`, with the OKX recipe and the demo header.
  - A refused admission or queue goes to `public_failed` and the native backoff.
- **Delivery gate.** `on_item` and `on_response` gate on
  `validate_background_read_http_delivery` before codecs, skipped-row and cache
  effects, the envelope, the reducer and publication. A stale delivery only
  retires. The 429 discovery pause is applied before the gate.
- **Retirement.** A response publishes, then retires. A chain continuation
  retires the old read before admitting the next. The empty-catalogue failure
  retires first.
- **Failures.** Transport, stream and sign failures, including a read's own
  20 s deadline, retire the Context first and then take the native
  `public_failed` path.
  - They are not stale-gated. A first cut that did gate them dropped the live
    read's own timeout, and the deadline RED caught it.
- **Fixtures.** The remaining raw drains (`public`, `startup`, `startup_degraded`,
  `history`, `lookup`, `closed_positions`, `dispatch_signer` and the lib.rs
  price-index unit tests) now release through the managed handshake at each
  event's time. `take_http` picks the first HTTP action past the deadline timers
  that admission arms.
- **Tests.** The 4 original REDs pass and both controls pass. All OKX tests,
  strict all-target Clippy and both ratchets pass. There is zero inventory
  delta.

