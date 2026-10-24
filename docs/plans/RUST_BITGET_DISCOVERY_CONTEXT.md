# Bitget caller adoption, slice 2: discovery under Background Context

Plan first, 2026-10-25; production follows the CLEAR plan and original REDs.
Owner: claude-4yjlw0. Scope approved by claude-ddi2il.
Branch `feat/rust-bitget-discovery-context` from landing `7fe49f6b964`, which
already contains slice 1 (#1972, `4ae5671e7da`).

**Status:** production implemented. The four original REDs pass. Ten existing
discovery fixtures (in `broker_attribution`, `public`, `startup_degraded` and
`startup_resilience`) drain through `test_support::prepared_actions` instead of
raw `drain_actions`, with their assertions unchanged. `Pending::Markets` and
`Pending::Tickers` carry a non-optional `Context`, so a missing-Context state
cannot be represented.

## Scope

The public discovery reads still go through the legacy `get`
(`venues/bitget/src/lib.rs:230-238`): `cx.request_streamed` with
`X-CHANNEL-API-CODE: Tealstreet` and `ArrayAt::Envelope(&[&["data"]])`. Its
only caller is `discovery_read` (`lib.rs:158-185`). That function serves two
reads:

- **Markets:** `/api/v2/mix/market/contracts?productType=…`. Codec at
  `lib.rs:714-750`; publication at `842-861` (`set_markets`, ticker index,
  public socket 0, `discovery.succeeded`, then the tickers read, then
  `start_private`).
- **Tickers:** `/api/v2/mix/market/tickers?productType=…`. `live_tickers.begin`
  runs at read start. Codec at `lib.rs:751-772`; publication at `862-881`
  (`live_tickers.finish`, `set_tickers`, ticker index, `succeeded`, sockets
  1 and 2).

`PublicRetries` (`self.discovery`) owns the retry schedule. `public_failed`
(`lib.rs:125-141`) reports with `failed_after(…, 0.0, error)`, and the timer at
`lib.rs:1028-1030` re-issues the read.

This was deferred as Q5 in `RUST_BITGET_CALLER_ADOPTION.md`. History (#1977),
the trading write job and `trade_timer` (slice 3, which needs Plan/write-group
or queue admission), and the lexical `matches!` rows are out of scope.

## Contract

The precedent is OKX slice 4 (#1996, `RUST_OKX_DISCOVERY.md`): one Background
managed Read per physical read.

- **Context.** `http_context(Scope::Background, Policy { Read, credential: None,
  timeout_ms: 20 000, pacing: None, retry: RetryPolicy::Never })`.
- **Request.** `request_managed_with_headers` with the same static
  `X-CHANNEL-API-CODE: Tealstreet` header and the same URL, then
  `stream_response(id, ArrayAt::Envelope(&[&["data"]]))`. The request is never
  signed.
- `Pending::Markets` and `Pending::Tickers` carry `context: Option<Context>`.
- **Delivery gate.** `validate_background_read_http_delivery(context)` runs in
  `on_item`, in `on_response` and in `on_stream_error`. It runs before the
  codec, before `skipped_row`, before the envelope and status checks, and
  before publication. It fails closed when a pending has no Context.
  - A stale or refused delivery only retires its Context. There is no
    `public_failed` call, no `live_tickers` change and no publication.
- **Publication order.** Publication completes first. Then `finish_http_context`
  retires the Context. Only then do the follow-ups run: the tickers read after
  markets, `start_private` and the socket connects. A follow-up read admits its
  own fresh Context.
- **Failures.** The failure paths are `on_request_failed`, `on_http_failed`,
  `on_sign_error` (unreachable for unsigned reads, but routed the same) and
  `on_stream_malformed`. Each retires the Context first, then calls
  `public_failed`.
  - These failure callbacks are not custody-gated. A read's own expired 20 s
    deadline must still reach `public_failed`, which schedules the native
    backoff.
  - `on_http_failed` passes the engine's typed error (Network or Timeout,
    outcome None) to `public_failed` unchanged. Natively, every transport
    failure was coerced to `Network`; a 20 s Timeout is new, because native had
    no deadline.
- **Refusals.** An admission or queue refusal is a prompt pre-send failure
  through `public_failed`, so the native backoff applies. There is no retry
  inside the read.
  - A queue refusal retires its already-admitted Context before
    `public_failed`.
  - An admission refusal has no Context to retire.
- **Unchanged:**
  - URLs, the product and testnet mapping, and the header.
  - Row codecs and skipped-row accounting.
  - Empty-catalogue and empty-tickers `VenueUnavailable`.
  - `discovery_error` and `public_response_error` texts.
  - The `invalid` flag routing.
  - The `live_tickers` begin/finish pairing.
  - Publication order and the `PublicRetries` schedule.

## Inventory

- Request map:
  - `venues/bitget/src/lib.rs` drops `a5c4555a` 1→0 (the `request_streamed` in
    `get`, `lib.rs:231`). Removing `get` removes exactly that fingerprint
    (computed with `check-request-context.py` `fingerprints()`).
  - The remaining `lib.rs` rows stay: `a921be85`, `5b441164` and `6c77d608` (the
    private `send` funnel and its signing step), and `c873db6f` (the History
    `on_sign_error` arm).
- `STAGING_BOOTSTRAPS['venues/bitget/src/lib.rs']`:
  - Its new map shrinks to exactly the live set `{5b441164, 6c77d608, a921be85,
    c873db6f}`. That drops `a5c4555a`, and also `43cf9107`, `569abc44` and
    `7280fdcd`. Slice 1 pruned those three but left them in the bound, which was
    a regrowth hole.
  - The approved test row in `check-request-context.test.py` changes from
    `(…, 4, 8)` to `(…, 4, 4)`. Its added-set is unchanged.
- Signing and credential-selection manifests: no change. Discovery is unsigned
  and selects no key.

## WASM budget

Bitget runs under the one-time override of 1 363 968 bytes, and #1972 measured
1 354 275. This slice may only ratchet down. It must stay within the override
on CI's venue and family builds, and the size is reported in the PR.

## Original REDs

New file: `sdk-rs/venues/bitget/tests/discovery_context.rs`. It drives the real
managed handshake through `test_support::prepared_actions` and uses monotonic
time.

1. The markets read is released through managed preparation with the exact URL
   and header. Today it is a legacy `request_streamed`.
2. The tickers read that follows markets is released through managed
   preparation. This is a separate RED.
3. An unanswered markets read ends at the shared 20 s deadline, and
   `PublicRetries` re-issues it after its native backoff. Today nothing ends it.
4. A markets response delivered after its read expired publishes nothing. There
   is no `set_markets` and no tickers read; only the retry from RED 3 stands.

The five original GREEN controls cover only what they actually execute. The
`Cycle` driver forces `public_data_only`.

- Exact URLs per product and testnet, the header, and unsigned requests.
- Contracts, then tickers, then the public sockets, in native order.
- Empty-catalogue and HTTP-error backoff with no publication.

These source-time gates are deferred and are not claimed by the original
evidence:

- private `start_private` after markets;
- skipped-row counts;
- full error fields and text (`discovery_error`, `public_response_error`);
- the `live_tickers` merge;
- missing-Context fail-closed;
- malformed, UTF-8 and claim-refusal routes;
- the exact 20 s deadline. RED 3 proves a retry by 50 s, not the exact
  bound. At cutover, pin the exact deadline timer, equality at 20 000 ms and no
  early expiry.

## Callers (tealstreet-next)

The TS-visible startup results are unchanged. One new behaviour: an unanswered
discovery read now fails at 20 s with a typed Timeout and retries on the native
schedule. Before, it hung with no deadline. The new 20 s bound runs from
admission to expiry and includes route, Prepare and Bind time. This goes into
`RUST_CUTOVER_CALLERS.md` at production time.
