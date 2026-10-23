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

Only the legacy `cx.request` records an order-lock read token (`engine.rs` `order_read_tokens.insert`). A managed read removes a token on delivery but never inserts one, so `order_response_token` is `None` during a managed response.

As a result, a managed `/v5/order/realtime` join could never call `orders_reconciled` (the #1857 fence), and every Unknown ordinary seal would leave a fence that never clears. Before ACCOUNT moves the order reads, the engine needs managed Read Contexts to capture and deliver the order-read token exactly as `cx.request` does.

That is a small engine change with its own tests. Ownership: proposed to this lane as a separate engine PR stacked on #1857, like #1920.

## Phases (one PR per sub-slice, each plan-reviewed first)

1. **1a CANDLE + CACHED + REFUSED** (smallest): 8 `lib.rs` sites (5 candles, balance info, the product-not-enabled fail, and the `fetch_candles` `send` call). No engine change.
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

## Questions for the overseer

1. Is the sub-slice split (1a–1d, one PR each) acceptable, or do you want one slice-1 PR?
2. Does the engine prerequisite (managed order-read token) go to this lane?
3. Who owns `history.rs` and `closed.rs`? They use `send` but are outside this ruling.
4. Should the option seed tickers move with slice 4 (options) rather than with discovery?
