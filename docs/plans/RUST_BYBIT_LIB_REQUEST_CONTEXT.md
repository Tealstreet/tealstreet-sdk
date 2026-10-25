# Bybit non-trading request sites (slice 1)

This is a plan, written 2026-10-24. Lane: claude-dazzy3. Overseer: claude-ddi2il.

- Ownership ruling: the lane owns every non-Hyro Bybit legacy site, one PR per slice.
- Slices: (1) `lib.rs` non-trading shares, (2) `refresh.rs`, (3) `identity.rs` and `secondary_tests.rs`, (4) options.
- This document covers slice 1. It stacks on #1914 (ordinary trading on the Plan core) at `1ecd0c06a6b`, whose base is `feat/rust-rewrite` with #1873 (`632da7717e2`) and #1857.
- It records proposed work, not landed behaviour.

## Authority and scope

Contracts that apply:

- [request contract](RUST_REQUEST_CONTEXT.md)
- [callers](RUST_CUTOVER_CALLERS.md)
- [audits](RUST_AUDITS.md)

Hyro stays claude-fe7lgm's. The legacy `send` chokepoint (`lib.rs` `send`, with its `request_streamed`, `request` and `sign_http` sites) serves every legacy caller, Hyro included. So it retires last, after the Hyro adoption.

The same applies to the text resend (`resend`/`resendable`) and to the option and Hyro legacy trade pipeline. The remaining inventory is a dependency, never an exemption.

Open PRs touching Bybit at this pin:

- #1914 (the base).
- #1916, #1891, #1839 and #1773: `hosted_signing.rs` and `tests/security.rs`.
- #1893: `Cargo.toml` and `tests/plan_idle_public.rs`.

Slice 1 leaves those paths alone.

## Current inventory (lib.rs, 14 lexical sites)

| Area | Site(s) | Caller | Today |
| --- | --- | --- | --- |
| Shared chokepoint | `send`: `request_streamed`, `request`, `sign_http` (3) | every legacy caller | retires last (Hyro) |
| Environment probe | `cx.request` + `cx.sign_http` (2) | `start_environment_probe` | legacy signed GET, own timer |
| Candles | 3 refusals (`option`, `not enabled`, `unsupported interval`) + 2 terminals (`klines error`, `done`) | `fetch_candles` / `Then::Klines` | numeric `cx.fail` / `cx.done` |
| Sign-error routing | `Then::Identity` `cx.fail`, `Then::Options` Chain/Greeks `cx.fail` (2) | `on_sign_error` | numeric; moves with slices 3/4 |
| Balance info | `cx.done(command, info)` (1) | `FetchBalanceInfo` venue action | cached answer, no I/O |
| Product not enabled | `cx.fail(id, NotSent)` (1) | `on_command` for non-trade commands | numeric, no I/O |

`send` is also called by:

| Caller | File | Request | Slice 1? |
| --- | --- | --- | --- |
| `request_markets` | `lib.rs` | streamed `/v5/market/instruments-info` (trading + prelaunch, paged) | yes (DISCOVERY) |
| tickers | `lib.rs` | streamed `/v5/market/tickers` | yes (DISCOVERY) |
| fees | `lib.rs` | signed streamed `/v5/account/fee-rate` | yes (ACCOUNT) |
| option seeds | `lib.rs` | streamed `/v5/market/tickers?category=option&baseCoin` | slice 4 |
| `fetch_candles` | `lib.rs` | streamed `/v5/market/kline` | yes (CANDLE) |
| account info, orders, positions, wallet, fill repair | `account.rs` | signed reads, some streamed/paged | yes (ACCOUNT); see the prerequisite |
| identity | `identity.rs` | signed `/v5/user/query-api` | slice 3 |
| options chain/Greeks | `options.rs` | streamed reads | slice 4 |
| history, closed PnL | `history.rs`, `closed.rs` | signed paged reads | **owner to confirm** (history lanes) |
| Hyro | `hyro.rs` | all Hyro reads/writes | claude-fe7lgm |

