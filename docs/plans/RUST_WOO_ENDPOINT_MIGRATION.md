# WOO endpoint migration proposal

Status: proposal only; held for the next SDK cut. No runtime activation, live
requests, credential access, release or merge is authorized by this document.
This follows #2981 (HTTP attribution) and #2982 (replay observation timing).

Audited SDK base: `3445164ef1134b909441b508f623215e74255ed7`, 2026-11-04.
All source references below are relative to `sdk-rs/venues/woo/src/` at that base.

## Lifecycle evidence and immediate-bug triage

WOO's [retirement announcement](https://support.woox.io/hc/en-us/articles/48320882568473-WOO-X-V3-API-Is-Live-Migrate-Before-Dec-31-2025-July-30-2025)
scheduled V1, V2 and the previous V3 for retirement on 2025-12-31. The date has
passed. The [current migration map](https://developer.woox.io/api-reference/migration_guide)
continues to identify their namespaced V3 replacements. This establishes a
vendor-declared migration obligation, not observed authenticated shutdown of
every individual route.

**Confirmed authenticated shutdowns in this audit: none established.** No live
venue request was made. #2981's recorded 2026-11-04 probes found 401/-1002 on old
and new account/position routes without credentials; its nonexistent-route
control differed. Neither result establishes that a valid key can use an old
route. A reported authenticated failure needs a redacted status/native-code,
route, timestamp and known-working authenticated control. If it identifies a
retired route, prioritize that live bug ahead of the ordinary migration phases.
Do not ask a user to regenerate a key because a route is retired; preserve #2981's
typed HTTP/native-code distinction and body/transport redaction.

The primary REST origins already use `api.woox.io` and
`api.staging.woox.io` (`lib.rs:190-197`), not the retired `api.woo.org` domain.
Historical candles still use a legacy path on `api-pub.woox.io`. Moving only the
hostname, or treating every path containing `/v3/` as migrated, is insufficient.

## Production REST inventory

The table counts distinct method/path operations, not repeated call sites:
26 operations, of which 25 use legacy contracts and one already uses current V3.
`GET /v1/orders` serves both open-order census and historical-order queries.
All rows except R12 are covered by the legacy migration obligation above.
Target links point to the relevant current endpoint contracts; every row also
has a mapping in WOO's migration guide except the `/v1/kline` spelling, discussed
below. This is the native inventory, not the older TS selector's owner list.

| ID | Used method/path | Target method/path | Native consumer / source |
| --- | --- | --- | --- |
| R01 | GET `/v1/public/info` | GET [`/v3/public/instruments`](https://developer.woox.io/api-reference/endpoint/public_data/instruments) | Catalogue; `seed_read.rs:85` |
| R02 | GET `/v1/public/futures` | GET [`/v3/public/futures`](https://developer.woox.io/api-reference/endpoint/public_data/futures) | Ticker seed; `seed_read.rs:93` |
| R03 | GET `/v1/public/funding_rates` | GET [`/v3/public/fundingRate`](https://developer.woox.io/api-reference/endpoint/public_data/fundingRate) | Funding poll; `seed_read.rs:101` |
| R04 | GET `/v1/public/orderbook/:symbol` | GET [`/v3/public/orderbook`](https://developer.woox.io/api-reference/endpoint/public_data/orderbook) | Book seed and price preflight; `lib.rs:316`, `trading.rs:1026` |
| R05 | GET `/v1/kline` | GET [`/v3/public/kline`](https://developer.woox.io/api-reference/endpoint/public_data/kline) | Recent candles; `lib.rs:510` |
| R06 | GET `https://api-pub.woox.io/v1/hist/kline` | GET [`/v3/public/klineHistory`](https://developer.woox.io/api-reference/endpoint/public_data/klineHistory) | Historical candles; `lib.rs:519` |
| R07 | GET `/v3/accountinfo` | GET [`/v3/account/info`](https://developer.woox.io/api-reference/endpoint/account/get_account_info) | Startup/valuation, identity and mode readback; `account.rs:582`, `identity.rs:97`, `trading.rs:1119` |
| R08 | GET `/v3/positions` | GET [`/v3/futures/positions`](https://developer.woox.io/api-reference/endpoint/futures/get_positions) | Position census; `account.rs:605` |
| R09 | GET `/v1/orders` | GET [`/v3/trade/orders`](https://developer.woox.io/api-reference/endpoint/trading/get_orders) | Regular census/history; `account.rs:627`, `history.rs:689` |
| R10 | GET `/v3/algo/orders` | GET [`/v3/trade/algoOrders`](https://developer.woox.io/api-reference/endpoint/trading/get_algo_orders) | Algo census; `account.rs:625` |
| R11 | GET `/v1/client/futures_leverage` | GET [`/v3/futures/leverage`](https://developer.woox.io/api-reference/endpoint/futures/get_leverage) | Leverage query/readback; `account.rs:720`, `trading.rs:1098` |
| R12 | GET `/v3/trade/transactionHistory` | GET [`/v3/trade/transactionHistory`](https://developer.woox.io/api-reference/endpoint/trading/get_transactions) (unchanged) | Preferred fills history, already migrated; `history.rs:669` |
| R13 | GET `/v1/client/trades` | GET [`/v3/trade/transactionHistory`](https://developer.woox.io/api-reference/endpoint/trading/get_transactions) | Legacy fills fallback; `history.rs:679` |
| R14 | GET `/v1/client/transaction_history` | GET [`/v3/asset/token/history`](https://developer.woox.io/api-reference/endpoint/assets/token_history) | Income ledger; `history.rs:700` |
| R15 | GET `/v1/public/market_trades` | GET [`/v3/public/marketTrades`](https://developer.woox.io/api-reference/endpoint/public_data/marketTrades) | Recent public trades; `history.rs:706` |
| R16 | POST `/v1/order` | POST [`/v3/trade/order`](https://developer.woox.io/api-reference/endpoint/trading/post_order) | Ordinary placement; `trading.rs:482` |
| R17 | DELETE `/v1/order` | DELETE [`/v3/trade/order`](https://developer.woox.io/api-reference/endpoint/trading/cancel_order) | Ordinary cancellation; `trading.rs:793` |
| R18 | PUT `/v3/order/:orderId` | PUT [`/v3/trade/order`](https://developer.woox.io/api-reference/endpoint/trading/edit_order) | Ordinary amend; `trading.rs:736` |
| R19 | POST `/v3/algo/order` | POST [`/v3/trade/algoOrder`](https://developer.woox.io/api-reference/endpoint/trading/post_algo_order) | STOP/TP/SL/trailing/brackets; `trading.rs:414,540,570` |
| R20 | PUT `/v3/algo/order/:orderId` | PUT [`/v3/trade/algoOrder`](https://developer.woox.io/api-reference/endpoint/trading/edit_algo_order) | Algo/child amend; `trading.rs:751` |
| R21 | DELETE `/v3/algo/order/:orderId` | DELETE [`/v3/trade/algoOrder`](https://developer.woox.io/api-reference/endpoint/trading/cancel_algo_order) | Algo cancellation and replacement preconditions; `trading.rs:618,688,780` |
| R22 | DELETE `/v3/orders/pending` | DELETE [`/v3/trade/orders`](https://developer.woox.io/api-reference/endpoint/trading/cancel_all_order) | Account-wide ordinary cancel leg; `trading.rs:837` |
| R23 | DELETE `/v3/algo/orders/pending` | DELETE [`/v3/trade/algoOrders`](https://developer.woox.io/api-reference/endpoint/trading/cancel_algo_orders) | Account-wide algo cancel leg; `trading.rs:844` |
| R24 | DELETE `/v3/merge/orders/pending/:symbol` | DELETE [`/v3/trade/allOrders`](https://developer.woox.io/api-reference/endpoint/trading/cancel_orders_by_symbol) | Symbol ordinary+algo cancellation; `trading.rs:857` |
| R25 | POST `/v1/client/futures_leverage` | PUT [`/v3/futures/leverage`](https://developer.woox.io/api-reference/endpoint/futures/set_leverage) | Set leverage; `trading.rs:887` |
| R26 | POST `/v1/client/position_mode` | PUT [`/v3/futures/positionMode`](https://developer.woox.io/api-reference/endpoint/futures/position_mode) | Set position mode; `trading.rs:927` |

The official mapping names `/v1/public/kline`, but production calls `/v1/kline`.
R05's target is the current recent-kline contract, not an assertion that the two
legacy spellings have identical availability. Keep an explicit old-wire fixture
and obtain authorized replacement parity rather than silently correcting the
inventory to match the vendor table.

Not production owners at this base: legacy `/v3/balances`, `/v3/position/{symbol}`,
`/v1/client/hist_trades`, spot-margin leverage and Cancel-All-After. Their presence
in TS constants/scaffolds does not make them native runtime calls. Conversely,
R23 IS a native owner even though `woo.v3.contracts.ts` labels that route ownerless.
Do not copy that exclusion into the native migration. Do not dual-call R12 in a
shadow comparison or fall back from a rejected current-V3 write to a legacy write.

## Target contract and unresolved evidence

Use the current [integration guide](https://developer.woox.io/api-reference/guide)
and endpoint-specific schemas. Documentation was rechecked on 2026-11-04;
[official merged OpenAPI](https://developer.woox.io/openapi-merged.json) SHA-256:
`837464d1d063c111fea0669d5604f8f65c78d9d593f9814acc27bdc615025c04`.
The schema is evidence, not permission to adopt inconsistent examples blindly.

- **Account/valuation admission:** native `account.rs:1210` requires `FUTURES`;
  V3 account info describes `MARGIN`/`SPOT`. Prove the supported perpetual-account
  equivalent and valuation components before replacing that guard. Do not accept
  all modes or use missing equity/free collateral as zero. Identity, hedge mode,
  balance readiness and secondary-read identity must remain coherent.
- **Positions completeness:** the endpoint page requires `symbol` but its curl
  example omits it; examples return `data.positions`, merged OpenAPI specifies
  `data.rows`. Resolve both scope and envelope with vendor-backed evidence before
  deleting any stored position. Per-listed-symbol queries alone cannot prove
  absence of a delisted/unlisted holding. Unknown scope is not a flat account.
- **History and candles:** preserve fills/fees/PnL signs, units, native IDs and
  timestamps. The current fills path stays current; remove its legacy fallback
  only after cursor/range/empty/error controls prove coverage. Map income types
  explicitly. New historical candles use `before`/`after`/`limit`, not the old
  `start_time`/`end_time`/`size`; pin ordering, bounds and month interval spelling.
- **Settings:** leverage is scoped by symbol/margin mode/position mode, with
  GET and PUT contracts; cross/isolated vocabulary differs across endpoints.
  An acceptance is not readback. Preserve settings revisions and Unknown fences;
  do not apply the account's spot-margin leverage as perpetual leverage.
- **Ordinary/algo writes:** freeze intended symbol, side, size, reduce-only,
  margin/position side, TIF, client ID, child-parent IDs and protection before
  serialization. V3 amend/cancel IDs move into the documented JSON/query fields.
  Prove partial results and preserve accepted IDs without retrying uncertain
  placement, amendment, cancellation or protective replacement.
- **Book/ticker data:** migrate the REST snapshot and matching stream grammar
  together. Preserve snapshot timestamp and `prevTs`/`ts` joining, buffer before
  publication, and resnapshot on a gap. No invented ticker last price from a BBO.

## Authentication and WebSocket changes

`sign.rs:35-77` currently selects V1 form signing versus old-V3 JSON signing.
Current V3 signs HMAC-SHA256 over timestamp + HTTP method + exact path/query +
exact JSON body, with `x-api-key`, `x-api-timestamp`, `x-api-signature` and JSON
content type for POST/PUT. GET/DELETE parameters are query bytes. Public reads
remain unsigned. Use independent byte vectors; changing the path alone must
fail a signature control. Preserve separate primary-write and secondary-read
ownership, and keep raw secrets in their existing protected custody.

| Current production socket family (`socket_codec.rs:147-161`) | Replacement |
| --- | --- |
| `wss://wss[.staging].woox.io/ws/stream/:applicationId` | `wss://wss[.staging].woox.io/v3/public` |
| `wss://wss[.staging].woox.io/v2/ws/private/stream/:applicationId` plus legacy auth frame | Signed POST `/v3/account/listenKey`, then `wss://wss[.staging].woox.io/v3/private?key=<authKey>` |

Sources: [WebSocket introduction](https://developer.woox.io/api-reference/websocket_introduction),
[listen key](https://developer.woox.io/api-reference/endpoint/account/listen_key),
[subscription commands](https://developer.woox.io/api-reference/endpoint/websocket/subscribe).

- Listen-key body is `{ "type": "WEBSOCKET" }`; response contains `authKey` and
  `expiredTime`. A new key immediately invalidates the preceding one, and validity
  is 24h. Serialize acquisition per account/API-key owner, redact keys from URL
  logs/diagnostics, and define retirement and expiry before reconnect. Never
  mint a fresh key per racing reconnect or silently assume two hosts can overlap.
- `AuthMode::Required` currently waits for a legacy auth response. URL-authenticated
  V3 has no documented equivalent frame: core review must pin readiness/coverage
  evidence before implementation; do not invent an authenticated event on TCP open.
- Replace `event`/`topic` subscription/control envelopes with documented `cmd`
  messages and acknowledgements. Prove batch/topic limits (20 per command, 100 per
  connection), request correlation, retired generation rejection and PING/PONG.
- Current native interests are global `tickers`/`bbos`/`markprices`, plus per-symbol
  books/klines/trades; private interests are `executionreport` and
  `algoexecutionreportv2`. V3 public topic names/arguments must be mapped explicitly;
  private target includes account/balance/position updates where needed and
  `executionreport`/`algoexecutionreport`. Do not equate unchanged names with
  unchanged payloads, or subscribe to all catalog symbols without a bounded plan.

## Existing scaffolding and ownership

The SDK already has inert `packages/safe-cex/src/__tests__/reference/woo/woo.v3.*`
request/decoder/regular-write/algo-write/settings/socket/book-recovery/selector
modules and matching audit-regression tests. Their hard-false gates remain false.
They provide fixture/contract material, not active Rust migration or live evidence.
The historical plan referenced by their CLAUDE.md is absent in this mirror;
this proposal is self-contained. Reconcile its TS-only owner exclusions with
R01-R26 instead of introducing a second authoritative runtime selector.

## Phases and RED gates

Each implementation phase is a small SDK PR with immutable source/control review.
All `sdk-rs` landings wait for the next cut and the overseer's merge instruction.
No new dependencies, Cargo.lock edits, cap changes, live probes or smoke writes
are part of this proposal. No runtime fallback/shadow selector is proposed: land
and qualify coherent families, then activate each family once in its cut.

| Phase | Intended files | RED controls required before its source change |
| --- | --- | --- |
| 0: Audit (this PR) | This plan, own caller fragment, venue CLAUDE pointer | No runtime delta; verify endpoint inventory and links offline |
| 1: Signing and contract originals | `sign.rs`, `wire.rs`, focused WOO tests; reuse existing TS V3 fixtures | Independent HMAC vectors for GET/DELETE exact query and POST/PUT exact JSON; wrong method/path/body/key refuse; old form encoding cannot satisfy V3 vectors; no secrets in failure diagnostics |
| 2: Public reads and public socket | `seed_read.rs`, public portions of `lib.rs`, `wire.rs`, `socket_codec.rs`, WOO public/seed/socket tests, protocol harness | Changed envelopes/intervals/bounds; incomplete catalogue retains good state; sequence-gap reconnect never publishes a false book; bounded topic batches and stale acknowledgements; snapshot + delta join and ticker/mark/funding parity |
| 3: Account reads and private socket | `account.rs`, `identity.rs`, private `socket_codec.rs`, `wire.rs`, WOO account/identity/startup/security/socket tests | Resolve account-mode/position-scope blockers first; primary versus secondary identity; malformed/partial snapshot retains orders/positions/equity; new key invalidates old, concurrent reconnect creates only one key, expired/foreign key cannot become ready; retired frames cannot publish; #2981's 401/404/429/5xx attribution stays exact |
| 4: History | `history.rs`, `wire.rs`, WOO history tests and generated V3 tapes | Empty/last page, inclusive range/cursor repetition, fromId and filter validation, fee/PnL signs and timestamps; current fill route called once; no unexpected fallback to legacy; no history row enters live Store |
| 5: Regular/algo/settings writes | `trading.rs`, dependent `account.rs`/`wire.rs`, WOO trading/security tests, protocol harness | Production-bound place/amend/cancel with exact signed bytes, IDs and protection; account cancel reaches both ordinary and algo legs; partial/Unknown retains IDs and never resends; trigger replacement failure remains visible; fresh scoped leverage/mode readback cannot be satisfied by a pre-write read |
| 6: Cut qualification | Owned harness/tapes, caller evidence; policy changes only by explicit ruling | Fresh composed WOO tests, strict clippy/fmt/bans, exact-source replay and family/combined measurements; no blanket ledger waiver; authorized demo lifecycle only after keys and written request approval; NX/server pin follows SDK release/import |

The production-binding originals must distinguish queued-before-write reads from
fresh successor reads, just as current revision/order-snapshot ownership does.
Keep #2982's actual observation-time distinction: equal tape labels do not prove
equal observed state. Existing synthetic legacy tapes prove legacy behavior only;
new V3 tape responses and independent expectations are needed for V3 qualification.

## Release, rollback and caller gates

See the [own caller fragment](callers/woo-endpoint-migration.md). No caller may
switch protocol, discard applicationId configuration, change key permissions or
infer a user's key is invalid merely because this proposal exists. Before the
cut, agree per-key listen-key ownership across clients, native server and any
legacy process. Rollback cannot blindly restore a retired API or replay pending
writes: stop admission, retain uncertainty/reconcile, and return only to a known
working approved build after actual old work is retired. A confirmed old-route
shutdown requires a fix forward, not an automatic legacy fallback.