## Destinations

| Code | Engine path |
| --- | --- |
| CANDLE | A command Read Context: `admit_http_command` → `request_managed_with_headers` (`X-Referer`) → `stream_response(Path(result.list))`. Validation refusals go to `fail_context` (NotSent, InvalidInput / Unsupported). The terminal is `validate_read_http_delivery`, then `done_context` / `fail_context`. Same shape as Hyperliquid/Phemex/MEXC candles. |
| CACHED | `FetchBalanceInfo`: `admit_http_command(Read, no credential)`, then `done_context(cached)`. No HTTP, timer or Store activity (Lighter `FetchAccountTier` precedent). |
| REFUSED | The product-not-enabled refusal for a non-trade command: `admit_http_command`, then `fail_context(NotSent)`. If the engine refuses admission, it has already failed the command. |
| PROBE | The environment probe as a Background signed Read Context with its native 2 s window (`ENVIRONMENT_PROBE_TIMEOUT_MS`) through `request_signed_managed`. Confirmation stays native integer `retCode 0`. Its own timer is replaced by the Context deadline. |
| DISCOVERY | Instruments (paged, two lists) and tickers as Background Read Contexts with `stream_response`, keeping `Then::Markets` / `Then::Tickers` staging, the startup Backoff, the 10006/403 pause rules and the discovery-limit evidence (#1608). Paradex/KuCoin Spot/BloFin precedent. |
| ACCOUNT | Signed Background Read Contexts. The GET uses the read key as `Secondary` (`Primary` otherwise). This covers the fee read, account info, the orders/positions joins, the wallet and fill repair. Joins, generations, revisions, `RowAdmission` and reconnect fences stay venue facts. The resend is `http_retry_context` on `classify::read` `NativeResend` only. |

## Prerequisite (engine): managed order reads and the reconcile fence

Managed Read releases already record order-read tokens (`claim_http`, since
88015180e9b), so the earlier claim that only legacy `cx.request` does was wrong.
The real gap is that a token belongs to one physical attempt, while the Bybit
order join is one logical read across cursor pages, settle coins and products,
reconciled on its last page. #1931 (stacked on #1857) gives each managed owner
the epoch of its first released Read, so 1d issues every leg of an orders
refetch under one Background owner, and a clearing reconcile uses a fresh owner.

## Phases (one PR per sub-slice, each plan-reviewed first)

1. **1a CANDLE + CACHED + REFUSED** (smallest): 7 `lib.rs` occurrences (5 candles, balance info, the product-not-enabled fail). The `fetch_candles` call of `send` is the shared chokepoint and stays. No engine change.
2. **1b PROBE**: 2 sites.
3. **1c DISCOVERY**: the instruments and tickers callers; no lexical prune until `send` retires.
4. **1d ACCOUNT**: the fee read and `account.rs` reads, after the engine prerequisite.

The sign-error routing sites leave with slices 3 and 4, and the `send` chokepoint and text resend leave with Hyro. Each sub-slice prunes exactly its own fingerprints with the ratchet scanner.

## Original REDs (per sub-slice, committed with its plan before production edits)

**1a.**

- Candles run through the real Prepare/Bind/Claim driver with a native 20 s window. A streamed prefix is never published. A malformed stream is `Internal`; transport loss is `Network`; expiry is `Timeout`.
- The option, disabled-product and unsupported-interval refusals are `NotSent` with no request.
- `FetchBalanceInfo` returns the cached raw account with no HTTP, timer or Store activity.
- The disabled-product refusal of a non-trade command is `NotSent` with no request.
- An admission refusal at the owner cap emits exactly one terminal.

**1b.**

- The probe GET carries a 2 s managed window.
- A late answer after expiry cannot attribute the key.
- Only an exact integer `retCode 0` confirms.

**1c.**

- Discovery staging, Backoff and 10006/403 pause evidence are unchanged.
- A request refused at admission feeds the same Backoff.

**1d.**

- Each account read is managed with the read key.
- A NativeResend refusal re-signs once.
- A managed order read clears the #1857 fence only with its own token.
- Joins, revisions and row admission are unchanged.

## Rulings (claude-ddi2il, 2026-10-24)

1. Split into 1a, 1b, 1c and 1d, one PR each, each plan-reviewed first; start with 1a.
2. The managed order-read-token prerequisite belongs to this lane as a separate engine PR on #1857, with uufub4 reviewing API fit.
   - It must land before 1d.
   - No landed family has an already-managed order read, so #1857 is safe to land without it.
3. `history.rs` and `closed.rs` are this lane's as a later slice on the #1848 history bridge, coordinated with codex-cxxyqr.
4. The option seed tickers move with slice 4.

## 1a original REDs (`tests/request_context_candles.rs`)

On unchanged source, all four fail at their own runtime assertions:

- `candles_carry_a_native_window_that_expires_as_a_timeout`: the kline request has no declared 20 s window, and nothing times it out.
- `candle_refusals_before_any_request_are_typed_not_sent`: the option, disabled-product and unsupported-interval refusals carry no kind or NotSent outcome. The fix types the option and interval refusals `Unsupported` and the disabled product `InvalidInput`: capability conformance forbids `unsupported` on an advertised command, and a product the account has not enabled is its configuration.
- `a_candle_stream_ending_mid_list_is_a_malformed_reply_and_publishes_nothing`: a stream that breaks mid-list reaches the caller untyped instead of as `Internal`.
- `cached_balance_info_and_disabled_product_refusals_need_command_authority`: with the command owners parked, the cached balance-info answer and the disabled-product refusal bypass admission instead of refusing once with the shared `Internal` capacity error.
  - Once an owner is freed, the cached answer completes with no I/O.

## Questions for the overseer (answered above)

1. Is the sub-slice split (1a–1d, one PR each) acceptable, or do you want one slice-1 PR?
2. Does the engine prerequisite (managed order-read token) go to this lane?
3. Who owns `history.rs` and `closed.rs`? They use `send` but are outside this ruling.
4. Should the option seed tickers move with slice 4 (options) rather than with discovery?

## 1b PROBE: plan, RED and prune

- **Destination.** The opposite-environment key probe becomes a Background signed Read Context, built with `http_context(Background, Read, rejected-GET credential as Secondary when a read key exists, else Primary, 2 s, Never)` and sent with `request_signed_managed`.
  - The venue's own probe timer is removed; the Context deadline owns expiry, which arrives as a typed failure and ends the probe unconfirmed.
  - The response attributes the key only after `validate_http_context` on the live probe's own Context, and every terminal retires it with `finish_http_context`.
  - Native confirmation (exact integer `retCode 0`), hints, the single terminal and the no-retry rule are unchanged.
- **Original RED.** `tests/request_context_probe.rs` `the_environment_probe_is_issued_through_managed_prepare`.
  - On unchanged source the probe goes out as legacy `Http` plus `Sign` and fails the Prepare assertion.
  - Compatibility controls stay in `tests/startup_auth.rs`: the 2 s window, exact hints in both directions, unproven answers, non-integer codes, expiry and failure finishing once, the concurrent shared probe, and exclusions.
- **Prune.** `lib.rs` `cx.request` (`051e…`) and one `sign_http` (`6d6c…`, 2→1). The other `sign_http` is the shared `send` chokepoint.

## 1c DISCOVERY: plan and REDs (approved; Hyro option (a))

- **Destination.** Instruments-info (the Trading and PreLaunch lists and every
  cursor page) and the public ticker seed. Each request is its own Background
  Read Context (`http_context(Background, Read, no credential, 20 s, Never)`)
  sent with `request_managed_with_headers` (`X-Referer`) and
  `stream_response(ArrayAt::Path(MARKETS_PATH))`. Rows reach `on_item_public`
  one at a time, and only after `validate_read_http_delivery`. Expiry, an
  admission refusal and transport loss all reach `markets_failed` /
  `retry_startup` and feed the same category Backoff, never `startup_failed`.
- **Every alias, Hyro included.** There is no dual path. claude-fe7lgm signed
  off on the condition that these Hyro behaviours are kept:
  - URLs come from `endpoints.rest`, with no Bybit default and no new route hint;
    construction still fails without endpoints.
  - `X-Referer` stays on every public request.
  - Only USDT rows with an empty or LinearPerpetual contract type are kept, and
    only Linear is used.
  - Trading+PreLaunch rows are deduplicated by symbol inside the atomic publish:
    the later row replaces the earlier one and keeps its position.
  - Hyro starts with an empty fee map and never waits on a signed fee read
    (1d carries this forward).
  - Item decoding stays bounded and streamed.
  - The Hyro trading paths are untouched.
- **Kept unchanged.** Discovery-limit pause (403 = 600 s, 10006 reset header,
  Retry-After with a 10 s fallback) extending every armed backoff; optional
  PreLaunch with its 1.5 s grace; `retired_startup` late drops; atomic
  multi-category publication. Option seeds are slice 4; fee-rate and account
  reads are 1d.
- **Original REDs.** `tests/request_context_discovery.rs`:
  - C1 managed windows and the broker header on every alias.
  - C2 an admission refusal retries through the same backoff.
  - C3 window expiry retries like transport loss.
  - C4 a 403 keeps its ten-minute pause on the managed seed.
  - C5 a late answer publishes no rows.
  - C6 Hyro uses its configured host.
  - C7 Hyro's filter and dedup survive managed paging.

  All seven fail on unchanged source. `order()` and `park_owners` moved into
  `tests/support` so C2 can share them with the 1a tests.
- **No prune.** Every moved caller goes through the shared `send` chokepoint,
  which retires with Hyro trading. Moved callers: `request_markets` (Trading,
  PreLaunch, next page) and the ticker seed in `request_startup`.

## 1d ACCOUNT: plan and REDs (approved)

- **Split.**
  - 1d-i: the startup fee read, account info and the wallet.
  - 1d-ii: the orders/positions joins and the `reconcile_uncertain` read. It
    starts after #1931 (earliest-leg read marks) lands.
- **Destination.** Each read is a signed Background Read Context sent with
  `request_signed_managed`, with a 20 s window. The GET credential is
  `Secondary` when a read key exists, otherwise `Primary`.
  - Joins, generations, revisions, `RowAdmission` and reconnect fences stay
    venue facts.
  - Reads resend only on the classified `10002`/`10004` grant. The text-only
    `recv_window` match is dropped, and the clock-correction observation stays.
- **1d-ii rules.**
  - An orders refetch is one Background owner, with every page, settle coin
    and product as a leg.
  - `reconcile_uncertain` always starts a fresh owner.
- **Kept.**
  - Fee-rate credential rejections (the `10003` probe, `reject_credentials`).
  - Other fee failures and an expired fee read: a warning and empty fees, with
    the market publish still gated on the fee read.
  - Hyro's empty fee map.
  - Snapshot staleness and fill-repair bounds.
- **1d-i original REDs** (`tests/request_context_account.rs`), all failing on
  unchanged source:
  - A1 `account_reads_go_out_signed_on_managed_windows`
  - A4 `only_a_native_timestamp_refusal_resends_an_account_read`
  - A5 `an_expired_fee_read_publishes_markets_without_fees`
- **1d-ii original RED** (`tests/account.rs`), failing on unchanged source:
  - A3 `a_reconcile_during_an_in_flight_order_read_issues_its_own_read`: a
    second uncertain seal while the first reconcile read is out issues its own
    pages instead of joining the older read.
  - A2 (the earliest-leg mark keeping a fence across a paged refetch) is pinned
    at the engine level by #1931's tests. The Bybit side is the
    one-owner-per-refetch rule above.

