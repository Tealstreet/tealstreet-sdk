# Phase 9: TypeScript caller inventory and deletion dependencies

Audited 2026-10-20 on the phase-9 readiness branch, against the repository's
`packages/` and `apps/` source. This is an inventory and an ordered future
migration plan. This change deletes no TypeScript venue. The authoritative
phase-9 decision and gate results live in [RUST_REWRITE.md](RUST_REWRITE.md).

## Log shim removal (PR6)

Engine diagnostics now use `Event` exclusively. WASM actions and Durable Object
messages use `{ op: 'event', level, code, message, fields }`; native clients get
`{ type: 'event', level, code, message, fields }`. Levels are debug/info/warn/error.
Host transport/command callbacks still use `onLog`; those are not venue shim
notices. The DO wraps them as `host.diagnostic` events, and its refused requests
as `host.rejected`; native oversized notices use `host.notice_dropped`.

New engine codes: `command.failed` for replyless diagnostics, `command.duplicate`
for an in-flight ID collision, `startup.incomplete` for the startup watchdog,
and `public_feed.rejected` for invalid trusted-provider updates. Startup retries
use `request.failed`; malformed discovery rows count `row.skipped`, and rejected
empty catalogues count `snapshot.discarded`, including when their warning is
filtered. Existing severity and retry cadence are preserved.

**Deploy safe-cex-server before Info+ venue events matter to native clients.**
The old NativeFrameSession rejects `event`; deployment is Sam's call and this
PR performs none. DO clients must accept the coded `event` envelope before
adopting this host version. tealstreet-next remains read-only in this lane.


## What is ready, and what is still a deletion blocker

The SDK exposes **49 registered exchange names**. `RUST_VENUES` includes **48**.
The two registries currently default **9 names** to `RustEngineExchange`:
`coinbase`, `coinbasespot`, and Orderly's `tealstreet-dex`, `kodiakfi`, `mode`,
`salsadex`, `what`, `woofi`, plus testnet-only `txflow`. They default the other **40 names** to TypeScript,
including **39 names with an available Rust adapter**. Registration in
`RUST_VENUES` proves the adapter exists; it does not prove every optional product,
credential mode, consumer, or live deployment is ready to delete.

Before this readiness change, only `ExchangeWorker` selected Rust from explicit
`engine: 'rust'`; `createExchange` ignored that option. This PR adds explicit
Rust selection to the SDK factory as well, preserving the default registries.
Consequently CLI, MCP and public factory consumers can opt in through the same
factory, but their current default creation paths still use the registry. An
explicit conformance suite is evidence for the tested frame/store/consumer
contract, not evidence of live private connectivity or an automatic migration
of saved accounts.

| Family / exact registered names | Rust adapter | Default registry / deletion condition |
| --- | --- | --- |
| Coinbase: `coinbase`, `coinbasespot` | Both | Rust default; active CFM/spot and saved spot-only. Legacy INTX fixtures remain compatibility coverage for retired Advanced Trade routes. Frozen TS reference remains a harness dependency. Inert Starbase code is not an enabled product. |
| Orderly: `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi` | All six | Rust default; all are broker options of one Orderly crate. `woo` is WOO X, a separate family. Frozen TS reference remains a harness dependency. |
| `txflow` | Yes | Rust default, testnet USDC perpetuals only; mainnet rejects before I/O and production/preview availability stays disabled. Retain `__tests__/reference/txflow` for independent public/private replay and audits, plus the production `exchanges/txflow/txflow.client-id.ts` helper used by the Rust wrapper. The former Phase-0 scaffold has been replaced; preserve [exact signing, account, order/query scope and measurement gates](TXFLOW_RUST_PORT.md). |
| Binance: `binance`, `binancepm` | Both | TS default; preserve USD-M and PM protocol/account/query scope and regression gates while switching both registries after consumer/routing/key-mode evidence. All four Binance Rust profiles now exist; shared TS implementation/reference files still have callers until each profile defaults/migrates. |
| Binance Spot: `binancespot` | Yes | TS default; signed WebSocket API user-data subscription with ACK/subscription-ID/liveness checks, native per-asset currency balances and USD totals; no derivative positions/settings. Native client-order-ID identity and supported regular/protective/order-list paths retain whole-batch preflight and final-fill-aware cancel/replace; unsupported derivative flags/STP/trailing or conditional parameters reject. Private history/fills/income retain the existing TS empty-result contract; public-trades/client-lookup queries unsupported. Preserve exact scope, live public versus documented private evidence and USD-M/PM controls in [BINANCE_VARIANTS_PORT.md](BINANCE_VARIANTS_PORT.md). |
| Binance COIN-M: `binanceinverse` | Yes | TS default; native contract order/placement amounts, USD-per-contract multiplier, base position contracts, coin-denominated PnL/margin and USD-valued aggregate balances with unpriced coin detail retained. Worker/main bridges preserve native count versus quote notional. Regular/stops/trailing endpoints and cancel/replace retain identity/terminal/final-fill proof; caller client IDs reject before batch writes. Public trades implemented; private history/fills/income retain TS empty results and client lookup unsupported. Preserve one-way/hedged/account sizing/routing evidence, and shared USD-M/PM regressions; [exact profile scope](BINANCE_VARIANTS_PORT.md). |
| Bybit: `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader` | All five | TS default; typed classification now reaches the real CLI TradFi preflight. The `bybit` linear account supports all four `enableInverseMarkets` / `enableOptionMarkets` combinations, category-specific feeds/reconciliation and native sizing, typed option terms/Greeks and explicit option APIs. Standalone inverse/spot/demo/Hyro retain their own profile boundaries; registration does not enable combined options on them. Actual TS/native and documented private fixtures do not establish live combined/private connectivity; Hyro connectivity remains unverified. Preserve `bybit.site`, public order parameters, shared-feed/default migration and [exact evidence](RUST_PRODUCT_SEMANTICS.md). |
| Bitget: `bitget`, `bitgetusdc`, `bitgetinverse` | All three | TS default; switch together after inverse/native-size and UTA ledger/trailing/optional-product checks. Preserve transport constants shared outside the family. |
| OKX: `okx`, `okxeu`, `okxdemo`, `okxeudemo` | All four | TS default; global linear/inverse swaps and EU `FUTURES` rows with `ruleType=xperp` now share the existing crate. Account-scoped EU discovery, private product subscriptions/reconciliation, scoped/joined histories and worker/main native chaser units are covered by typed fixtures. Preserve native placement counts, reference-price inverse base quantities, USD position PnL/margin and native ledger currency. Live public captures do not prove live private sizing/connectivity; host/demo, external-public account overlays, defaults and broker extraction still gate deletion. [Exact scope/evidence](RUST_PRODUCT_SEMANTICS.md). |
| `apexomni` | Yes | TS default; native zkLink/API auth plus wrapper/consumer gates; no inference to other ApeX products. |
| `asterdex` | Yes | TS default; V1/V3 auth, builder attribution and exported wallet approval utilities remain independent deletion dependencies. |
| `backpack` | Yes | TS default; perpetuals only, with native uint32 decimal caller IDs; do not imply Backpack spot. |
| `bingx` | Yes | TS default; explicit Rust selection is available; migrate default creation after gates. |
| `blofin` | Yes | TS default; preserve broker constants and native-unit consumer semantics. |
| `bitrue` | Yes | TS default; futures adapter; no spot coverage inferred. |
| `cryptocom` | Yes | TS default; preserve optional account/product gates before switching creation defaults. |
| `kucoin` | Yes | TS default; futures adapter, native contract-unit and broker configuration gates. |
| `kucoinspot` | Yes | TS default; opt-in native HF Spot in the KuCoin crate. Preserve native Spot IDs/base units, trade-account currency balances, broker/passphrase and secondary-read routing, caller-context amendments/cancels and fixed Spot settings. Futures coverage is a separate profile; see [scope and evidence](venue-ports/KUCOIN_SPOT.md). |
| `hyperliquid` | Yes | TS default; perps/HIP-3 adapter. Public builder/agent utilities and `hyperliquid.cloid` still have callers. |
| `hyperliquidspot` | Yes | TS default; distinct spot product in the existing Hyperliquid crate. Native spot identities, balances, signing, live public/documented private gates and native worker/late-consumer checks pass ([RUST_SPOT_PRODUCTS.md](RUST_SPOT_PRODUCTS.md)); no combined perps/spot account claim. Preserve wallet/builder/agent and cloid helper callers before deletion. |
| `kraken` | Yes | TS default; futures adapter. |
| `krakenspot` | Yes | TS default; distinct spot state machine in the existing Kraken crate, including Spot form signing, WS v2 token authentication and atomic txid amend. Live public/documented private gates and native worker/late-consumer checks pass; exact scope and independent review evidence are in [RUST_SPOT_PRODUCTS.md](RUST_SPOT_PRODUCTS.md); preserve futures regression and consumer/default migration prerequisites. |
| `lighter` | Yes | TS default; local signing on main/test chains 304/300. Remote-only account snapshot polling, payload-only public startup and mixed local/callback behavior match the current TS startup contract; no supported remote transaction signer in TS. Keep wallet approval/account-tier helpers, the wrapper's API-key-index parser and TS oracle; [variant evidence](LIGHTER_VARIANTS_PORT.md). |
| `lighter-robinhood` | Yes | TS default; explicit Rust alias with Robinhood main/test URLs, USDG, signing chains 466324/300 and no default zkLighter integrator attribution. Same credential modes and retained helper/oracle requirements as `lighter`; [variant evidence](LIGHTER_VARIANTS_PORT.md). |
| `mexc` | Yes | TS default; conditional/trailing and native-unit proofs still required when removing reference coverage. |
| `paradex` | Yes | TS default; retain binary/text feed and Starknet account mode checks. |
| `phemex` | Yes | TS default; retain native account identity and shared broker constants. |
| `weex` | Yes | TS default; retain protocol/routing gates. |
| `woo` | Yes | TS default; WOO X, not Orderly `woofi`; keep WOO position-history adapter used by Rust. |
| `polymarket` | Yes | TS default, development-only (`enabled: false`, `enabledInDev: true`), `supportsChaser: false`, native 1x leverage; generic algorithm fixtures do not enable venue features. Prediction CLOB V2 outcome shares with typed prediction/holding metadata; native local-wallet signature types 0/1/2/3 and supplied/derived L2 credentials. Limit/GTC only; no client IDs, market/conditional/bracket/reduce-only orders, or generic fills/order-history/income/public-trades query claims. Preserve authoritative final-fill-safe cancel-replace and bounded sampled price history. Read [venue-ports/POLYMARKET.md](venue-ports/POLYMARKET.md) for exact auth/product/query scope; private tests use documented fixtures, not live-key proof. |
| `grvt` | Yes | TS default; perpetual-only Rust catalog, with local EIP-712 signing (chains 325/326), API-key/session login, routed/direct private auth, subaccount discovery and mandatory builder authorization. Canonical caller IDs are decimal 1..2^63-1; limit/market/protective/bracket commands and final-fill-safe cancel-replace, one-way positions, real leverage reads/updates; margin-mode mutation unsupported. Bounded fills/history/funding/public-trade queries and definitive client-ID lookup, including the chaser fill-verification hook. Preserve public browser builder-authorization exports and signing/session routing. [Explicit retirement decision](GRVT_PRODUCTS_DECISION.md) resolves unexposed mapper-only Future/Call/Put paths; no future/options claim or TS deletion. Exact scope/evidence: [GRVT_RUST_PORT.md](GRVT_RUST_PORT.md). |
| `breakoutprop` | Yes | TS default, legacy DXtrade only and disabled in production/preview (`enabledInDev: true`); authenticated REST polling, no venue WebSocket or anonymous public feed. Username/password and exact FULL_TRADING account selection are required even for public-data-only reads; `supportsChaser: false` remains enforced by metadata. LIMIT/MARKET/last-price STOP, GTC, reduce-only as CLOSE, confirmed zero-fill cancel-replace; caller IDs, attached brackets/close-all and leverage/margin/mode changes unsupported. One-way/cross account model, native lot IDs and explicitly estimated margin/PnL; quote-derived books/live candles are synthetic. Generic fills/order/income history remain branded inert fallbacks, public trades unsupported; internal cancel history proof is not a history-query implementation. Public fixture is documented/scripted under the recorded waiver, with live credential capture/connectivity still unverified; read [RUST_BREAKOUTPROP.md](RUST_BREAKOUTPROP.md). |
| `bitunix` | Yes | TS default; opt-in USDT futures. Closed-PnL rows stay TS only (no `closedPositions` capability). Scope and gate evidence: [RUST_BITUNIX.md](RUST_BITUNIX.md). |

Every `ExchangeName` is in `RUST_VENUES`.
Do not silently route a venue through another family's adapter.

Native consumer proof includes both WebSocket venues and HTTP-polling venues.
BreakoutProp starts the actual Rust engine with documented DXtrade session/
account/quote responses; it emits no venue socket frames. Its fixture proves
polling dispatch, frame/store publication and consumer compatibility under that
input. It is not a live credential capture or evidence that new proprietary
Breakout Terminal accounts work with this legacy DXtrade adapter.

## tealstreet-next callers at cutover

Read-only inventory of every tealstreet-next use of the SDK that the landing
branch changes. The landing branch reaches master and tealstreet-next in one
cutover, so each item below must be handled in the same change set (or before).

**Inputs.** Original tealstreet-next audit: `origin/master` `41e538d78d` (2026-10-20), read with
`git grep`/`git show` only; Tealstreet/tealstreet-mobile `master` through the
GitHub API. SDK `origin/feat/rust-rewrite` `1fa06c93b5` (2026-10-21), which has
the merged contract PRs #1375, #1377, #1383, #1386, #1394, #1400, #1411, #1424,
#1401, #1439, #1441, #1443, #1445, #1447, #1451, #1453, #1462, #1467 and #1469 (venue construction and the Polymarket, Orderly, BreakoutProp and AsterDEX construction rules moved into the shared `safe-cex-venues` crate; messages and the `start()` rejection point unchanged) and the per-venue classified-error PRs (#1403, #1404,
#1417, #1425, #1429, #1432, #1434, #1436, #1437, #1438, #1442, #1444, #1446,
#1452, #1455, #1457, #1458, #1460, #1463, #1468, #1470, #1471, #1473, #1478,
#1479, #1475, and the B9 SDK PRs #1482, #1483, #1484), plus #1388 (engine log
events, `getHealth`), #1450 (B5), #1510 and #1531 (payload-signer routing, narrowed to Bybit/Binance).
The [2026-10-21 refresh](#contract-refresh-2026-10-21) checks SDK landing
`3d73437e589`, tealstreet-next local `origin/master` `fdd2389a825`
(`git grep`/`git show` only), and mobile `master` `8377a4f3f1e` through the
GitHub API. Existing line references retain the original NX audit revision;
new history-forwarding references below use `fdd2389a825`. Open PR proposals
are not merged contracts.

Construction follow-ups: checked SDK landing `570d15f3001` for #1563/#1567/#1575, retaining
those downstream inventory revisions. The existing NX script-editor options
already declare `extra.asterdexApiVersion?: 'v1' | 'v3'` and
`extra.hyrotraderMode?: 'demo' | 'production'`; no new caller edit.
The native account/fill follow-up checks #1430 at `3002a226469` against the same
NX revision; mobile `master` remains `8377a4f3f1e` (GitHub API).
The leverage follow-up checks #1522 at `faad41cfba5` with those inventory revisions.
The coded-health follow-up checks #1583 at `916585a30f2`, with NX `fdd2389a825`
and mobile `8377a4f3f1e` unchanged; its caller inventory remains cutover work.
The OKX port follow-up checks #1588 at `447134dbf24`; mirror `origin/master`
`1f91dfe4dcb` still has the old 8443 URLs. Live-master/hot-patch status below is
the overseer's 2026-10-21 report, not evidence of a deployed change.
The coded-only host follow-up checks #1586 at `ef68c4ba563`; its landed PR6
protocol/deployment section above is retained as cutover work.
The fill-identity follow-up checks #1592 at `389f42aa2e2`; focused live-fill
callers were rechecked read-only at NX `65e086a3c90`. Earlier inventory pins
still apply to the other caller rows.
The private-snapshot follow-up checks #1524 at `b69285f9909`; caller shapes
and diagnostic delivery remain the already documented SDK/host contracts.
The construction build follow-up checks #1599 at `e7f0b3d3628`; the shared
JSON remains a source-build input, now consumed by the Cargo build script.
The BingX overlap follow-up checks #1604 at `0d4f17a1bdd`, including its
integration with the landed auth work; publication and read-return semantics
are documented separately below.
The rejected-row follow-up checks #1603 at `7fcd7193fe1`; Orderly margin-mode
row isolation checks #1606 at `363139a6cc7`. Neither adds a caller API or wire
revision; the observable retention and diagnostic rules are recorded below.
The Bybit discovery-limit follow-up checks #1608 at `f55918d236b`, with its
startup cooldown changes separated from private reads and writes.
The credential-feedback/Coinbase-options catch-up checks #1487 at `1fb3c64d948`;
history ownership checks #1593 at `559a633a2c5`, and the direct SDK journal guard
checks #1591 at `083dbb5121b`. Focused NX hook checks use `65e086a3c90`.
The typed-error catch-up checks #1589 at `ef362708305`; the incoming venue
read/display notes are retained, and #1607/#1610 only change harness proof.
The envelope-only credential-evidence follow-up checks #1611 at `a07b6b35c87`.
The Bybit environment hint checks #1590 at `15434894bf5`; explicit market
refresh checks #1596 at `687fe0c1e08`. Shared public startup reporting checks
#1615 at `c63acfc19c2`; its required caller visibility work remains open below.
The Bybit private-row follow-up checks #1620 at `4592d985e8a`; leverage-ACK
read fencing checks #1621 at `7adb43c4beb`. History-marker ownership remains a
separate follow-up.
The BingX partial-error follow-up checks #1617 at `4e2f92c2622`, with its
existing host/worker fields and NX scaled-order consumer rechecked below.
The host-entropy follow-up checks #1624 at `0cd8f2b1657`; production defaults
and custom-host requirements below use the actual merge. NX construction roots
were checked read-only at `65e086a3c90`; shipped NX runtime verification remains
a cutover requirement, not evidence supplied by this source audit.
The history-owner batch checks #1628 (`d659c18c3f5`), #1629 (`a3c4e827722`),
#1630 (`b2092b50aca`), #1631 (`e9049c794c8`), #1632 (`41cd4914a70`),
#1633 (`a29812d8e49`) and WEEX #1627 (`9a444620376`). Bitunix also makes
failed reads reject as described below.
Bitget ordinary private-row repair checks #1625 at `02a0db19f3c`. Focused NX
query/result boundaries were rechecked read-only at `65e086a3c90`; B11 remains open.
The bridge capability follow-up checks #1637 at `673565c2e61`; final runtime
fields, construction metadata and pending-start behavior are recorded below.
NX caller ordering was rechecked read-only at `65e086a3c90`.
The Bitunix public-discovery follow-up checks #1636 at `62adf7651b0`; private
initialization ordering and the shared startup visibility scope are updated below.
The Bitunix socket/book follow-up checks #1639 at `77a5e5faaec`; NX empty-book
propagation, smoothing and canvas rendering were checked read-only at
`65e086a3c90`. The required caller repair below remains open.
The Bitunix secondary-read follow-up checks #1640 at `93bb218ffda`; identity,
egress and the limits of account-auth evidence are recorded below.
The Bitunix private-snapshot batch checks #1641 (`f156e3dc0aa`), #1643
(`163929bc0f5`) and #1645 (`6642e51468b`); ordinary position-read progress,
retained row families and readiness/preflight limits use those actual merges.
The Bitunix host-clock/login batch checks #1648 (`205ceb3fe8d`) and #1649
(`fc5ab92248a`); synchronized SDK time and login diagnostic/retry ownership
below use the merged source.
The Bitunix explicit-route follow-up checks #1650 at `b2059cce998`; existing NX
flag forwarding was checked read-only at `65e086a3c90`.
The Bitunix order-repair batch checks #1655 (`ba3d5bc69ee`) and #1658
(`67e81cfc13b`), including the exact 44 terminal-retention replay keys.
The packaging/account-recovery batch checks #1661 (`391dbe9af87`) and Bitunix
#1662 (`501fb664edb`); artifact mapping is a source/build inventory, not a
deployed-binary inspection.
The B11 SDK transport catch-up checks #1666 at `bba1f43126c`; the worker
envelope, derived mappings and MCP contract below use that actual merge. NX
forwarding and ingestion evidence remains read-only at `65e086a3c90`.
The Bitunix balance-progress follow-up checks #1665 at `8f7b0cee3ff`, including
the currentness predicate shared by financial publication and auth/recovery.
The settings/fill-identity batch checks Bitunix #1668 (`7fd70c13e34`) and
WEEX #1670 (`ce906e1368e`) at their actual merges.
The WEEX metadata-hook catch-up checks #1675 at `22c6c6133f2`; its incoming
B8 and hook inventory updates are retained.
The GRVT builder-error catch-up checks #1679 at `bcf9fc37afe`; the validated
helper and required NX migration below use that actual merge.
The first REST-book batch checks Backpack #1680 (`6e14b6a0ef9`) and MEXC
#1681 (`3db712f0446`) at their actual merges; other pending venue query PRs
are not covered by these pins.
The next query/measurement batch checks Crypto.com #1685 (`84f150c76dd`) and
browser cold-start #1688 (`b6cb612bc7c`). Browser observations below identify
their own dispatch head and do not certify a deployed NX runtime.
The next REST-book batch checks GRVT #1686 (`850bc25e51e`) and Paradex
#1687 (`90df20d2f14`) at their actual merges. The final B8 batch checks
WEEX #1690 (`2ff8452aa53`) and Hyperliquid #1693 (`fa960d9cfcf`), closing
the listed SDK-method gaps without closing separate caller or owner work.
The fee-subject catch-up checks Hyperliquid #1701 (`b1510f267c7`) at its
actual merge; harness-only #1698 adds no caller contract row.
The browser warning catch-up checks #1703 (`0888c62bdf8`); informational
median warnings do not change production loaders or caller APIs.

**Impact key.** *CT*: breaks at compile time. *RT*: compiles, behaves
differently at runtime. *Blocker*: no new equivalent exists yet; cutover
cannot proceed for the affected venues/hosts until one does.

**Re-run** when a contract PR merges: repeat the three scans in
[Audit method](#audit-method), then update the Status and Resolving PRs columns
and the rows that cite that PR. Open contract work is tracked separately from
the merged behavior below. #1430 introduced financial tails under revision 1;
#1592 now writes FILLS revision 2 with execution identity, as detailed below.

### Contract refresh, 2026-10-21

Initial batch checked landing `3d73437e589`, including the merges after the last refresh
(#1549, `00efd136bb6`) and the earlier candidates named in the handoff.
The construction follow-ups check through #1575 at `570d15f3001`, including
#1563, the #1550 logging merge and #1518's internal failure-path refactor.
The subsequent #1558 logging-level merge is checked at `e757d8d7990`.
Native account reads, identities, fill financials and nuke check #1430 at `3002a226469`.
Fresh leverage reads and combined capability records check #1522 at `faad41cfba5`.
Coded health, SDK event metadata and worker forwarding check #1583 at `916585a30f2`.
The following rows record the caller effect, including changes requiring no
downstream edit. PR numbers refer to Tealstreet/tealstreet-sdk.

| Merged PR / landing commit | Contract and caller effect |
| --- | --- |
| #1709 / `6f815755c37` | Shared synchronized SDK clock ingress now covers every Rust venue; the per-venue `hostClock` capability is removed and wire bit14 reserved. This supersedes #1520/#1648/#1693 clock opt-ins. Explicit injected clocks remain authoritative. Upgrade paired SDK/engine artifacts; no new account or constructor argument. See the clock supersession below. |
| #1476 / `0741419bf2` | Local signing-credential validation moves from TS into `safe-cex-venues`, shared by WASM/native construction. The existing error text and `start()` rejection remain; engine loading now precedes this validation. B2's hosted TS hedge and #1531's payload-signer routing remain in force. |
| #1519 / `9363852438` | One options blob reaches every venue; each Rust arm consumes its fields. No caller edit. A non-AsterDEX secondary read key without a secret now gets `Secondary read credentials require a nonempty key and secret`, replacing a decode error; AsterDEX keeps its separate read-agent rules. |
| #1520 / `418e4f6ebc` | Engine capability `hostClock` (CAPABILITIES bit 14) replaces the bridge's virtual-clock name list for Backpack, Coinbase/spot and Orderly. Same corrected signing time; no caller edit. Ship matching engine and reader. Historical opt-in superseded by #1709; bit14 is now reserved. |
| #1392 / `48aeec3783` (earlier merge, stale caller row corrected here) | Resolved Base polling policy now scales every Rust venue's periodic reconciliation, including server multipliers and auth backoff; `Infinity` pauses and a finite update resumes. Socket supervision, startup and explicit reads are unaffected. Existing caller configuration works unchanged. |
| #1529 / `ea39da2a1b` | BloFin historical-order `amount`/`filled` use base units, as fills/open orders already do; unknown-market rows are omitted. The owner-run `historical_orders` backfill remains required before Rust ingestion. |
| #1531 / `a4d17f8a88` | Only private Bybit/Binance-family `payloadSigner` accounts fall back to TS. Other non-Paradex private Rust accounts refuse the option as `unsupported`/`notSent`; the construction row below already records the MCP effect. |
| #1542 / `7ee1411110` and #1554 / `f52f08ea65` | Kraken futures returns the available newest ledger page when its 500-row page cannot be traversed, with `history.incomplete`; Binance exhausted income walks also warn. #1554 adds command-correlated, non-enumerable `incomplete: true` to those array results in the JS WASM host. Forwarding and ingestion must preserve/read it (B11). Kraken HTTP failures also retain the venue error code in their text. |
| #1551 / `c2a36b7ca1` | Binance native stop-limit orders refuse `updateOrder` as `unsupported`/`notSent`, before cancellation, instead of becoming stop-market orders with the wrong trigger. Regular stop-market amend remains supported. Surface the refusal; no new call shape. |
| #1513 / `084a326abf` | Cancel/amend of a just-acknowledged placement may wait for its order-store row on opted-in venues, up to 10 s before dispatch. Chaser STOP already waits for placement IDs; no caller edit. A timeout still requires reconciliation before retry. |
| #1527 / `8c1e779021` | A book-cadence frame carrying fills also carries pending store sections; worker/main consumers apply orders before emitting those fills. No reader or FILLS revision change. Venue-delivered execution-before-order ordering remains possible. |
| #1538 / `c18c30e45e` | First venue batch uses coded log events; duplicate auth notices in one dispatch count once. Native sessions use `event` notices; DO logs carry `code`/`fields` (#1388). No new `exchange.on` code payload: B9's classified fields remain the host integration. |
| #1547 / `e8ad6facc1` | Hyperliquid zero TP/SL prices omit that bracket leg; unified/portfolio spot holdings populate `balance.coins` for the margin panel. `currencies` remains unset to preserve sizing. No caller edit. |
| #1555 / `92a1608191` | Bybit OHLCV with `limit` ignores `from` and returns the newest bars ending at `to`, as TS. Chart/history callers keep the same arguments. |
| #1546 / `2bd1ef7ae8` and #1556 / `5b0c16a952` | Mobile options/costing and existing production issues are documented in [RUST_MOBILE_B3.md](RUST_MOBILE_B3.md). B3 and the app fixes remain owner items. |
| #1501 / `550e534e08` | Spot-enabled `hyperliquid` accounts now run on Rust; the perp engine merges spot pairs, holdings and orders. B8's product fallback is superseded. The keyless hosted/payload-signer TS routing remains in place. |
| #1506 / `c0d37f95d9` | Optional `UNDERLYING_TOKENS` frame section 20 carries merged Hyperliquid spot token names; both consumers reconstruct `Market.info.hlTokenName`/`hlTokenFullName`. The Margin panel's underlying-token risk disclosure works without an app edit. Ship matching engine/reader/applier. |
| #1560 / `d68a47b7c7` | Orderly bounded OHLCV reads now use native history and preserve caller bounds; trailing-stop display uses the native extreme/callback with BUY adding and SELL subtracting. Candle/order shapes and write contracts are unchanged. The landed display/read note is preserved below. |
| #1550 / `012e073e4f` | The remaining fifteen venue families use coded log events, completing #1538's migration. Message text and levels remain unchanged; B9's classified-error integration and the native/DO notice contract remain the caller requirements. |
| #1563 / `60a3dc1e79` | Shared Rust AsterDEX API-version resolution now also supplies optional WASM construction metadata. The wrapper forwards only a valid explicit `extra.asterdexApiVersion` override and adopts the native version for signing eligibility/STP checks after loading. Synchronous pre-host and older-host inference remains parity-tested; account/options/trading call shapes are unchanged. |
| #1567 / `d06eabae16` | One shared JSON rules table derives signing/public-mode eligibility synchronously in TS and before native validation/startup. Optional raw `publicMode` carries caller intent; `construction()` adds optional `canSign`/`publicDataOnly`, and the WASM module exports no-I/O `resolveConstruction`. Existing application options and legacy native/harness input remain compatible; the explicit-private Paradex refusal moves into the registry. See [Construction metadata and public mode](#construction-metadata-and-public-mode-1563-1567). |
| #1518 / `8b115cefd7` | Internal `Cx::done` is success-only and `Cx::fail` owns command failures. Hosts still emit the existing `done` error shape with classified detail; message text is unchanged. No caller edit or new wire revision. |
| #1575 / `570d15f300` | Rust selects HyroTrader endpoints from optional raw mode/network maps, retaining legacy explicit endpoints when that config is absent. Private non-Paradex `payloadSigner` refusal moves into the registry; WASM registry failures carry JSON `Error.venueError`, which the SDK converts to `ExchangeError`, preserving `unsupported`/`notSent` for that refusal. Factory routing and application options stay unchanged; see [Host construction rules](#host-construction-rules-1575). |
| #1558 / `e757d8d799` | At migrated retried-read sites, the first failure is Error; repeats with the same digit-normalised message at that site are Warn until 90 s after its last Error, or until the message changes. Text stays stable, reducing repeat toast/Sentry reports while retaining periodic Errors for auth detection. Backpack optional child-leg failures now emit Error instead of Warn. No caller shape or classified-error field changes. |
| #1430 / `3002a22646` | Account reads synchronize authoritative native state without a network refresh or new streaming freshness observation; worker mirrors forward the reads. Native identity lookup expands by capability. Live fills add optional `realizedPnl`/`fee` tails under the existing revision-1 header, and native nuke batches closes without automatic retry. See [Native account reads, identities and fills](#native-account-reads-identities-and-fills-1430) for caller and host requirements. |
| #1522 / `faad41cfba` | BingX/WEEX `fetchLeverageForSymbol` now makes fresh native symbol reads (`leverageQuery: 'onRead'`), retaining `account: 'query'` in the combined capability records. The metadata-refreshed bridge issues a native command for uncached symbols instead of returning a cache miss immediately; valid published rails remain cached. Existing UI calls and `FetchedLeverage \| null` shape stay unchanged. |
| #1583 / `916585a30f` | SDK `engine-event` and the same worker op expose received coded diagnostics. Legacy Error and Info/Warn delivery retains event metadata for caller-controlled duplicate suppression; Error telemetry adds scalar `event_code`. `ExchangeHealthMonitor.processEvent` classifies codes/optional typed detail, and `processHealth` copies authoritative engine counters. Downstream handlers still need migration; preserve the [coded diagnostic caller inventory](#coded-diagnostic-caller-cutover-pr5). #1586 removes the engine log shim; host protocol and deployment requirements are recorded above. |
| #1588 / `447134dbf2` | Rust/TS OKX public/private/business WebSocket URLs use default TLS 443 across global/EEA live/demo, preserving host/path; relay upstream metadata changes the global live private/business targets. SDK arguments stay unchanged. Proxy upstreams and live master need a separate owner-run port migration before the public-internet 8443 deadline; see [OKX operations prerequisite](#okx-websocket-port-operations-prerequisite-1588). |
| #1586 / `ef68c4ba56` | Removes `Cx::log`, `LogLevel`, `Notice::Log`, WASM `log` actions and native `log` notices. Diagnostics use coded `event` envelopes; DO changes from `op: 'log'` to `op: 'event'` with `warn`, preserving its host `onLog` callbacks as coded host events. Deploy compatible safe-cex-server before native clients receive Info+ venue events; DO clients must accept the new envelope before host adoption. Sam owns that deployment. See [Log shim removal](#log-shim-removal-pr6). |
| #1592 / `389f42aa2e` | Rust always writes FILLS revision 2 with optional execution-ID refs and aligned financial columns. Updated readers retain revisions 0/1; old readers reject revision 2, requiring coordinated producer/consumer builds. Hyperliquid perp/merged spot/spot-only `tid` becomes SDK fill `id`, with `orderId` separately preserved; other venues retain order-ID fallback. See [Fill identity and wire compatibility](#fill-identity-and-wire-compatibility-1592). |
| #1524 / `b69285f990` | Private snapshots (batch-3 venues) isolate rejected rows while valid peers commit; stored rows protected by rejected identities remain visible, including Bitrue protective children and Kraken open orders. Unidentified rows prevent absence-based removal within their snapshot scope. Coded `row.skipped` / `snapshot.discarded` diagnostics use the existing event/health paths. No new caller shape; see [Private snapshot row isolation](#private-snapshot-row-isolation-1524). |
| #1599 / `e7f0b3d36` | Rust construction rules are compiled from the shared JSON into a static table by Cargo `build.rs`, replacing runtime JSON decoding. B1 source builds still require the JSON input; compiled engines contain the table. TS rules, parity and caller options/eligibility are unchanged. |
| #1604 / `0d4f17a1b` | BingX revision/query-generation races and reads issued during pending settings writes fence stale store publication while returning usable cached buy/sell rails, with validated-read fallback independently per missing rail. Overlap alone no longer fails the read or causes `null`; genuine read failures remain possible. Current reads retain atomic leverage/margin publication; caller APIs and write retry behavior stay unchanged. |
| #1603 / `7fcd7193f` | Migrated Binance, Coinbase and WEEX snapshot order-rejection paths and Kraken Futures order skips (Kraken spot unchanged), plus Coinbase spot holding skips emit `row.skipped` at Error when no stored copy is retained, Warn when one is kept. Bounded identity caches suppress repeats but allow a prior Warn to escalate when its retained copy is lost; mapping/clean reads re-arm reporting. Store retention and caller shapes stay unchanged. |
| #1606 / `363139a6c` | Orderly margin-mode lists retain a rejected symbol's old default while valid peers commit; an unidentified row protects the old defaults scope. Clean empty lists still clear, structural failures and settings-revision fences still hold, and this path emits Warn-level `row.skipped`. No new caller shape. |
| #1608 / `f55918d23` | Bybit discovery retains typed HTTP-200 `retCode: 10006` evidence and headers, including missing-list error envelopes, and delays retries using the later usable reset/Retry-After hint and existing deadline. Missing hints fall back to 10 s; hints are bounded by the shared 1 h cap and HTTP403 keeps its 10 min floor. Already-waiting categories are extended without extra attempts. No new caller API or private/write retry behavior. |
| #1487 / `1fb3c64d9` | Optional typed `accountAuth` feedback carries credential slot, read scope and accepted/rejected outcome across matching hosts; the SDK consumes it for isolated polling-backoff streaks. Native/DO clients opt in, preserving older clients. Rust Coinbase ignores defined saved read-credential fields and their proxy in an effective option copy; caller/saved options stay unchanged. See [Credential feedback and saved options](#credential-feedback-and-saved-options-1487). |
| #1593 / `559a633a2` | Native/DO command diagnostics remap `fields.command` to the owning client's ID and target that owner. Command-correlated `history.incomplete` survives an Error minimum level as metadata; direct JSON consumers must correlate it before the result and carry completeness onward. SDK worker/hosted result metadata now preserves the flag as described in B11; NX ingestion/coverage remains open. |
| #1591 / `083dbb512` | Direct SDK journal folding rejects flagged fills and symbol-scoped income. With complete account-wide fills it discards flagged income enrichment and leaves funding unattributed. This guards direct folds; SDK result forwarding now preserves the flag; NX historical coverage remains caller work (B11). |
| #1589 / `ef3627083` | Bitunix command failures gain the existing classified-error/partial-detail contract while preserving native text/codes. Dispatched uncertain writes stay `unknown`; observed effective legs/IDs survive later batch failure, and only definitive rejections enter existing recovery writes. B9 callers must inspect fields and reconcile effects before retrying; no new API shape. |
| #1611 / `a07b6b35c` | For eligible signed BingX/MEXC/WEEX list reads, a typed success response (BingX/MEXC envelope; WEEX root array) with its required list proves Accepted even when rows are skipped, uncatalogued or held. Malformed/missing required envelope data supplies no outcome before grouped auth state changes; typed credential rejection retains precedence. Row diagnostics/financial retention, classifier scope and caller shapes stay unchanged. |
| #1590 / `15434894b` | A startup fee-read 10003 on eligible built-in live linear/demo Bybit can add an opposite-environment key hint after one bounded signed diagnostic GET using the rejected credential. Failure/timeout retains the original rejection; native code, credential role and deactivation policy stay intact. No automatic environment switch or trading retry. |
| #1596 / `687fe0c1e` | Bybit `refreshMarkets()` stages enabled public categories atomically, shares concurrent discovery and returns added/removed membership after publication. `marketRefresh` capability uses wire bit15; unsupported engines keep cached behavior. Failed discovery retains the old catalogue; shared-feed consumers must refresh the source and restart provider/accounts. See [Bybit explicit catalogue refresh](#bybit-explicit-catalogue-refresh). |
| #1615 / `c63acfc19` | OKX/BloFin/Bitget public startup emits one Warn `startup.degraded` after six failures of one required read, while capped retries continue; after a prior degraded report, Info `startup.recovered` follows usable markets and tickers. Typed cause fields use the existing Event wire, without terminal `StartupFailed`. Required NX event forwarding, per-exchange degraded/recovered state independent of reconnect-toast suppression, legacy deduplication and pre-start CLI listeners remain caller work. |
| #1620 / `4592d985e` | Ordinary Bybit REST/WS order and position rows isolate malformed peers while retaining rejected cached parents/protection or symbol exposure/Greeks. Unknown identity protects absence removal; valid peers can progress, with coherent same-symbol modes. Rejected WS evidence protects pending REST joins and coalesces read repair without globally starving unrelated rows. Warn `row.skipped` uses existing diagnostics; Hyro, execution/wallet/history handling and caller shapes stay separate. |
| #1617 / `4e2f92c26` | BingX sequential placement failure now retains earlier acknowledged native IDs in existing typed/legacy partial-error details after a later rejection, transport loss or malformed acknowledgement. Existing classification/outcome and unsent-tail behavior remain; reconcile known effects and any uncertain failed step before retrying. No new API or automatic write retry. |
| #1621 / `7adb43c4b` | Ordinary Bybit accepted leverage writes, including native `110043` not-modified acknowledgements, fence older in-flight position joins before publishing the selected leverage. A stale join cannot restore old leverage/settings; an active join coalesces a fresh read after settling. Refused writes leave authoritative reads eligible. Hyro's atomic path, position-mode semantics, caller API and write retries are unchanged. |
| #1624 / `0cd8f2b16` | Core engine/registry construction requires explicit identity and signing entropy; missing providers fail as typed `invalidInput`. Production WASM/JS hosts bind WebCrypto and native factories bind OS randomness. Optional JS `entropy`/`signingEntropy` overrides are independent, checked and installed before start. Existing SDK factory callers need secure runtime availability proof rather than new account fields or constructor arguments; custom core hosts must supply providers. See [Host entropy and NX construction](#host-entropy-and-nx-construction-1624). |
| #1625 / `02a0db19f` | Bitget Classic/UTA ordinary private WS order/position rows isolate malformed peers, retain rejected cached identities/protection and coalesce read-only repair. Rejected observations do not renew financial freshness or starve unrelated fresh REST peers; held positions do not republish stale settings. Initial all-rejected traffic does not prove loaded-empty readiness. Financial fill admission and caller shapes stay unchanged. |
| #1628 / `d659c18c3` | BloFin closed-position page-cap `history.incomplete` now includes the query command before retained rows complete, allowing the existing host to flag that result. Cap, rows and failure policy stay unchanged. |
| #1629 / `a3c4e8277` | Bybit closed-PnL and order-history page-cap markers carry their query command before retained-row completion/window continuation. Existing pagination, rows and failures stay unchanged. |
| #1630 / `b2092b50a` | Hyperliquid rolling-retention order-history marker carries its query command before completion; retained rows and transition/time/limit policy stay unchanged. |
| #1631 / `e9049c794` | OKX closed-position cap and noncomposite history cap/cursor-stall markers carry their query command. EEA composite failures and existing financial/history policy stay unchanged. |
| #1632 / `41cd4914a` | Paradex bounded history page-budget marker carries its query command before partial completion. Unbounded page-cap and other stall failures stay unchanged. |
| #1633 / `a29812d8e` | Bitunix history transport/HTTP/envelope/malformed-row failures now reject instead of resolving earlier rows as complete. Genuine remaining-page caps or stalled order/income paging retain rows with command-attributed `history.incomplete`; a completed final page does not get a false cap marker. Callers must preserve failure versus partial versus complete outcomes before coverage. |
| #1627 / `9a4446203` | WEEX income lifetime-budget, truncated-chunk, advertised-cursor, failed-page and terminal-failure markers now carry their query command. Existing retained rows, chunk continuation, pacing and terminal rejection policy stay unchanged; a marker on a rejected query is not a successful partial array. |
| #1637 / `673565c2e` | Runtime bridge policy now follows engine capability flags/enums; shared construction rules retain synchronous policy and expose optional `construction().bridge`. Capability-dependent public calls wait for an existing startup; no-start/failed-start calls reject with method-named `ExchangeError` `invalidInput`/`notSent`. Worker forwarding stays first. NX explicit readiness is recommended; typed rejection handling is required. See [Runtime capability readiness](#runtime-capability-readiness-1637). |
| #1636 / `62adf7651` | A usable Bitunix catalogue starts private initialization before ticker seeding finishes. Valid public rows survive malformed typed peers; unusable required seeds retry without false loaded-empty readiness. Independent discovery retries honor rate floors and use existing startup.degraded/recovered codes. Ticker recovery does not restart private settings/polling; private/history failure policy and caller shapes stay unchanged. |
| #1639 / `77a5e5faa` | Bitunix retains socket supervision/backoff per feed, rejects retired socket/timer events and replays current interests once on replacement. Futures-socket retirement immediately publishes empty books before retry; public ticker-shard retirement leaves the separate futures books intact. **Required, high priority:** NX must propagate empty-book invalidation through raw/smoothed listeners and clear displayed depth, rather than keep stale levels. See [Bitunix disconnected books](#bitunix-disconnected-books-1639). |
| #1640 / `93bb218ff` | Bitunix signed GETs now use configured secondary read credentials and matching secondary egress; mutations, posted reads and private WS login stay primary, and public discovery stays unsigned. Account/settings requests tag the selected slot and scope, without adding native auth-outcome classification. Existing caller options remain; malformed secondary identities are rejected before I/O. |
| #1641 / `f156e3dc0` | Ordinary Bitunix position reads can publish financial snapshots despite intervening WS routing-ID notifications. Reads coalesce around an active serial with bounded lost-read replacement; accepted leverage/margin/mode writes still fence older snapshots, and command-owned preflight keeps full generation checks. Caller APIs and write retries stay unchanged. |
| #1643 / `163929bc0` | Malformed typed Bitunix position rows retain the affected symbol's cached hedge legs, settings and REST routing IDs while valid peers advance. Unknown identity protects absence; valid explicit zero rows can still close positions. Partial mode conflicts discard atomically; partial reads cannot establish initial readiness or approve trading preflight. |
| #1645 / `6642e5146` | Malformed Bitunix regular/TP-SL REST rows retain rejected native-ID families and routing metadata while admitted peers advance. Unknown identity holds absence within its endpoint scope; admitted parents can remove obsolete children. Only admitted groups advance execution baselines, and partial initial/preflight reads remain insufficient. Order/TP-SL WS scheduling and global generation fences are separate. |
| #1648 / `205ceb3fe` | Bitunix advertises the existing `hostClock` capability, so the SDK supplies synchronized virtual-clock time before first engine start and later callbacks, including private login. REST/login timestamp units and signing formats stay unchanged; callers add no account field or constructor argument. Historical hostClock opt-in superseded by #1709: flag removed, bit14 reserved, shared SDK ingress for all venues. |
| #1649 / `fc5ab9224` | Missing/null/malformed private login results now emit sanitized Warn `stream.malformed`; explicit boolean false retains Error `auth.rejected`. The existing two-second same-socket login retry coalesces repeated failures; success/retirement cancel it and stale timers cannot sign on a replacement. Deactivation and trading retry policy stay unchanged. |
| #1650 / `b2059cce9` | With a routing-capable transport, explicit Bitunix `tealstreetWsRoute: true` selects each channel's declared fallback proxy from the first connection and on reconnect. False/unset keeps the existing two-attempt direct-first policy. A direct-only provider stays direct and warns; callers must choose a routing-capable provider to force proxy use. Existing options and native routing-plan schema stay unchanged. |
| #1655 / `ba3d5bc69` | Bitunix regular/TP-SL reads keep independent serials and ten-second lost-read deadlines; ordinary notifications coalesce without postponing the first one-second repair. Earlier reconciliation can retire venue-terminal orders sooner than TS. The private replay declaration adds exactly 44 tape-pinned terminal-removal/index-shift paths; other checks and all prior declarations stay intact. |
| #1658 / `67e81cfc1` | Ordinary Bitunix order snapshots admit healthy REST peers while preserving newer WS native families/terminal absence and bounded malformed-row scope. A separate write revision fences stale publication across placement/cancel/ACK/cleanup, including partial cancellation failure. Initial halves remain atomic, command-owned prerequisites stay strict, and no trading write is retried. |
| #1661 / `391dbe9af` | WASM-only compiler profiles shrink the combined host and selected family builds without changing loader APIs or native profiles. Delivery still depends on the consumer: 28 browser families, combined Node for hosting/MCP, combined web embedded by the compiled CLI, or a deployment-injected Workers/React Native module. See the refreshed packaging inventory below. |
| #1662 / `501fb664e` | Current Bitunix signed account/settings GETs now supply selected-credential scoped auth evidence. Strict native refusal is distinct from ambiguous failures; grouped order endpoints/settings symbols cannot heal each other. Background reads use bounded backoff and a shared rate pause; a blocked command prerequisite fails once `notSent`, with no write replay. |
| #1666 / `bba1f4312` | Successful worker replies carry optional request-scoped `data.completeness`; the matching receiver restores the array marker before resolution/observers. SDK derived histories preserve it. Flagged MCP text JSON is `{requestId, rows, completeness}`, while unflagged arrays stay unchanged. Upgrade sender/receiver together and update MCP consumers; NX ingestion/coverage remains open (B11). |
| #1665 / `8f7b0cee3` | Ordinary Bitunix balance reads can publish current financial state and scoped auth evidence during WS notifications. Reads coalesce around their serial/ten-second dispatch bound; accepted position-mode ACKs fence older responses through a separate write revision. Recovery gates, balance mapping and caller/write-retry APIs stay unchanged. |
| #1668 / `7fd70c13e` | Bitunix per-symbol leverage/margin metadata and scoped auth/recovery use the current symbol attempt serial plus accepted-settings-write revision, so WS position-ID traffic no longer starves flat-symbol settings. Accepted leverage/margin/mode ACKs still fence older reads; caller APIs and command-owned prerequisites stay unchanged. |
| #1670 / `ce906e136` | In-scope WEEX fill-history rows with parsed empty `id`/`tradeId`/`fillId` remain skipped without invented identity. Useful peers resolve with one per-query Warn `row.skipped` (`field: id`) and owned `history.incomplete`. Error-only logging retains the semantic marker, while the ordinary row diagnostic is suppressed. Strict parsing and method shapes remain; B11 consumers must not complete coverage for retained partial or flagged-empty rows. |
| #1675 / `22c6c6133` | WEEX now reports `onReadAndMetadata`, so the existing ready private `updateSymbolMetadata(symbol)` hook performs a fresh native configuration read even for cached/flat symbols. Malformed reads reject while good rails remain; public-only skips private I/O. Explicit reads and caller arguments stay unchanged; await capability readiness and preserve ordinary hook rejection handling. |
| #1679 / `bcf9fc37a` | Definitive GRVT builder refusals carry validated flat code/reason/raw caps through host errors and SDK command RPC. The SDK-root `builderAuthorizationOf(error)` helper works without class identity. **REQUIRED:** migrate NX's live class-only handler, preserving reason-specific modal/toast, cleanup/rethrow and approval reload. Upgrade sender/receiver together; keep the command's existing outcome and do not replay a write. |
| #1680 / `6e14b6a0e` | Backpack adds unsigned request-owned REST `fetchOrderBook` and advertises the capability. Native queries need no market seed, credentials or subscription; caller limit is ignored, lexical price IDs and native side order remain, `total` is zero and no update ID is returned. Standalone JSON admission uses the precise shared book-command feature. |
| #1681 / `3db712f04` | Standalone MEXC family builds now admit JSON `fetchOrderBook`, matching their existing capability and native REST implementation. Mapper, limits, base-unit scaling, signing/auth and rate policy are unchanged. The existing family-artifact gate now probes actual JSON admission for advertised queries without dispatching I/O. |
| #1685 / `84f150c76` | Crypto.com adds an independent unsigned live/UAT REST book query at fixed depth50, ignoring caller limit. It retains native order, lexical price IDs, zero quantities and `total: 0`; only the first returned book is mapped and no update ID is exposed. Typed public failures settle their owner without account-auth or subscription changes. |
| #1688 / `b6cb612bc` | Adds landing-push/manual-dispatch Chrome DedicatedWorker module-readiness observations for 28 production family-loader paths plus a combined-web diagnostic, three fresh processes/profiles each. Measurement integrity errors fail; #1703 adds informational median warnings without a timing-failure gate. No production API/default-loader or mandatory NX caller change is added, and B1 delivery remains open. |
| #1686 / `850bc25e5` | GRVT adds an unsigned request-owned MDG REST book query. Known markets POST `{i: nativeId, d: 50}` to `/lite/v1/book`, correcting the TS request against the official schema; caller limit is ignored. Unknown markets resolve an owned empty book without HTTP. Full/lite aliases retain native order and cumulative base-amount totals, without IDs/version or decimal rescaling. Query failures do not refresh the private session. |
| #1687 / `90df20d2f` | Paradex adds unsigned request-owned REST books on live/testnet roots with encoded raw symbols, default depth20 and explicit caller depth unchanged. Native order, duplicates and zero quantities remain; each level's `total` is price × amount in quote units, not cumulative. No IDs/version, scaling or subscription publication is added. |
| #1690 / `2ff8452aa` | WEEX adds an owned unsigned Contract V3 REST book query, default limit200 and explicit limit unchanged. Native order, null-coalesced aliases, cumulative base-amount totals and exact string `lastUpdateId` remain; no contractVal scaling or level IDs. Strict malformed financial/version parsing rejects atomically. Shared GET pause may defer a new owner without retrying the failed owner. |
| #1693 / `fa960d9cf` | Hyperliquid perpetual `scheduleCancel(time?: number \| null): Promise<void>` now arms a signed absolute UTC-ms deadline or clears on null/omitted time. SDK/host pre-JSON guards prevent unsafe values becoming accidental clears; native deadlines are checked again before signing. The owned promise requires an accepted control ACK; uncertainty stays `unknown` without replay or account-state inference. Renewal remains caller-owned; spot stays unsupported. The newly enabled perp hostClock adopts synchronized SDK time for all perpetual engine work/signing, not only scheduling. Historical hostClock opt-in superseded by #1709: flag removed, bit14 reserved, shared SDK ingress for all venues. |
| #1701 / `b1510f267` | Hyperliquid perpetual fee-tier reads use the configured vault ahead of the trading account (`applicationId`), rather than the API signer. Unresolved private identity waits for existing `userRole` discovery before queuing `userFees`. Signer inputs, existing `Market.makerFee`/`takerFee`, options and fee arithmetic stay unchanged. No new NX field or call is required; the analogous TS defect remains separate owner work. |

#1543 (retry jitter), #1552 (bounded Binance Retry-After), #1545 (WASM
headroom), and the #1541/#1544/#1548/#1557 replay-clock fixes do not change a
downstream call shape. They add no cutover edit here. #1592 separately lands
execution-ID attribution and FILLS revision 2; #1430 did not introduce either.

### Summary: cutover blockers (no new equivalent)

| # | Status | Resolving PRs | Blocker and what is left | tealstreet-next callers | Affects |
| --- | --- | --- | --- | --- | --- |
| B1 | **Open: owner decision 1** | none | **Engine delivery.** Copybara does not mirror `sdk-rs/**`; tealstreet-next already owns an unrelated `sdk-rs/`; `@tealstreet/safe-cex-wasm` `pkg/` is a gitignored build output. No tealstreet-next build runs a Rust/wasm step. See [Build and delivery steps](#build-and-delivery-steps). | every host below; `apps/web/next.config.mjs`, `services/exchange-server` build, `apps/cli/scripts/build.sh`, `packages/exchange-mcp/tsup.config.ts` | all 9 default-Rust names immediately; every opt-in later |
| B2 | **Decided short term: keep remote signing** (Sam, 2026-10-21). Hosted accounts keep working at cutover; the engine port is costed, undecided | #1498 (hedge), #1505 (hosted Coinbase/Orderly); costing in [RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md) (#1500) | **Keyless hosted signing.** exchange-server constructs private accounts with `secret: ''` and a live `RemoteSigner` (`services/exchange-server/src/server.ts:1015-1034`); the browser signs via `exchange.signHttpRequest`/`signWsAuth` (`apps/web/src/controllers/exchangeController/exchangeAccountController.ts:3029`, `apps/web/src/utils/shim/worker/signingHost.ts:191`). No engine path signs through a remote signer. Hosted accounts must keep working at and after cutover; server-held session keys are the likely long-term direction but are not committed. **Hedge:** `runsOnRustEngine` keeps every keyless remote-signed account (`isKeylessRemoteSigned`: a `remoteSigner`, no `secret` and no AsterDEX V3 wallet key) on the TS venue, so `createExchange` and `ExchangeWorker` never start it on an engine that cannot sign; a directly built `RustEngineExchange` refuses it at startup with `keyless remote-signed accounts run on the TS venue` instead of starting and rejecting every order (Hyperliquid did). Lighter (read-only without a secret, as TS), Paradex and Polymarket (signing key in `key`) stay on the engine. **Default-Rust names (#1505):** master's TS Coinbase and Orderly venues are restored as hosted-signing-only venues (`exchanges/hostedSigningVenues.ts`). A keyless hosted account on coinbase, coinbasespot or an Orderly broker runs them, and a browser `RustEngineExchange` signs `signHttpRequest`/`signWsAuth` through them, as master did; a main-thread mirror whose server worker is TS keeps its forwarders' capability defaults. TxFlow has no kept venue (no hosted users; its master build was a stub). Orderly hosted private WS signs with the local secret, so it stays unavailable, as on master. **Left:** the engine port itself (Sam's decision, costed in RUST_REMOTE_SIGNING.md); these TS venues are deleted when it lands or session keys replace remote signing. | `services/exchange-server/src/server.ts:972-1106`, `remote-signer.ts:513-693`; web `signingHost.ts:74-191`, `exchangeAccountController.ts:2482,3029` | hosted mode for every Rust venue |
| B3 | **Open: owner decision 3** | options/costing #1546; live-issue inventory #1556 | **React Native / Hermes.** Sync reaches the wasm imports through `registry.native.ts`, breaking Metro bundling; Hermes cannot execute wasm. The 9 default-Rust names are filtered from the mobile UI, so bundling is the immediate cutover problem. [RUST_MOBILE_B3.md](RUST_MOBILE_B3.md) recommends keeping device TS for cutover and routing through exchange-server with session keys as a direction; neither is decided. Existing transport/signing/storage app fixes remain separate owner items. See [Mobile](#mobile). | tealstreet-mobile | the synced mobile bundle; any future on-device Rust execution |
| B4 | **Resolved in the SDK; one tealstreet-next edit** | #1443 (merged `6d6e3d666f`); #1462 (merged `341ea148bb`) | **Main-thread trading and reads.** An instance with a `callMethod` listener (the web controller's passthrough in `store`-payload and frame modes, and the hosted `ExchangeServerWorker` shim) forwards `placeOrder(s)`, `placeOrderWithClientOrderId`, `updateOrder`, `cancelOrders`, `cancelSymbolOrders`, `cancelAllOrders`, `findOrderByClientOrderId`, `nuke*`, `fetchPublicTrades` (`components/chart/datafeed/DefaultDatafeed.ts:836`), `fetchOHLCV` (`components/dom/Dom.tsx:237`, `lightweightCharts/SafeCexKLinesChart.tsx:29`) and the journal axes `fetchHistoricalFills`/`fetchHistoricalOrders`/`fetchIncomeHistory(Range)`/`fetchHistoricalPositions` (`utils/historical/collectAccountAxes.ts:229-241`) whole to the worker's engine. It adopts the worker's capability record without building an engine, so Lighter `changeAccountTier`/`fetchAccountTier` (`components/settings/profile/PerExchangeSettingsBody.tsx:74-130`) appear once markets load and forward. No edit for these. Optional edits: journal ingestion (`utils/historical/runHistoricalWindow.ts:45`) should `await exchange.loadCapabilities()` before `isAxisSupported`, or for one round trip after markets load it asks unsupported axes and records empty rather than unsupported; `PerExchangeSettingsBody` does not re-render when the tier methods appear, so a panel opened before adoption shows the tier controls disabled until its next render. **Left:** warm start (`utils/exchange/eagerExchangeWarmStart.ts:124` `ensureMarkets` → `fetchMarkets` at `:58`, from `hydrateExchangeFromCache.ts:31`) starts a second private engine on the main thread; skip it for Rust accounts (edit below). Deploy exchange-server before web: an older server answers forwarded methods with an empty failure. | `eagerExchangeWarmStart.ts:116-130`; optional `runHistoricalWindow.ts:45`, `PerExchangeSettingsBody.tsx:74-130` | web, all Rust venues |
| B5 | **Resolved; no tealstreet-next edit** | #1388 (merged `f9e21a8f37`), #1450 (merged `5ff7ebde6c`) | **Socket health and recovery.** Before #1450, `store.socketHealth` was never written by frames, `reconnect`/`recoverAfterOffline` were no-ops/`true`, `getAllSockets()` returned `[]` and `authFailureStreak` stayed 0. #1450 writes `socketHealth` through the TS summarizer on every socket change and every 5 s, makes `reconnect`/`recoverAfterOffline` close open engine sockets (code 1006) so the venue reopens them (`false` only after dispose or engine death), returns per-channel handles from `getAllSockets` (`isReadonlyFallbackActive()` is always `false`), raises `authFailureStreak` on credential rejections, and teaches the CLI's `findKeyRejection` the engine's wording. Every caller listed then works unchanged; a private stream counts as healthy for 60 s of silence (TS: 20 s), and a socket already in venue backoff (up to 30 s) is not hurried. | web `exchangeAccountController.ts:3391-3415`, `utils/exchange/getWebSocketStatus.ts:27-55`, `getExchangeAppearsConnected.tsx:34`, `components/connectionStatus/{ConnectionStatusIndicator.tsx:39,97,ReconnectingBanner.tsx:157-261}`, `nudges/hostedServerForInstability.ts:66`, `ConnectionStateWorker.tsx:364`, `utils/desktop/desktopRecovery.ts:82`, `order/createLimitOrder.ts:221-222`; CLI `packages/cli-core/src/commands/BaseCommand.ts:173`, `ConnectionCommand.ts:182-196`, `ConnectionMonitor.ts:128-132`, `legacyCommands.ts:28`, `apps/cli/src/repl.ts:2831,2903`; exchange-server `public-exchange-manager.ts:95,459` | all Rust venues |
| B6 | **Resolved; no tealstreet-next edit** | #1441 (merged `ae3a1fc122`) | **In-process shared public data.** Rust accepts exchange-server's `externalPublicData: { type: 'in-process', source }` (`server.ts:1015-1034`) unchanged. `serveInProcessPublicData` (`exchanges/rust/publicFeed.ts`) speaks the shared-feed protocol over an in-memory port pair, so the account ingests natively and validates the catalog as with `RustPublicFeedProvider`. Like the TS adapter it is lenient: private startup continues after 30 s without a loaded source, and account markets the source has not listed yet are tolerated (their book/candle requests wait). It follows the server's re-attaching source across public-exchange restarts and re-announces a changed catalog. Real field mismatches still fail, so the source must run on the same engine (at cutover the public exchange is Rust too). One hub per source broadcasts changed tickers every 250 ms. Websocket gateways still reject. Optional exchange-server edit: `createSharedPublicDataSource` keys by venue only, so testnet accounts receive the mainnet source; Rust ignores it for testnet and runs its own feed, while TS prices testnet off mainnet. | `services/exchange-server/src/server.ts:1015-1034`, `shared-public-data-source.ts:50-131`, `public-exchange-manager.ts:73-293` | hosted accounts on Rust venues with `supportsPublicOnly` |
| B7 | **Resolved; no tealstreet-next edit** | #1439 (merged `0ced7ccf1e`) | **Per-account proxy token across async engine work.** Each engine HTTP transport captures the contextual token provider of the context that created the account (`captureProxyTokenProvider`) and reinstates it synchronously around proxy authorization (`withProxyTokenProvider`), so requests started from timers, sockets, shared public feeds or another account's context use the account's own live provider (`transport/engine.proxy-token-context.test.ts`). The HOW adapter and relay sockets read the token after awaits or on open, so they bind the lookup at request/socket start (`HOWConfig.bindProxyToken`), and the engine creates relays inside the account scope. Shared HOW connections are still authenticated once per connection by whichever request opens them, as with TS. | `services/exchange-server/src/server.ts:2,59,77,796-819`, `virtual-worker-context.ts:681` | hosted mode, proxied venues |
| B8 | **Resolved: listed SDK method gaps** | #1445 (merged `fc771ab13d`), #1447 (merged `8f29096de9`), #1453 (merged `fe43821c3e`), #1451 (merged `a0f4e7c53c`, superseded), #1501 (Hyperliquid spot markets), #1680 (merged `6e14b6a0ef9`), #1681 (merged `3db712f0446`), #1685 (merged `84f150c76dd`), #1686 (merged `850bc25e51e`), #1687 (merged `90df20d2f14`), #1690 (merged `2ff8452aa53`), #1693 (merged `fa960d9cfcf`) | **Missing per-venue methods.** `fetchClosedPnl` serves engine closed-position rows (`fetchClosedPositions` venue action) through the TS mappers for bybit, bybitdemo, hyrotrader, bybitinverse, the four okx names and blofin (#1445); `fetchHistoricalPositions` follows each TS declaration for bybit/bybitdemo/hyrotrader, okx (linear), blofin, bitget*, hyperliquid and paradex (#1447); Bybit's five names sync the raw REST wallet account into `balance.info` every 2 s (`fetchBalanceInfo`, #1453). All use the existing call shapes. Hyperliquid with `enableSpotMarkets` runs on Rust: the perp engine merges spot pairs (TS display alias), spot holdings (`balance.coins`) and spot orders, as the TS venue does. The listed method gaps are closed: Backpack, Crypto.com, GRVT, Paradex and WEEX REST queries now work, and Hyperliquid perpetual `scheduleCancel` arms/clears natively; spot remains unsupported. Renewal stays caller-owned. No default direct NX/CLI/MCP producer for these REST-book/schedule APIs was established by the bounded audit; MEXC standalone JSON admission matches its existing query implementation. Definitive GRVT builder failures now preserve validated code/reason/raw caps through command replies; the NX structural-handler migration below remains required. WEEX metadata refresh now uses native `onReadAndMetadata` with the shared bridge path. See [Cutover tail audit](RUST_CUTOVER_TAIL_AUDIT.md). | web `components/openPositions/ClosedPositions.tsx:93-121`, `utils/historical/collectAccountAxes.ts:229`, `ChartContainer.tsx:161`, `useChartSession.ts:303`, `Margin.tsx:85-110,413,566`, `isBybitInverseUnifiedMargin.ts:25`, `placeOrderStoreData.ts:107`, `bybit-skin/.../AccountInfo.tsx:32-45,97`, `MarginHeader.tsx:56`; MCP `packages/exchange-mcp/src/tools.ts:182` | listed SDK methods; separate caller work remains |
| B9 | **Resolved in the SDK; caller migration remains** | #1377, #1383, #1401, #1424, #1482, #1483, #1484, #1583 (merged); venue PRs merged for the listed families | **Typed errors.** `ExchangeError` (exported from `@tealstreet/safe-cex`, `kind`/`outcome`/`venueCode`/`venueMessage`/`retryAfterMs`/partial-batch fields) now rejects **command promises** on the main thread. Through the worker it arrives as a plain `Error` with the same fields (`serializeCommandError`/`deserializeCommandError`), so callers must read fields, not `instanceof`. Classified venues: apexomni, asterdex, backpack, binance, bingx, bitget, bitrue, bitunix (#1589), blofin, breakoutprop, bybit (+variants, #1479, which also fixes the invalid-options hang), coinbase (+spot, #1478), cryptocom, grvt, hyperliquid, kraken (+spot), kucoin (+spot), lighter (#1475), mexc, okx, orderly (#1471), paradex, phemex, polymarket, txflow (#1473), weex, woo. Since #1482 `start()` rejects with an `ExchangeError` carrying the startup `kind` (a credential deactivation is `authInvalid`), and `validateAccount` returns `{ error, kind }` with unchanged `error` text. #1483 tags worker Sentry reports with closed-set `error_kind`/`error_category`/`error_outcome`/`venue_code` (`exchangeErrorTags`). #1484 adds `classified: { kind?, outcome?, venueCode? }` to the `'error'` event options on the main thread and through the worker, and `ExchangeHealthMonitor.processMessage(message, isError, classified?)` prefers a specific classified kind over text patterns. **Left (tealstreet-next):** pass `opts.classified` for ordinary command/legacy errors; for coded diagnostics, adopt #1583's `engine-event`/metadata, `processEvent` and `processHealth`. Suppress a duplicate legacy diagnostic only when its matching event metadata is present. Preserve auth/reload behavior and the caller inventory below. See [Error strings](#error-strings-that-must-move-to-codes). | see [Error strings](#error-strings-that-must-move-to-codes) | all |
| B10 | **Resolved, given B1** | #1375 (merged `b6c4f006a6`) | **candle-server public exchanges never started.** `fetchMarkets` (`exchange-pool.ts:265`) now starts the engine and waits ≤30 s for native data; it still needs the engine loadable in that image (B1). See [Mobile](#mobile). | `mobile-services/candle-server/src/exchange-pool.ts:169-312` | candles for the 9 default names |
| B11 | **Open: NX history ingestion and coverage** | #1542, #1554, #1593, #1591; SDK transport/mapping #1666 (merged `bba1f43126c`) | **Partial history must not become complete coverage.** Command-owned partial arrays now retain their assertion through SDK worker/hosted JSON, SDK derived history and MCP serialization. Upgrade SDK sender and receiver together. NX must inspect completeness before transforming rows, preserve incomplete derived axes and leave coverage uncompleted. See [Incomplete history](#incomplete-history-1542-1554). | NX `ExchangeServerWorker` (matching SDK upgrade), `collectAccountAxes`, `runHistoricalWindow`; MCP consumers (flagged envelope) | affected Rust history reads and any fill/position axis derived from them |

### What remains before cutover (beyond owner decisions B1–B3)

SDK history transport/mapping is implemented in #1666 (B11): worker/hosted
replies carry request-owned completeness, derived SDK histories preserve it,
and MCP serializes known partial rows explicitly. Upgrade SDK sender and
receiver together. NX ingestion and coverage tracking remain required; raw
native/DO clients still correlate their command-owned notices. #1593 supplies
that ownership and #1591 guards direct SDK journal folds.
B2's engine port is costed in
[RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md) and undecided; hosted accounts
keep working without it.

tealstreet-next edits, shipped with the cutover (listed in [tealstreet-next caller changes shipped at cutover](#tealstreet-next-caller-changes-shipped-at-cutover)):

1. Capability record: the 10 compile-time call sites and two test fixtures (#1400).
2. Warm-start skip for Rust accounts (B4).
3. `callMethodInWorkerFireAndForget.ts:3-7` also accepts `code === 'COMMAND_TIMEOUT'`.
4. Root `CLAUDE.md:662` names `isBaseCapabilityFallback` as the brand check; point it at `exchange.capabilities`.
5. Pass typed detail for ordinary errors (#1484); migrate coded diagnostics to `processEvent`/`processHealth` (#1583), forwarding `engine-event` and suppressing duplicate legacy delivery only by event metadata. See [Coded diagnostic callers](#coded-diagnostic-caller-cutover-pr5).
6. Upgrade SDK worker sender/receiver together; inspect request-owned history
   completeness before NX collection transforms and preserve derived incomplete
   axes. Incomplete windows never call `completeWindow` (B11).
7. **Required:** forward startup event codes, show per-exchange degraded state
   independently of reconnect-toast suppression, clear it on recovery, deduplicate
   legacy delivery and attach CLI listeners before starting. See
   [Required startup degradation visibility](#required-startup-degradation-visibility).

Cutover item, `collectAccountAxes`: #1666 preserves the incomplete-history flag
through SDK forwarding/mapping, but NX ingestion/coverage work stays open (B11).
The exact transport and ingest requirements are in [Incomplete history](#incomplete-history-1542-1554).

Owner item, one-off at cutover: `historical_orders` (created by `supabase/historical_data.sql`) already holds BloFin order rows that TS wrote in contract units. Rust writes base units, so the table would mix both. The fix is to **multiply** `amount`/`filled` by the instrument's `contractValue` (base = contracts × contractValue, e.g. 1000 × 0.001 = 1 BTC). Do not select rows by `ingested_at`: an upsert keeps the original timestamp, so a TS-era row that Rust later rewrites in base units would be converted twice. Run the backfill with BloFin order ingestion paused, before Rust writes any BloFin order row, or make it idempotent: convert only rows whose `filled` far exceeds the summed base `qty` of the matching `historical_fills` (TS fills were already base units). The owner runs it; there is no SQL in this repo.

Deployment order: exchange-server before web (B4 forwarding, `fetchEngineCapabilities`, then B11 completeness metadata). #1538 already emits venue events: deploy safe-cex-server with #1388's notice validator before connecting such engines to native sessions (an older validator closes the session on `type: "event"`).

Not blockers but required edits are listed per area below.

**High-priority trading-safety caller repair:** [Bitunix disconnected books](#bitunix-disconnected-books-1639)
must clear previously displayed depth on invalidation. The current NX listener
and renderer can keep stale book levels visible after disconnect.

### Venue names on Rust

| Set | Names | Selection |
| --- | --- | --- |
| Rust by default (TS classes deleted) | `coinbase`, `coinbasespot`, `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi`, `txflow` (testnet only) | `registry.ts`, `registry.native.ts`; no opt-in, every factory/worker/server/CLI/MCP path |
| Rust adapter, TS default | `apexomni`, `asterdex`, `backpack`, `binance`, `binancepm`, `binancespot`, `binanceinverse`, `bingx`, `bitget`, `bitgetusdc`, `bitgetinverse`, `bitrue`, `blofin`, `breakoutprop`, `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader`, `cryptocom`, `grvt`, `hyperliquid`, `hyperliquidspot`, `kraken`, `krakenspot`, `kucoin`, `kucoinspot`, `lighter`, `lighter-robinhood`, `mexc`, `okx`, `okxeu`, `okxdemo`, `okxeudemo`, `paradex`, `phemex`, `polymarket`, `weex`, `woo`, `bitunix` | `ExchangeOptions.engine: 'rust'` in `createExchange`, `createExchangeFromAccount` overrides, `ExchangeWorker`; MCP `EXCHANGE_ENGINE=rust`. No tealstreet-next caller sets it today. No `'typescript'` value (Owner decision 4) |
| TS only | `binanceoptions` (master-only, survives the merge) | |

`ExchangeName` keeps every name, so the exhaustive maps in
`apps/web/src/components/chart/web/hostedMarketProduct.ts:23` and
`components/newApiFlow/getNewApiFlowTutorialUrl.ts:56` still compile. The
webpack chunk names `exchange-coinbase`, `exchange-coinbasespot`,
`exchange-<orderly broker>`, `exchange-txflow` disappear (no tealstreet-next
reference). Venue lists outside the SDK that must be reconciled by hand:
`apps/admin/server/exchangeAssets.ts:57-130` (missing `binanceinverse`,
`binanceoptions`, `hyrotrader`, `mexc`, `txflow`), `packages/exchange-pretty-error-messages/src/prettifyErrorMessage.tsx:16-22`
(`bybit*` prefix excludes `hyrotrader`), `apps/extension/src/exchanges/registry.ts` (bybit only),
`packages/exchange-signer/src/index.ts:108-128` (no consumers).

### Contract changes by caller

#### Construction, registry and engine selection

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| 9 names default to `RustEngineExchange` | web `utils/exchange/createExchangeFromAccount.ts:6,57`, `preloadAccountExchanges.ts:3`, `signingHost.ts:23,88`; exchange-server `server.ts:1059` (`storeWebworkerStart`), `public-exchange-manager.ts:80`; CLI `apps/cli/src/platform-adapter.ts:62`, `record/recorders/orderbook.ts:43`; MCP `packages/exchange-mcp/src/server.ts:23`; `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts:142`; `mobile-services/candle-server/src/exchange-pool.ts:169` | TS venue class | Rust class; needs the engine loadable in that realm (B1) and, on main threads, a worker engine (B4) | RT |
| `ExchangeOptions.engine?: 'rust'` | none set; `createExchangeFromAccount` callers above | n/a | opt-in per account; `accountToExchangeOptions` does not map an engine field, so callers pass it as an override | add |
| `start()` single-use; startup failures reject (`StartupFailed`, #1383) | MCP `server.ts:13` (start-once, retried on failure on the same instance); CLI `platform-adapter.ts:247-261` (30 s race) | TS `start()` re-callable | a failed Rust start needs a new instance; MCP retry must recreate the exchange | RT |
| `validateAccount()` starts the engine; error text from engine startup | web `components/newApiFlow/useAccountForm.ts:210-275` (matches `'passphrase'`, `'api key'`, `'secret'`, `'whitelist'`, `'permission'`, `'secondary'`) | TS venue messages | engine messages (e.g. `<name> credentials were rejected`, `Orderly requires base58-encoded 32-byte key and secret`) | RT |
| `externalPublicData` websocket gateway rejected; in-process source accepted (B6); `privateOnly`+`publicFeed: direct` rejected | exchange-server `server.ts:1015-1034` | none for in-process | `publicFeed: { mode: 'shared', port }` + `RustPublicFeedProvider` for gateways | RT, B6 |
| Keyless `remoteSigner` account routes to TS | exchange-server `server.ts:1015`; web hosted signing | browser-signed | #1498/#1505 keep hosted TS and browser signing working (B2); a direct private engine construction is refused, with the documented exceptions | none through factory/worker; direct construction RT |
| `payloadSigner` used only by Paradex | MCP `packages/exchange-mcp/src/config.ts:15-22,59-72` (hmac proxy signer) | Bybit/Binance REST | #1510/#1531: `runsOnRustEngine` keeps Bybit/Binance-family `payloadSigner` accounts (not `publicDataOnly`) on TS, whose signers honour it; every other private non-Paradex Rust account with one, including default-Rust Coinbase/Orderly (their kept TS classes ignore it), is refused at startup with `ExchangeError` `kind: 'unsupported'`, `outcome: 'notSent'`. #1575 enforces this in the registry after WASM loading, before venue I/O or signer invocation | RT (MCP `EXCHANGE_ENGINE=rust` signer mode: Bybit/Binance run TS; other venues fail loudly) |
| Resolved polling policy (#1392) | MCP `config.ts:59-72` (`extra.tickInterval: Infinity` disables polling); web `exchangeAccountController` worker-config multipliers | TS `getTickInterval()` policy | all Rust periodic reconciliation now scales from the resolved interval, including overrides, server multipliers and auth backoff. `Infinity` pauses, finite updates resume; native rate floors/cooldowns remain. Startup, socket supervision and explicit reads continue | none; existing controls now apply to Rust |
| `instanceof BlofinExchange` | web `components/reactiveStateHelper/exchangeReactiveStateHelper/BlofinExchangeReactiveStateHelper.tsx:6,20` | true for TS BloFin | false for Rust, but pinned NX audit finds no production import/render of this helper; active containers use the generic helper | unused-source/test cleanup before class deletion; not a demonstrated live Rust UI failure |
| `RUST_VENUES` becomes `ReadonlySet` (#1400) | none | | | none |
| `Exchange` gains `fetchEngineSnapshot`; #1400 adds `capabilities`/`loadCapabilities` | no `implements Exchange` downstream; only `as Exchange` casts (`apps/cli/src/repl.ts:713,1517-1925`, `cross-trade-daemon.ts:222`, web tests) | | casts still compile | none |
| #1476/#1519/#1520 construction and clock policy | factory/worker creation and MCP config callers above | TS local-credential check, per-venue options/clock name lists | shared Rust validation and uniform options; #1709 supersedes engine `hostClock` with one synchronized SDK clock ingress; no caller edit. Missing secondary-secret errors are now specific; #1531 still narrows payload-signer fallback | RT validation text; same signing clock |
| Construction metadata/public mode (#1563/#1567) | saved-account/factory/worker callers above; web `components/customModules/monacoTypesString.ts` already declares the Aster override and `publicDataOnly` | synchronous venue branches and derived public-mode hint | shared rules preserve synchronous getters; after WASM loading they adopt native `canSign`/`publicDataOnly` when present, while Aster STP reads native `asterdexApiVersion`. `WasmEngineLike.construction()` remains optional; missing fields use TS fallback. See the input/compatibility boundaries below | none for application callers; additive host API |

#### Construction metadata and public mode (#1563, #1567)

`packages/safe-cex/src/exchanges/rust/construction-rules.json` is now the single
rules table for signing and private-data eligibility. TS `derivePublicMode`
evaluates it synchronously before WASM loads. Since #1599, the Rust registry's
`build.rs` reads and validates the same JSON during Cargo builds and generates
a static table in `OUT_DIR`; the engine evaluates that table before credential
validation/startup, without runtime JSON decoding. Source delivery/builds (B1)
must retain the JSON at `packages/safe-cex/src/exchanges/rust/construction-rules.json`
relative to the repository root; compiled engines contain the generated table
and do not need the JSON at runtime.
The compiled parity matrix covers every registered Rust name across 69 input
combinations, including signer kinds, public intent and venue-specific rules.
This preserves existing options and eligibility; it does not add remote signing
to an engine or establish that credentials are valid.

| Host boundary | Landed contract and compatibility |
| --- | --- |
| SDK bridge → native `EngineOptions` | Optional `publicMode: { requested: boolean \| null, remoteSigner: boolean, payloadSigner: boolean, apiKeyIndexPresent: boolean }` carries raw intent/presence beside the existing `publicDataOnly` hint. The SDK sends it; application callers keep using `ExchangeOptions.publicDataOnly`, credentials and signer options. With `publicMode`, Rust derives public mode and sets `accountStartup` from it rather than trusting a conflicting derived hint. |
| `WasmEngineLike.construction?(): string` | JSON `EngineConstruction` now has optional `canSign`, `publicDataOnly` and `asterdexApiVersion` fields. The wrapper adopts available native fields after loading and uses shared synchronous TS rules for missing fields/older hosts. This metadata is separate from frame/engine capabilities and contains no credentials. Without `publicMode`, native/harness construction keeps the legacy `publicDataOnly`/`accountStartup` interpretation and omits the two new metadata fields; non-Aster metadata can still be `{}`. |
| WASM module `resolveConstruction(venue, optionsJson): string` | New named export returns the same metadata JSON without constructing an engine or performing I/O. `WasmEngineModule` declares it optional, so existing custom loaders need no new method. Resolving metadata is not credential validation or successful startup; the production wrapper still has synchronous TS getters before async loading. |
| Explicit-private Paradex startup | `publicDataOnly: false` without key, Paradex address or subkey address still rejects `start()` with `Rust Paradex private accounts require a key, Paradex address and subkey address`. The registry now enforces it after WASM loading and before venue I/O, instead of the bridge refusing before loading. Public/automatic mode and the existing private-field requirements stay as before. |

#### Host construction rules (#1575)

The construction scope in [RUST_VENUE_CONSTRUCTION.md](RUST_VENUE_CONSTRUCTION.md)
is now landed through #1563/#1567/#1575. B2's factory routing for keyless remote
signing and supported payload signers still applies; the engine signing port
remains an owner item. Paradex signing transport and polling are unchanged.

| Host boundary | Landed contract and compatibility |
| --- | --- |
| SDK bridge → HyroTrader registry | Optional `EngineOptions.hyrotrader` carries raw `mode` plus `production`/`simulator` maps, each with `rest`, `publicWs`, `privateWs` and their `livenet`/`testnet` URLs. Rust selects simulator for `mode: 'demo'` or an empty production REST livenet URL (even for testnet accounts), then selects the requested network. All three selected URLs must be configured or construction rejects with `HyroTrader REST/public/private URLs are not configured`. Raw config takes precedence over explicit `endpoints`; omitting it preserves legacy native/harness explicit endpoints. Application callers keep `extra.hyrotraderMode` and `testnet`; this is no new live-connectivity evidence. |
| Native private `payloadSigner` refusal | `payloadSigner` is parsed across features. After public-mode derivation, private non-Paradex construction refuses it as `unsupported`/`notSent`, including with a local secret, before HTTP/WS I/O or a signing callback. Public accounts bypass the refusal, including the existing Aster/Lighter payload-only public fallback. Paradex delegated signing and the Bybit/Binance TS factory exceptions remain. |
| Raw WASM constructor → SDK startup | Registry construction failures throw a JS `Error` with JSON-encoded `venueError` detail. The SDK wraps valid detail in `ExchangeError`, preserving message/classification; `validateAccount()` still reports `kind: 'unsupported'` for the payload-signer refusal. Errors without usable detail retain their original form for older/custom loaders; invalid options JSON remains a plain constructor error. No constructor arguments or application call shapes change. |

#### Host entropy and NX construction (#1624)

Core `Engine::new(venue, Option<Entropy>)` now returns a `Result` and refuses
`None` with typed `invalidInput`; registry `build` also takes that provider.
Native `production_entropy()` supplies independent OS-backed identity/signing
sources, including `FrameServer` construction. Custom Rust hosts must pass a
provider at construction; native `with_entropy` / `with_signing_entropy` and
engine setters can replace their respective sources only before startup.
Direct Rust signing-helper callers also supply entropy explicitly: Lighter
`Signer` signing/auth methods, Coinbase `Credentials::jwt`/`headers`, and
Bitunix `sign::headers`. SDK venue adapters already pass signing entropy.

JS `EngineHost` keeps its existing constructor and adds optional fourth-options
`entropy?: (length: number) => Uint8Array` and independent `signingEntropy?` with
the same signature. Results must have exactly the requested byte count and are
copied; malformed or failing callbacks produce typed `invalidInput`. Explicit
overrides require the matching raw `setEntropy` / `setSigningEntropy` method
and are installed before start; these methods remain optional on
`WasmEngineLike`. Identity covers IDs, jitter and host receipt sessions; changing
identity entropy never seeds signing. Signing retains its engine cryptographic
source unless explicitly overridden, so deterministic signing fixtures must
supply their own separate override.

Production JS host identity defaults to `globalThis.crypto.getRandomValues`.
Raw WASM construction independently binds WebCrypto for both engine sources,
lazily preferring `globalThis.crypto` before `self`/`window` and accepting only
callable `getRandomValues`. An empty or throwing shim does not mask a usable
higher-priority provider. Missing usable constructor-time WebCrypto fails with
typed `invalidInput`; a later EngineHost override cannot repair an already
failed raw WASM constructor. There is no weak-random fallback. The SDK's
`RustEngineExchange` still constructs the raw engine then `EngineHost` with
only `commandTimeoutMs` in its fourth options (`rust.exchange.ts:863-886`).
Application/account option shapes therefore remain unchanged.

**Required cutover verification:** confirm secure entropy in every engine-owning
NX realm and its shipped bundle. These roots use the SDK's secure defaults;
this source audit does not require redundant app constructor arguments. Keep
provider functions local to the owning host, outside account JSON and
worker/hosted messages. Public-only engines also need identity/jitter entropy.

| NX runtime / read-only evidence at `65e086a3c90` | Required integration proof |
| --- | --- |
| Dedicated web worker: `apps/web/src/utils/shim/worker/workerFactories.ts:113-115,440-451` → `StoreWorkerEntry.ts:13-25` → SDK store worker/ExchangeWorker → Rust host. WorkerShim fallback invokes the same entry in the window realm (`WorkerShim.ts:13-32`). | Verify WebCrypto in the dedicated worker and window fallback separately. The synthetic `workerSelf` only supplies messaging; adding entropy to that object would not configure the host. |
| Hosted private/account VWC: `services/exchange-server/src/server.ts:1000-1006,1056-1069` creates an instance and passes it to `storeWebworkerStart`; its constructor (`virtual-worker-context.ts:117-125`) stores instance fields. Separate shared public engine: `public-exchange-manager.ts:74-89` / `public-virtual-worker-context.ts:44-46`. | Verify Bun's actual server realm supplies WebCrypto before both private and public-engine construction, including restarts. A provider attached only to private-account setup would miss the public engine. |
| Standalone CLI: `apps/cli/src/platform-adapter.ts:59-63,238-251` uses the SDK factory/start path; `scripts/build.sh:69-79` compiles Bun. `bin/tealstreet.cjs:1-8` only launches that binary. | Verify WebCrypto in the compiled Bun binary, which owns the engine, as well as any supported development runtime; the Node launcher is not the engine realm. |

No production global `window`/`self` shim was found in the hosted/CLI roots or
relevant workspace source inspected. Hosted VWC objects do not define those
globals; CLI's first import only replaces `globalThis.WebSocket`
(`apps/cli/src/index.ts:1-4`, `ws-shim.ts:50,69-76`). The empty `window = {}`
setup is test-only (`packages/safe-cex/config/test/setupVitest.ts:4-8`, selected
by `vitest.config.ts:17`); no production-root import was found. This is static
source evidence, not a deployed-binary assertion. #1624's actual-WASM shim
regressions and SDK/Workers/packaged-loader CI passed, providing SDK-side proof;
they do not replace the NX shipped-runtime checks above or B1's delivery choice.

#### Capability detection

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| #1400 (merged) un-exports `isBaseCapabilityFallback`, `isInertCapability`, `isEmptyStubCapability` | web `components/openPositions/ClosedPositions.tsx:11,114`, `utils/historical/collectAccountAxes.ts:5,162,164`; `packages/venue-data/src/journal/symbolScopedFills.ts:1,74-76` (+ fixtures `symbolScopedFills.test.ts:94-99`, `symbolScopedFills.bybit.test.ts:35,47`); `packages/venue-data/.../axis-grading.registry.test.ts:9,89`; `scripts/history-audit/lib/axis-inventory.ts:3,72,74` (call `:128`), `closedPnl.ts:32,234`, `venue-survey.ts:4,86,107,165` | `isBaseCapabilityFallback(exchange.fetchX, 'fetchX')` | `exchange.capabilities.{fillHistory,orderHistory,incomeHistory,incomeRange,closedPnl,…}`; on never-started instances `await exchange.loadCapabilities()` first. Rust mirrors adopt the worker's record from the snapshot frame's `CAPABILITIES` section (#1411) | CT |
| MCP capability gating | `packages/exchange-mcp/src/tools.ts:2,15,363` | `!isInertCapability(fn)` | already rewritten in the mirrored package by #1400; arrives through Copybara, not a hand edit | none |
| Coinbase/Orderly fills and order history now served by the engine | web `collectAccountAxes.ts:162-241`, MCP `tools.ts:168,175` | branded empty (TS) | real data; Orderly/Coinbase/TxFlow history rejects `orderId`/`status` filters | RT (additive) |

#### Methods and properties

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| `fetchClosedPnl` | web `ClosedPositions.tsx:93-121`; MCP `tools.ts:182` | TS (blofin, bybit, bybitinverse, okx, bitunix) | engines with the `closedPositions` capability (bybit, bybitdemo, hyrotrader, bybitinverse, okx names, blofin) serve `fetchClosedPositions` rows, mapped by the TS mappers; others report `capabilities.closedPnl` false, as TS (Rust bitunix reports it false; its closed PnL stays on the TS venue) | none (handled) |
| `fetchHistoricalPositions` | web `collectAccountAxes.ts:229` (`!` asserted) | TS bitget/blofin/bybit/hyperliquid/okx/paradex | as TS: bybit/bybitdemo/hyrotrader fold reported rows with the order-axis leg join, okx (linear) and blofin map reported rows, bitget* fold account-wide fills, hyperliquid/paradex fold per-symbol fills; venues TS leaves undeclared (e.g. bybitinverse) keep the base refusal | none (handled) |
| Incomplete income results (#1542/#1554) | web `collectAccountAxes.ts` income symbol enumeration, `runHistoricalWindow.ts` coverage | successful arrays treated as complete | inspect/preserve `incomplete: true` before enumeration, folding or spreading, and leave affected coverage uncompleted (B11) | RT, B11 |
| Binance native stop-limit `updateOrder` (#1551) | web order amendment; SDK chaser amend path | re-place could change the order into stop-market | reject as `unsupported`/`notSent` before any write; do not hide the refusal or retry with a different order type | RT |
| WEEX `updateSymbolMetadata` refreshes cached/flat symbols | web via `callMethodInWorker` `hooks/margin/usePositionMode.ts:62`, `useMarginType.ts:62`, `useLeverageSlider.ts:235`, `hooks/market/useSymbolMetadataUpdate.ts:14` | Rust silently skipped the hook despite working explicit reads | native `onReadAndMetadata` requests fresh signed symbol configuration through the existing SDK path; malformed data rejects without replacing good rails; public-only skips private reads | no argument change; preserve ordinary hook failure handling |
| `recoverAfterOffline` closes open engine sockets for the venue to reconnect; `false` only after disposal or engine death | web `ConnectionStateWorker.tsx:364`, `utils/desktop/desktopRecovery.ts:82` (reload on `false`) | per-socket recovery | per-channel recovery; dead engines reload | none (B5 fixed) |
| `getAllSockets`/`publicWebsocket`/`privateWebsocket`/`businessWebsocket` are per-channel engine socket handles; `authFailureStreak` counts `Deactivate` and `auth.rejected` | CLI `ConnectionCommand.ts:182-196`, `ConnectionMonitor.ts:128-132`, `legacyCommands.ts:28`, `repl.ts:2831,2903`; web `order/createLimitOrder.ts:221-222` (`privateWebsocket.isReadonlyFallbackActive()` is `false`) | TS sockets | handles expose `lastReceived`, health, `reconnect`, `recoverAfterOffline`; no `ws`, no per-topic stats | none (B5 fixed) |
| `fetchMarketsAndTickers` (not on base), `fetchTickers`, `fetchPositions` via `as any` | web `exchangeAccountController.ts:668-669,687,750,863`, `components/customModules/frame/feedSources.ts:82-83`, `eagerExchangeWarmStart.ts:52,58` | REST reads | `fetchMarketsAndTickers` undefined on Rust; `fetchMarkets`/`fetchTickers` start the engine and wait ≤30 s for native data, else throw `Timed out waiting for native markets|tickers` (#1375). #1430 makes `fetchPositions` wait for its native sections and synchronize engine state; see below | RT |
| Native account reads (#1430): `fetchOrders`, `fetchPositions`, `fetchBalance`, `fetchPositionMode` | MCP `tools.ts:62,82-83,144,152,160,322`; web warm-start `eagerExchangeWarmStart.ts:75,86` | cached JS store or venue REST | await native section readiness and synchronize current engine state; main-thread mirrors forward to their worker. No forced network refresh. Missing public-only private sections reject visibly | RT; same arguments/results |
| Native identity `getAccount` (#1430) | no application exchange-identity calls found in web/CLI/MCP/server at `fdd2389a825` (CLI adapter `getAccount` is a different API) | limited native lookup support | query/after-load behavior follows `capabilities.account`; unsupported venues retain an explicit refusal. See identity boundary below | additive; no app edit found |
| `fetchLeverageForSymbol` (#1522) | mirrored worker `UiStateHandler.ts:27-64`; web `hooks/margin/useMarginType.ts:27`, `usePositionMode.ts:27` consume `store.fetchedLeverage` | BingX/WEEX settings-only reads and uncached metadata rails could return `null`; WEEX native command had no handler | BingX/WEEX read fresh symbol rails; MEXC/WOO/Kraken `onMetadata` reads query only when no usable cached rails exist. UI still fetches only for a missing active-symbol entry. After capability readiness, read failures keep the SDK warning/`null` result contract; #1637 readiness errors reject before that fallback | RT for pre-start/failed-start handling; same ready read shape |
| Native `nuke`/`nukePositions` (#1430) | web `hooks/trading/useNuke.ts:7`; mirrored CLI `commands/trading/NukeCommand.ts:32,42,51,60` | inherited store-based closes/retries | one native-state batch of reduce-only market closes; no automatic replay even if callers pass `3`. Observe/catch the promise and reconcile partial/unknown outcomes; CLI's max-retries text must not imply a guarantee | RT; caller error/retry handling |
| `changeAccountTier`, `fetchAccountTier` (#1386/#1394) | web `components/settings/profile/PerExchangeSettingsBody.tsx:46-62,74-130` | TS LighterExchange | Rust provides both; `balance.info.lighterAccountLimits = { user_tier_name }` synced; the main-thread mirror forwards them (#1462) | none (B4) |
| `fetchPublicTrades` | web `components/chart/datafeed/DefaultDatafeed.ts:831-836` (binance only) | TS | engine command; main-thread mirror forwards it (#1462) | RT (B4) |
| Live fill identity/financials (#1430/#1592) | web `exchangeAccountController.ts:2315-2355`, `fillOrderToast.tsx`, `FillsModule.tsx:141`, `HistoricalIngestionWorker.tsx:297`, `ChartTradingController.ts:239-245`; CLI `repl.ts:941-949`; script-editor `monacoTypesString.ts:614-618` | Rust IDs identified the order; #1430 added optional financials | `{ id, orderId, symbol, side?, price, amount, realizedPnl?, fee? }`; only reported finite values appear, including zero. Since #1592, `id` is the reported execution identity or falls back to `orderId`; `orderId` remains the order identity. Existing script-editor types already declare the optional fields | RT for order-ID assumptions; matching engine/reader/applier required |
| `options.enableSpotMarkets` on hyperliquid | web `ChartContainer.tsx:161`, `useChartSession.ts:303` | spot markets | the Rust perp engine merges spot markets, holdings and orders into the account, as the TS venue; best-effort spot discovery retries on the discovery backoff without holding back perps | none (handled) |
| `store.balance.info` raw venue payload | web `Margin.tsx:85-110,136,358,413,566`, `isBybitInverseUnifiedMargin.ts:25`, `placeOrderStoreData.ts:107`, `bybit-skin/.../AccountInfo.tsx:32-45,97`, `MarginHeader.tsx:56` | Bybit `accountType`, multi-collateral array, Lighter limits | Lighter tier bridged; Bybit (all five names) syncs the engine's last REST wallet account verbatim (`balanceInfo` capability, `fetchBalanceInfo` every 2 s), replacing `info` as TS does; frame-forwarding workers send `info` changes to main | none (handled) |
| `Market.info` raw payload | cli-core `order/prepareCliOrder.ts:44`, `commands/session/CheckCommand.ts:14` (`classifyMarket`); MCP strips `info` (`tools.ts:113`) | raw venue JSON | typed exceptions only (Bybit class/region/underlying, Polymarket, OKX) | RT |
| `Market.info.hlTokenName`/`hlTokenFullName` (Hyperliquid spot) | web `components/margin/Margin.tsx:783-789` (Underlying Token risk disclosure) | TS merged spot markets | typed `UNDERLYING_TOKENS` frame section (token name, full name) applied as the same `info` fields, so the disclosure renders unchanged | none (handled) |
| `store` writes from the app | web `eagerExchangeWarmStart.ts:20-107`, `exchangeAccountController.ts:691-884,2573`, `feedSources.ts:98`; `packages/safe-cex-server/src/publicExchange/hydrateStore.ts:6-13`; candle-server `exchange-pool.ts` | TS store | frames overwrite markets/tickers/orders/positions/balance on the next engine frame; app writes are transient | RT |

#### Native account reads, identities and fills (#1430)

| Boundary | Landed behavior and caller requirement |
| --- | --- |
| Current account state | `fetchOrders`/`fetchPositions`/`fetchBalance`/`fetchPositionMode` start or reuse the engine, wait up to 30 s for markets plus the relevant private section, then call `syncAccountState()`. Missing private sections on public-only accounts reject with `Native <label> is unavailable for a public-data-only account`; timeout/startup/disposal failures also reject. Reads use current native state, without forcing HTTP reconciliation. Mirrors forward these async reads to the worker; their synchronous `syncAccountState()` is a no-op. B4's warm-start edit remains necessary because `fetchMarkets` still starts a local engine. |
| Decisions and freshness | `syncAccountState(): void` is additive on `Exchange` (Base is a no-op); in-process Rust requires a started engine and ready native market catalog. CLI conditions/substitution/loops and chaser creation synchronize before store-based decisions. Generic cancel-all selects native open-order symbols; set-all leverage uses native positions and hedge-leg sides. The zero-length `ACCOUNT_READ` section 21 marks private snapshots so both consumers update rows without advancing orders/positions freshness IDs. Reads preserve pending streaming changes, fills and public baselines; they do not establish a fresh venue observation. Ship matching engine, reader and applier. |
| Account identity | `getAccount()` uses native `fetchAccount` for `capabilities.account: 'query'`; `'afterLoad'` still waits for startup, and `'unsupported'` refuses. #1430 expands query support across supported aliases of Backpack, Binance USD-M/PM, BingX, Bitget, Bybit (excluding HyroTrader), Crypto.com, GRVT, KuCoin, Lighter/Robinhood, MEXC, Paradex, Phemex, WEEX and WOO. Lighter lookup can be unsigned; MEXC/WEEX validate selected read credentials and namespace identity from the primary key. These identities do not refresh financial state or establish new signing support. |
| Bulk close and cancel | `nukePositions` reads native positions, sizes closes in venue order units (inverse notional where required), preserves hedge legs and submits valid reduce-only market closes in one `placeOrders` batch. Unusable rows are skipped; if the valid-close batch resolves, skipped rows cause rejection, while a batch failure propagates its own error. `nuke` also attempts cancel-all after close failure, then rethrows the original close failure. Concurrent nukes reject; legacy `tries` arguments do not trigger retries. Web `useNuke` must handle the returned rejection; CLI prints progress and catches warnings asynchronously, so its immediate success is not completion evidence. Partial/unknown outcomes require reconciliation before any retry. |

#1522's leverage reads use native signed symbol queries: BingX maps long/short
rails to `buyLeverage`/`sellLeverage`; WEEX returns rails plus `marginType` from
the symbol configuration. BingX fences stale store publication against
configuration/settings changes and newer same-symbol queries. Superseded reads
and reads issued during a pending settings write still return current cached
buy/sell rails, falling back independently to validated read values for a
missing rail; overlap alone does not fail the read or cause a `null` result.
WEEX fences store publication against settings writes. Native read failures
still leave settings intact. `onMetadata` serves finite positive cached buy/sell rails, otherwise
issues native `fetchLeverage` (Kraken spot still returns `null`); `settings` stays cache-only. Ship matching engine and
capability reader so BingX/WEEX retain both account lookup and on-read leverage.

#### Fill identity and wire compatibility (#1592)

#1430 introduced optional financial tails under revision 1. #1592 now always
writes **FILLS revision 2**: the existing order/market/side/price/amount prefix
and raw-symbol refs are followed by execution-ID refs (`NONE` when absent),
then aligned f64 `realizedPnl[n]` and `fee[n]` columns (`NaN` when unreported).
The updated reader accepts revisions 0/1, including revision-1 frames without
financials and with #1430's financial tails; revision 2 is accepted only for
FILLS. Missing/nonfinite financial values become absent SDK fields; reported
zeroes survive.

`FillRows.fillId?(i)` exposes the optional native execution identity. The SDK
emits `id = fillId ?? orderId`, retaining `orderId` separately. Hyperliquid perp,
merged spot and spot-only fills populate `id` with the decimal `tid` string;
other native venues leave execution identity absent and retain order-ID fallback.
Use `orderId` for order attribution; do not assume every venue's `id` identifies
a unique execution. Live fills remain events rather than persisted store rows.

**Build compatibility prerequisite:** old readers reject FILLS revision 2.
Ship matching Rust/WASM producers, JS readers and SDK appliers, including both
worker and main-thread consumers when forwarding frames. A new reader can read
legacy producers; updating only the producer breaks older consumers. This
requirement is separate from B1's choice of engine delivery mechanism.

Focused NX checks at `65e086a3c90`: web optimistic-order retirement and CLI
`recordFill` already prefer `orderId ?? id`; fill toast coalescing also prefers
`orderId`. FillsModule uses live fills only as a symbol-based database refresh
hint, and HistoricalIngestionWorker discards the payload and marks the account
dirty. No edits to those handlers are required for the new identity. Script
consumers assuming `id === orderId` must instead use the separate `orderId`.

Replay now captures native `executionId` separately from order ID and captures
finite fee. #1592's green Hyperliquid/HyperliquidSpot CI replay gates prove the
four removed identity-only exception groups (37 paths); unrelated declarations
remain. This does not claim identity parity for every venue. See the
[fill execution identity plan](RUST_FILL_EXECUTION_ID.md).

Live financial conventions remain venue-specific: Hyperliquid close fills use
net PnL (`closedPnl - fee`, requiring both values) and the reported fee; opening
fills omit both. MEXC/WEEX preserve reported realized PnL and absolute fees.
Synthetic/cumulative-order fills invent no financial values. Historical
execution PnL retains its separate gross convention; this does not resolve
B11 completeness forwarding/coverage or owner-run backfills.
Order mapping also retains generic `Order.triggerType` alongside the existing
TP/SL-specific trigger fields; no caller signature changes.

#### Private snapshot row isolation (#1524)

The third snapshot batch covers Backpack, Bitrue, BloFin, Crypto.com, GRVT,
Kraken futures/spot, KuCoin futures/spot, MEXC, OKX and WOO. Migrated private
list reads accept valid peers when another row cannot be mapped. A rejection
with a recoverable identity retains that identity's stored copy; a rejection
without an identity prevents absence-based removal of stored rows in that
snapshot's scope. Accepted rows can still replace their matching stored rows.
A fully valid empty snapshot still clears its scope. Structural failures,
catalogue safety holds and existing revision/terminal fences remain in effect;
this is not permission to commit every malformed response. KuCoin futures
still discards the whole order snapshot for a missing-ID order row; its
missing-ID position rows instead protect stored positions.

Callers can observe a mixture of newly mapped and retained older rows. A store
update or resolved account read is not proof that every row was freshly decoded.
Do not infer a close/cancel or zero balance merely from a skipped row. Balance
aggregates keep venue-specific safety rules: KuCoin spot, for example, retains
its previous USD wallet total when rejected currency rows would mix old and new
values. No new per-row freshness flag, method argument or frame revision is
introduced by #1524.

Bitrue malformed order trees protect named children as well as the parent;
when a child lacks an ID, cached children of that parent are protected too.
Kraken futures/spot isolate malformed open-order rows instead of rejecting valid
peers. Kraken spot also skips a malformed execution update alone, without
emitting a fill for it or changing its order.

`ROW_SKIPPED` / `SNAPSHOT_DISCARDED` are the existing wire codes `row.skipped` /
`snapshot.discarded`, delivered through the coded diagnostic/health paths above.
A row skip does not mean the whole snapshot was discarded, and a discard is
not evidence that the account is empty. Repeated skip diagnostics may be
suppressed until recovery; emitted levels vary by venue/path (several use Warn
when retaining a stored copy and Error when none can be kept). Use code and
level, not message matching or an assumed one-event-per-row count. The B9
caller migration and snapshot collection requirements remain open.

#### Bitget ordinary private WS row admission (#1625)

Classic/UTA order and position streams now decode ordinary rows independently:
healthy peers progress while rejected cached order IDs, parent TP/SL protection
or position identities remain visible. Unidentified position rows hold absence
removal; partial/mixed position evidence cannot flip the account mode against
retained exposure. Later authoritative clean snapshots can still clear their
scope. Callers must not infer cancellation, a closed position or freshness for
every row from one store update.

Rejected observations protect pending REST joins without becoming a fresh full
financial snapshot, so repeated malformed traffic does not starve unrelated
fresh REST rows. Held positions do not republish stale settings; retained plan
and trailing metadata keep the existing revision/opened/terminal fences.
All-rejected initial traffic does not manufacture loaded-empty readiness or
renew useful financial traffic. Existing timers coalesce read-only repair and
Warn `row.skipped` uses the coded event path. Execution/fill-channel admission
and accounting remain unchanged; this adds no write retry or caller API.

#### Bybit private-row admission (#1620)

Ordinary Bybit REST order/position snapshots and WS order/position arrays now
admit healthy peers when another row cannot decode or validate. A rejected
native order protects its cached parent, attached TP/SL family and
position-bound metadata. An admitted parent still owns its complete family,
including removal of old brackets. A rejected position protects its symbol's
cached legs and Greeks; retaining those rows does not republish cached leverage
or mode as newly confirmed settings.

Unknown identities protect absence-based removal within the affected category
while valid peers progress. Ambiguous same-symbol mode changes retain coherent
exposure, and bounded rejection coverage holds the scope conservatively on
exhaustion. A valid option update/close still updates/removes its Greeks without
duplicating retained rows. Later authoritative reads can remove old exposure.

Rejected WS evidence also protects affected identities in older pending REST
joins; it does not globally invalidate unrelated healthy peers. Order repair
uses the existing bounded read cadence and positions use account polling.
These paths emit Warn `row.skipped` through the existing event channel; callers
may see retained older exposure alongside fresh peers and must not infer fresh
confirmation for every row. Structural/envelope/paging failures never prove
empty exposure. Strict Hyro handshake/account evidence, execution/wallet paths
and history completeness remain separate. Leverage-ACK read fencing is the
separate change below; caller APIs and write retry policy are unchanged.

#### Bybit leverage acknowledgement versus older position reads (#1621)

For ordinary Bybit accounts, an accepted leverage write (including native
`110043`, leverage not modified) advances the positions revision before
publishing the selected leverage to settings and cached positions. An older
in-flight position join cannot restore stale leverage/settings; if a join is
active, one fresh read is coalesced after it settles. This is a read-publication
fence, not a write retry. A refused leverage write leaves its authoritative
position read eligible. Hyro retains its existing atomic write/snapshot fence;
position-mode switch semantics and caller method shapes stay unchanged.

#### Rejected-row diagnostics and Orderly defaults (#1603, #1606)

#1603 changes severity in the migrated Binance, Coinbase and WEEX snapshot
order-rejection paths and Kraken Futures orders (Kraken spot unchanged), plus
Coinbase spot holdings.
A skip that retains an eligible stored copy warns; a skip that retains none
errors. Identity/symbol matching and terminal fences still decide whether a
stored copy can actually be kept. This changes diagnostic severity, not the
snapshot's retention policy, caller method shape or wire revision.

Their bounded severity-aware identity caches suppress repeat reports. Within
that cache window, a previously warned identity may escalate to Error once if
its retained copy is lost; a mapped row or clean snapshot re-arms reporting.
These events are not an exact count of rejected venue rows. Error-level
`row.skipped` means missing/unrepresentable data, not an `auth.rejected` event;
use the existing code-aware B9 integration and metadata-based duplicate handling.
This severity policy is scoped to the migrated paths, not every venue skip.

#1606 applies row isolation to Orderly's **default margin-mode cache**: a
rejected row with a recoverable symbol retains that symbol's old default;
an unidentified row (including an unmappable non-PERP symbol) protects the
old defaults scope, while valid peers can
still replace their entries. Valid empty lists clear the cache. Broker suffixes,
empty mode meaning Cross and accepted duplicate last-wins behavior remain.
Malformed/failed envelopes and settings-revision races still prevent replacement
of the whole defaults map. This path reports fixed Warn-level `row.skipped`
notices, including when no old default exists; it does not adopt #1603's
Error-without-retention policy. Callers keep their APIs and can see valid peer
margin defaults refresh while retaining the previous value of a symbol whose
row was rejected.

#### Bybit discovery cooldowns (#1608)

Markets, tickers and option-seed discovery honor HTTP-200 `retCode: 10006`
with or without a result list. A finite future `X-Bapi-Limit-Reset-Timestamp`
(epoch milliseconds) and `Retry-After` extend the cooldown using the later
usable hint; no usable hint falls back to 10 s. Hints are bounded by the shared
1 h cap, HTTP403 retains its 10 min floor, and a shorter later hint cannot
shorten an existing window. A reset header on success or an unrelated business
error does not create this pause.

The shared discovery deadline extends already-waiting category retries without
increasing their attempt count or creating retries for idle categories. Callers
can observe a longer wait for discovery readiness; existing host readiness
limits still apply. Optional-product degradation, other stream errors, private
reads and writes retain their handling. This adds no caller argument or new
transport-wide/write retry gate.

#### Credential feedback and saved options (#1487)

`EngineHandlers.onAccountAuth?` receives a typed `AccountAuthResult` with
`credential: 'primary' | 'secondary'`, scope `account | balance | orders |
positions | settings`, and outcome `accepted | rejected`. It carries no command
ID or credential material. Native classifiers currently provide this evidence
for BingX, MEXC, WEEX and, since #1662, Bitunix account/settings GETs; this
does not establish equivalent coverage for every
venue whose command errors are classified. Transport failures, public reads
and text matching are not substitutes for this typed evidence.

The Rust SDK consumes it automatically: ten credential/scope streaks remain
isolated, Accepted clears only its context, and their maximum drives Base's
existing capped polling backoff. Manual `recordAuthFailure`/`recordAuthSuccess`
address primary/account only, so a success there cannot clear secondary/orders.
This is separate from the diagnostic `engine-event` channel and B9's caller
health/toast migration. No direct NX accountAuth/auth-streak hooks were found
in web/CLI/exchange-server at `65e086a3c90`; custom hosts must consume the typed
channel if they own credential policy.

Native clients offer `safe-cex-frame.account-auth` alongside v1; the negotiated
protocol stays `safe-cex-frame.v1`, and only opted-in peers receive accountAuth.
`NativeFrameSession` offers it and validates optional `onAccountAuth` delivery
before the raw `onNotice` callback. DO clients opt in with `{ op: 'accountAuth' }`.
Evidence is account-scoped, not owned by a pending command; older clients are
not sent the new notice. Matching WASM/JS hosts use the additive action/callback.
This does not replace PR6's native coded-event deployment prerequisite.

#1611 separates credential acceptance from row quality. For already eligible,
tagged signed list reads, a typed success response (BingX/MEXC envelope; WEEX
root array) containing its valid required list (including an empty list) proves
Accepted even if rows
are skipped, uncatalogued or held. Missing, null, wrongly shaped or truncated
required envelope/list data supplies no outcome before grouped auth state can
change; typed credential rejection still takes precedence. A malformed success
cannot clear a rejected grouped endpoint, and a sibling's success cannot clear
an endpoint that still rejects. Non-list mode/metadata reads retain their typed
validation; existing tagging, resend and cycle fences remain.

Row rejection remains a data diagnostic, never credential Rejected. Financial
reducers keep their existing policy: a BingX bad balance list can retain the old
snapshot while reporting Accepted; MEXC/WEEX can commit valid peers and report
`row.skipped` while accepting credentials. Auth acceptance therefore does not
prove snapshot freshness or completeness. No new venue classifier, caller shape
or explicit network resync is introduced.

For Rust `coinbase`/`coinbasespot`, any defined saved secondary credential field
(including empty/partial fields) causes the secondary credential fields and
`secondaryProxyUrl` to be removed from an effective options copy. Private JWT
reads use primary credentials. Nonempty ignored read fields produce one warning
before host creation; empty fields are ignored silently, and a proxy-only option
without defined read fields keeps its policy. The caller's options object and
saved configuration are not rewritten. Factory routing and TS venues retain
their behavior; no application option shape changes.

#### Bybit rejected-key environment hint (#1590)

For built-in live linear/demo Bybit, a startup fee-read `10003` can trigger one
signed `/v5/user/query-api` GET on the opposite environment using the same
rejected credential, including a secondary read key. A 2 s bound and shared
probe prevent concurrent fee failures or late replies from duplicating the
terminal report. Only HTTP success with native integer `retCode: 0` adds the
existing environment-mismatch hint; a negative, malformed or timed-out probe
keeps the original rejection. Regional/site, custom endpoints, testnet, Hyro,
inverse, spot, public-only and remote-signing contexts are excluded.

Callers should retain the hint and native `10003` detail without treating the
probe as account recovery or switching environments automatically. Credential
role and deactivation policy stay unchanged. B9's code/field migration still
applies to the existing NX CLI message-prefix check; no caller method or saved
configuration shape changes.

#### BingX acknowledged IDs on partial placement failures (#1617)

BingX sequential placement failures now retain earlier acknowledged native IDs
in the existing `partialOrderIds`/`partialOrderCount` error fields. An empty
acknowledged set does not invent partial IDs; this helper does not supply a
total batch count or failed index. The original failure kind/native code stay
intact. A later definitive rejection does not mark the whole partially
effective batch `rejected`, while transport loss or malformed acknowledgement
keeps its `unknown` outcome.

Reconcile acknowledged orders and any uncertain failed step before retrying;
the remaining tail is not sent or replayed automatically. Worker consumers
read fields on the reconstructed Error, without relying on `instanceof`.
NX `apps/web/src/order/createScaledOrder.ts:64-69` at `65e086a3c90` already
reads the partial count or ID-list length. This adds BingX evidence to the
existing error contract, without a new caller method.

#### Bitunix classified failures and partial effects (#1589)

Bitunix failures now carry the existing typed error fields, preserving native
codes/messages; HTTP status owns classification over a contradictory body code.
Preflight/signing failures are `notSent`. After a dispatched write, transport
loss, HTTP 408/5xx, malformed acknowledgements and undocumented native errors
remain `unknown`. A failed replacement-reconciliation read does not imply that
a new write was sent.

Batch failure retains observed accepted IDs and confirmed effects. After partial
batch-place acknowledgement failures, independent later legs may still run before
the aggregate rejects once; transport, HTTP, envelope and cancel failures reject
immediately. An effective leg clears an aggregate `notSent`/`rejected` label,
while an uncertain leg keeps `unknown`. Reconcile partial IDs/effects before retrying. Only definitive
rejection may enter the existing recovery-write path; no automatic caller retry
is introduced. This extends B9's field-based integration to Bitunix, with no
new application method or error shape.

#### Events and worker protocol

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| Engine error logs → `'error'` payload `Exchange Error: <message>`; exactly one `'error'` per failed command (#1375, landed) | web `exchangeAccountController.ts:1986-2055` (`(err: string, opts)`); CLI `repl.ts:958-1025` (`args[0] as string`); candle-server `exchange-pool.ts:224-230` | venue `emitAndLogError` strings | prefixed strings; one toast per rejection after #1375 | RT |
| New events `command-progress`, `engine-dead`, worker op `command-progress` (#1375) | web dispatcher `exchangeAccountController.ts:2425-2800` (unknown ops ignored) | n/a | additive; surface `engine-dead` to the user | add |
| Worker frame mode (`__workerConfig { acceptsFrames: true }` → `op: 'frame'`, store payload slimmed, no `fill` op) | web `utils/shim/worker/workerFactories.ts:386` (does not set it); exchange-server VWC `virtual-worker-context.ts:247-320` | full `store` payloads | unchanged unless opted in; opting in requires `exchange.applyWorkerFrame(buffer)` on the main thread | none until opted in |
| `callMethodInWorker` timeout (#1375, landed): Rust = `rustCommandTimeoutMs + 1 s`, error `Rust worker command timed out; reconcile interrupted writes before retrying` with `code: 'COMMAND_TIMEOUT'`, `outcome: 'unknown'` | web `utils/exchange/callMethodInWorkerFireAndForget.ts:3-7` (`error.message === 'Timeout callMethodInWorker'`) | 5 s, exact message | match `code === 'COMMAND_TIMEOUT'` as well | RT |
| Partial-batch fields across the worker | web `order/createScaledOrder.ts:64-71` (`partialOrderCount`, `partialOrderIds`) | TS venue errors | preserved across the worker by `serializeCommandError` (#1375, landed) | none |
| Packed ticker delta adds `midPrice` | exchange-server `public-virtual-worker-context.ts:4,200-235` (imports `PACKED_TICKER_FIELDS`) | | follows the imported constant | none |
| `exec-result`/`updatePositions:done` gain `errorDetails` (#1375) | web CLI `components/cli/CLI.tsx:306`, `cliChainRunner.ts:454-479`, `runCliLine.ts:77-78` | `{reqId, success, error}` | additive | add |
| Native/DO diagnostic protocol (#1388/#1586) | no current NX `NativeFrameSession` consumer found; native/DO host adopters | native `event` plus legacy `log`; DO `op: 'log'` with optional code/fields | #1586 diagnostics use native `type: 'event'` / DO `op: 'event'`, levels debug/info/warn/error. Deploy compatible safe-cex-server before native Info+ events; upgrade DO clients before host adoption. #1554's command-correlated `history.incomplete` still needs ID translation when a server remaps commands | host compatibility/deployment prerequisite; Sam's call |
| #1527 fill/store frame ordering | web fill listeners, provisional positions and SDK chasers | fills could flush before already-pending orders by up to 250 ms | a fill-bearing flush carries pending store sections; existing appliers publish them before fill events. Venue execution-before-order delivery can still occur | none; no new FILLS revision |
| Coded diagnostics (#1583) | web worker dispatcher `exchangeAccountController.ts:2715-2727`; web/CLI health/error listeners in the [caller inventory](#coded-diagnostic-caller-cutover-pr5) | only legacy `error`/`log` paths handled | add worker `op: 'engine-event', data: [event]` dispatch and code-aware listeners; legacy Error `opts.engineEvent` / Info-Warn third `log` argument identifies duplicates. Debug has only structured delivery when received | additive SDK/worker API; caller migration required |

#### Exchange-server hosting

The server runs `store.webworker` in-process on a fake Worker
(`services/exchange-server/src/server.ts:1059`, `public-exchange-manager.ts:80`);
the op protocol is its ABI. Beyond B2, B6, B7:

| Item | tealstreet-next file:line | Requirement |
| --- | --- | --- |
| Signer error text parsed by `unknownSendOutcome.ts` | `remote-signer.ts:580,592,602,659,664,714,761` | keep byte-identical, or move `NOT_SENT_MESSAGE` and `alerts.ts` to codes together |
| Unhandled engine promise rejections | `alerts.ts:62,81-94` (`uncaught-exception`/`unhandled-rejection` routing) | engine promises must not reject unhandled, or alert routing changes |
| `callMethod` passthrough of any method name | `server.ts:1273-1344` | every hosted method name the web sends must exist on the Rust class |
| `exchangeWorkers[key].exchange` live access | `shared-public-data-source.ts:70-131` | `.store.subscribe`, `.store.{tickers,markets,loaded}`, `.listenOrderBook`, `.listenOHLCV` on the Rust object |
| Redis market/ticker seeds into `init` | `public-exchange-manager.ts:240`; `packages/safe-cex-server/src/publicExchange/{serialization.ts:95-130,tickerCache.ts:53-93}` | `Market[]`/`Ticker[]` JSON shape unchanged; Rust frames replace seeded store data after start |
| 512 MB worker memory guard | `Dockerfile.connect-exchange-cloudflare` `TEALSTREET_WORKER_MEMORY_MB=512` | wasm linear memory counts against it; one engine per account plus one per shared venue |
| Wasm stack frames | `packages/service-telemetry/src/capture.ts:97-120`, `alertKey.ts:22-26` | only JS sourcemaps ship; stable error `name`/`message` needed for alert keys |

#### CLI and MCP

| Item | tealstreet-next file:line | Requirement |
| --- | --- | --- |
| Console output is part of the CLI result | `apps/cli/src/oneShotResult.ts:41-76`, `repl.ts:2864-2919` (`setConsoleTap`; `KEY_REJECTION`; any exchange `error` line ⇒ `UNCONFIRMED`) | Rust errors must still log to `console.error`/`warn` with matching text |
| Log classification | `apps/cli/src/log-classify.ts:30-57` | engine log lines keep the critical keywords |
| MCP stdout is the protocol stream | `packages/exchange-mcp/src/stdio.ts` | wasm init/engine must not write stdout |
| Writable generic emitter | cli-core `worker-host/WorkerPlatformAdapter.ts:76`, `router.ts:38-107` (emit `exec-log`/`exec-result`) | `ExchangeEmitter` accepts arbitrary events (it does) |
| Global teardown | `repl.ts:2940-2948` (`virtualClock.stop()`, `disposeHOWWebSockets()`), MCP `index.ts:15` | `dispose()` must release engine timers/sockets so Bun/Node exit |
| Chaser/TWAP/cross-trade managers driven against the exchange | cli-core `ChaseCommand.ts:340-350`, `TwapCommand.ts`, `apps/cli/src/cross-trade-daemon.ts:221-224` | `on('update'|'fill')`, `mintClientOrderId`, `findOrderByClientOrderId`, `store.*BySymbol` on Rust (present) |

### OKX WebSocket port operations prerequisite (#1588)

**Open ops work, before 2026-10-31.** [OKX's port-retirement notice](https://www.okx.com/docs-v5/log_en/)
states public-internet WebSocket connections to 8443 stop on that date; 443
already works, with the same hosts and paths. #1588 removes `:8443` from SDK
global/EEA live/demo URLs; relay metadata covers only global live private/business
upstreams. This merge does not migrate deployed proxy upstreams or allowlists.
The overseer identified these deployment prerequisites on 2026-10-21; repo
[OKX routing notes](../../packages/safe-cex/src/exchanges/okx/CLAUDE.md) describe
the global/EEA tunnels and Caddy passthrough, while the
[routing table](../../packages/safe-cex/src/transport/tealstreet/routing/routingTable.ts)
pins the global relay targets and host+path allowlist requirement.

| Ops dependency | Required action / owner boundary |
| --- | --- |
| Python `ws-proxy` global/EEA termination | Move global `okx/private` / `okx/business` upstreams on `ws.okx.com:8443` and EEA `okxeu/private` / `okxeu/business` upstreams on `wseea.okx.com:8443` to default 443, preserving each entity host and channel path. Changing only client URLs does not update these server-side destinations. |
| `okx-proxy-ja.tealstreet.io` Caddy passthrough | Move its deployed upstream destination from 8443 to 443 before the deadline. Its client-facing 443 listener is not proof that its OKX upstream already uses 443. |
| `tradetunnel` relay allowlist | Ensure deployed `RELAY_ALLOWED_UPSTREAMS` host+path-prefix rules accept the port-less global-live `wss://ws.okx.com/ws/v5/private` and `/ws/v5/business` targets from relay metadata, retaining the intended host/path match. |
| Live master / hot patch | The overseer reports live master still uses 8443; checked mirror `origin/master` `1f91dfe4dcb` also retains those URLs. Sam decides the production hot patch and rollout timing; the Rust landing merge is not a deployed fix. |

Keep this prerequisite separate from B1's engine delivery and the Rust cutover:
the port deadline also affects the live TS path. No proxy edit, hot patch or
deployment is performed by this documentation refresh.

### Error strings that must move to codes

All of these match message **text**; Rust venue messages differ from the TS
venue messages they replace. Codes available on landing: `ExchangeError`
(#1424) on rejected **command promises**, with `kind` (`rejected`,
`invalidInput`, `authInvalid`, `authExpired`, `authPermission`, `rateLimited`,
`network`, `timeout`, `venueUnavailable`, `unsupported`, `internal`), `outcome`
(`notSent`, `rejected`, `unknown`) and `venueCode`, for the classified venue
families listed in B9; host `error.code` (`COMMAND_TIMEOUT`, `COMMAND_DEADLINE_FAILED`,
`ENGINE_PROTOCOL_ERROR`, `ENGINE_DISPOSED`, `ENGINE_DEAD`, #1375). Through the
web worker the error is a plain `Error` carrying those fields, so match
`(err as { kind?: string }).kind`, never `instanceof ExchangeError`. The
`'error'` event carries `opts.classified` (#1484), worker Sentry reports carry
`error_kind`-family tags (#1483), `start()` rejects with an `ExchangeError` and `validateAccount`
returns `kind` since #1482. #1388's event codes (`ws.reconnect`, `auth.rejected`, …) are
counted in `getHealth()`. Since #1583 they also reach `exchange.on('engine-event', …)`
and the worker's `engine-event` op; legacy Error uses `opts.engineEvent`, and
Info/Warn supplies the event as the third `log` argument. These are diagnostic
metadata, separate from classified command failures. Error telemetry carries
scalar `event_code` and the event object in extra data; explicit tags still win.

| Site | Matched text | Status | Target |
| --- | --- | --- | --- |
| SDK `health/errorPatterns.ts` via web `exchangeAccountController.ts:1773,1831,356-394` and CLI `apps/cli/src/exchange-manager.ts:203`, `repl.ts:958-1025`, `error-classifier.ts:18-76` | ~48 regexes (auth/rate/network/time-sync/order) incl. `Bybit API key environment mismatch` | **Movable** (#1484): pass `opts.classified` from the `'error'` handler (`:1986-2019`, `repl.ts:959`) | `processEvent(event, classified?)` for coded diagnostics and `processHealth(exchange.getHealth())` for lifetime counters (#1583); `processMessage(message, isError, classified)` for ordinary command/legacy TS errors. Do not classify a diagnostic twice via its structured and metadata-bearing legacy paths. The exhaustive switch `:356-394` is CT if categories change |
| web `exchangeAccountController.ts:245-246,338-340` | `/error sign/i` (bybit*); `builder fee has not been approved` (hyperliquid*) | **Movable**: `opts.classified` on the `'error'` event (#1484); bybit (#1479) and hyperliquid (#1470) are classified | `opts.classified.kind === 'authInvalid'` / `venueCode`, keeping the text for TS venues |
| `packages/exchange-pretty-error-messages/src/bybit/bybitHandler.ts:12,21-33` | `/ab not enough for new leverage/i`, `EC_*` reject reasons, `/error sign/i` | **Movable** for command rejections: bybit trading is classified (#1479) | map from `venueCode`; the `'error'`-event toasts read `opts.classified.venueCode` (#1484) |
| web `utils/exchange/callMethodInWorkerFireAndForget.ts:3-7` | `'Timeout callMethodInWorker'` | **Movable now** | `error.message === CALL_METHOD_IN_WORKER_TIMEOUT \|\| (error as { code?: string }).code === 'COMMAND_TIMEOUT'` (the Rust timeout message differs, `rust.exchange.ts:730-737`; TS venues have no code) |
| web `order/createLimitOrder.ts:361`, `createMarketOrder.ts:357` via SDK `utils/unknownSendOutcome.ts` | `UNKNOWN_OUTCOME_MESSAGE`/`NOT_SENT_MESSAGE` regexes | **Done in the SDK** (#1424): `isUnknownOutcome` checks `outcome` first (`unknownSendOutcome.ts:19-21`) and `outcome` survives the worker | no web edit; arrives with the safe-cex mirror. #1401 (merged) marks the Lighter, Coinbase, TxFlow, Orderly and Bybit unknown-outcome write sites |
| web `order/createLimitOrder.ts:367-372` | Lighter `below exchange minimums`, `min_base_amount`, `min_quote_amount` | **Movable** for command rejections: Lighter is classified (#1475) | `kind: 'invalidInput'`, keeping the text for TS venues |
| web `order/createScaledOrder.ts:642-651` | `reduce only`, `reduce-only`, `nothing to reduce`, `position does not exist`, `no position`, `position`+`close` | Partial: every Rust venue family is classified; TS-only venues still need the text | `err.kind === 'rejected' && err.venueCode === <venue code>`, keeping the text fallback for unclassified venues |
| web `components/newApiFlow/useAccountForm.ts:246-275` | `passphrase`, `api key`, `secret`, `whitelist`, `permission`, `secondary` | **Movable** (#1482): `validateAccount` returns `{ error, kind }` | branch on `result.kind` (`authInvalid`/`authPermission`), keeping the text for the field hint and TS venues |
| web `components/toasts/errorToast.tsx:30-32` | `Server was unavailable for a request`, `Cannot read properties of undefined` | Unchanged (app/TS strings) | n/a |
| web `utils/sentry/sentryErrorBlacklist.ts` | ~148 verbatim venue error regexes | **Movable** (#1483): worker reports carry `error_kind`/`error_category`/`error_outcome`/`venue_code` tags | filter Rust venues by tag; keep the regexes for TS venues |
| CLI `apps/cli/src/oneShotResult.ts:41-56` (`KEY_REJECTION`), `connect-error.ts:13-17`, `log-classify.ts:30-57` | key-rejection, `authentication`/`token`/`401`/`timed out`, critical log keywords | Partial: `code === 'COMMAND_TIMEOUT'` works for command timeouts; #1450 teaches `findKeyRejection` the engine wording | auth: startup `kind` (#1482); a runtime rejection raises `authFailureStreak` (#1450) and counts `auth.rejected` in `getHealth()` (#1388) |
| exchange-server `alerts.ts:82-89`, SDK `unknownSendOutcome.ts:10-13` | `Invalid API-key`, `Way too many requests`, `banned until`, signer messages | Blocked: process-handler reasons never see the error object | `kind: 'authInvalid'\|'rateLimited'` where the object is reachable; signer messages stay JS |

Typed managed-write failures (#1816, #1838; backfill). For a Binance managed
write that fails without a usable native answer, the `kind` changes but the
outcome and message do not:
- engine deadline expiry of a released write: `kind` was `network`, now
  `timeout`;
- a delivered reply that is not UTF-8 or exceeds the response bounds: `kind`
  was `network`, now `internal`;
- host transport loss: still `network`.

The `outcome` stays `unknown` and the message text is unchanged. Callers
branching on `kind === 'network'` for these Binance write failures should also
accept `timeout` and `internal`. Venues adopting the typed `Venue::on_http_failed` hook see the same
kinds; others are unchanged.

Engine error classification (Binance, [plan](RUST_ERROR_CLASSIFICATION.md)):
message text is unchanged. A Binance write that fails with a non-JSON HTTP
error used to carry `outcome: unknown` with no `kind`. It now has these fields:
- 429 or 418: `rejected` + `rateLimited`, plus `retryAfterMs` when the
  response carries a Retry-After header;
- 400: `rejected` + `invalidInput`;
- 401: `rejected` + `authInvalid`;
- 403: `rejected` + `rejected`;
- 409 and other statuses: still `unknown`.

Native `-1003`/`-1015` rate-limit refusals also gain `retryAfterMs` when the
header is present. Callers branching on `isUnknownOutcome` see fewer unknowns
for these answers; nothing that matched on text changes.

### Build and delivery steps

| Host | Today | Needed for the engine |
| --- | --- | --- |
| Web (Next.js, `apps/web`) | `next.config.mjs:330-336` `asyncWebAssembly`; safe-cex via tsconfig paths; only wasm is hand-copied `public/wasm/poseidon_crypto.wasm` | resolve `@tealstreet/safe-cex-wasm` (browser condition → per-family `pkg/web/<family>`); run `yarn rust:build-wasm --families` before `next build` (Vercel needs a Rust toolchain or prebuilt artifacts); tealstreet-next `sdk-rs/` path conflict (B1); worker realms (`StoreWorkerEntry.ts`, `ExchangeServerWorker.ts`, WorkerShim) load the engine |
| exchange-server (Bun) | `bun build --target bun` single file; `Dockerfile.connect-exchange-cloudflare` copies only `dist/*.js`; `scripts/cloudflare/deploy-edge.sh:45-47` has no Rust step | node/bun build of `@tealstreet/safe-cex-wasm/node`; inline or COPY the wasm; add the build to `deploy-edge.sh` and `.github/workflows/deploy-connect-data-edge.yml` (path filter lacks `sdk-rs`); budget memory |
| CLI (`apps/cli`) | `scripts/build.sh:60-98` `bun build --compile` ×5 targets, no asset embedding; `cli-release.yml` `bun-version: latest` | embed the wasm in the binary (Bun file import → `/$bunfs/`) and set the loader; pin Bun |
| MCP (`packages/exchange-mcp`) | Current tsup copies combined CJS/WASM to `dist/wasm/rust`, package files include it, and `registerBundledRustEngine()` loads artifact-relative bytes. The release workflow builds the pinned engine, checks exact packed files/licences and runs isolated smoke outside the repo. | SDK packaging gap closed (#1342); current #1666 CI exercises the packaged consumer. Do not infer an actual npm publication or NX deployment from source/CI evidence. |
| candle-server (`mobile-services/candle-server`) | `createPublicExchange` per venue | engine for the 9 default names; image already has a Rust stage (`Dockerfile.data-candles-cloudflare`) |
| Mobile | see below | Hermes bridge or TS fallback (B3) |

### Mobile

tealstreet-mobile does not depend on safe-cex as a package: `yarn sync`
(`tools/sync`) copies tealstreet-next `packages/*` into the gitignored
`src/packages/` and rewrites `@tealstreet/X` to `@packages/X`. It never copies
`sdk-rs/`, so `@tealstreet/safe-cex-wasm` cannot reach the app.

| Realm | How exchanges are built | Cutover effect |
| --- | --- | --- |
| App thread (Hermes, Expo 54, RN 0.81, New Architecture) | synced web code (`createExchangeFromAccount` → `registry.native.ts`) constructs exchanges that are never started | Metro bundle fails once synced: `rust.exchange.ts` imports `@tealstreet/safe-cex-wasm/host`, `/wire` and dynamically `/engine`, absent from `src/packages`; Metro ignores package `exports` (`unstable_enablePackageExports=false`) and has no `.wasm` handling. `metro.config.js` already stubs paradex-sdk and poseidon-crypto for Hermes; a similar stub (or a sync change) unblocks the bundle. Hermes execution needs the native host (B3). |
| Hidden WebView | loads the web app's `/exchange-host` (`apps/web/src/app/exchange-host/page.tsx:53`), a web bundle on `registry.ts` with WebAssembly; its Worker owns the hosted socket and signs with `createExchange(...).signHttpRequest` (`signingHost.ts:88-191`) | #1498/#1505 preserve hosted TS signing, including browser delegates for default-Rust Coinbase/Orderly (B2). The WebView supports wasm, but this is not an on-device execution migration or a chosen B3 solution. |

No Rust venue runs on mobile today: all 9 default names have
`supportedOnMobile: false` and `isExchangeAvailableInCurrentEnv` filters them in
React Native. JS-only or WebView changes ship OTA (`expo-updates`, runtime
version per build, `check-ota-native-deps` guard); a Hermes native bridge needs
a native EAS build and a device acceptance gate.

[RUST_MOBILE_B3.md](RUST_MOBILE_B3.md) (#1546/#1556) inventories device-direct
order writes, validation and warm-start reads, and costs the B3 options. Its
recommendation to keep device TS through cutover is an owner decision still
to make. It separately records existing app issues: missing Hermes transport
registration for proxied accounts (Bitget/BloFin/Binance inverse construction),
Lighter's throwing poseidon stub on order writes, and plaintext AsyncStorage
venue secrets/sync keys. Those app fixes are outside this SDK docs lane;
neither the costed recommendation nor this inventory resolves them.

**candle-server** (`mobile-services/candle-server`, serves mobile charts):
builds `createPublicExchange(name, { forceAllProxiesOff: true })`
(`src/exchange-pool.ts:169`) without `start()`, then `fetchMarkets()` (`:265`,
falls back to duck-typed `fetchMarketsAndTickers()` on `'Not implemented'`,
`:267-283`), `store.update({ markets, loaded })` (`:309-312`) and `fetchOHLCV`.
Before #1375 a Rust `fetchMarkets` returned the empty store (`<name> returned an
empty market list`) and `fetchOHLCV` rejected `<name> engine is not started`, so
candle requests for the 9 default names failed (502). #1375 makes
`fetchMarkets`/`fetchTickers` start the engine and wait for native data, which
clears this on landing (#1375 merged); still needs the engine in the image. The smoke `KNOWN_FAILURES` lists 8
of the 9 names (not `txflow`). Its Dockerfiles ship only `dist` and nothing
calls `setWasmEngineLoader`; the default node loader needs the gitignored
`sdk-rs/hosts/wasm/pkg/node`.

### Audit method

1. SDK delta: three-dot `git diff origin/master...origin/feat/rust-rewrite --
   packages/safe-cex sdk-rs/hosts` plus `gh pr diff` for each open contract PR.
2. tealstreet-next callers: `git -C <tealstreet-next> fetch origin master`, then
   `git grep -n "safe-cex" origin/master -- ':!packages/safe-cex' ':!sdk-rs'`
   grouped by host (apps/web; apps/cli, cli-core, exchange-mcp, venue-data,
   script-api-core; services/exchange-server, mobile-services, safe-cex-server,
   other packages), then each changed SDK symbol/string by name.
3. Mobile: GitHub tree/code search of Tealstreet/tealstreet-mobile.

## Consumer gaps repaired by this readiness change

The following repairs preserve the existing consumer seam without changing
registry defaults or claiming broader product support:

- `packages/safe-cex/src/index.ts` now honors explicit `engine: 'rust'` for
  registered Rust aliases. Factory callers previously stayed on the TS class
  even when they explicitly selected Rust. Unknown/unported aliases retain
  the existing registry fallback, as `ExchangeWorker` does.
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` and
  `packages/safe-cex/src/exchanges/base.ts` now enable the native placement-unit
  bridge for all four OKX aliases, in the worker and main store respectively.
  OKX base fills divided by `contractSize` must remain exact native contract
  quantities for chaser/copier arithmetic. The product-semantics follow-up also
  carries inverse native counts/quote notional and preserves USD-valued versus
  native PnL units; [scope and proofs](RUST_PRODUCT_SEMANTICS.md).
- `packages/safe-cex/src/exchanges/base.ts` reapplies worker-derived chaser
  tags when native order frames and derived `activeChasers` fields arrive in
  either order. The reusable `chaserOrderUpdates` function in
  `packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts`
  supplies the same enrichment to the worker postprocessor and main frame
  consumer. Main-store changes use the existing order mutation path and its
  update-ID/notification contract. This helper is now an additional dependency
  of the base shell that must survive its eventual extraction.

## Consumer-blockers follow-up

Native feed ownership and public-factory lifecycle are implemented on
`feat/rust-consumer-blockers`; no TS venue/default is deleted or changed.

| Caller boundary | Current proof / required integration |
| --- | --- |
| `ExchangeWorker` private-only / shared public data | Native Public-role suppression including retries and public-topic guards on multiplexed account sockets; one running public Rust provider fans typed public data into two private `ExchangeWorker` instances over transferred ports. Catalog/contract and typed semantic identity, native snapshots, private-state isolation, subscriptions and disposal are tested, including unchanged-seed readiness and frame-forwarded public sources. In-process `externalPublicData` sources are served over the same protocol (B6); legacy websocket gateway config is rejected on Rust, never silently ignored. |
| `safe-cex-server` `PublicExchangeFactory` | Cold native metadata startup, lazy Redis cache hits, forced native refetch/replacement and failed-refresh preservation are explicit; handle identity stays stable but callers re-read `handle.exchange` after refresh. A bounded startup deadline aborts stuck metadata transports; cache fallback revives a fresh lazy engine. Native product/environment profiles isolate current/latest cache and lock keys, including registry-default Rust venues. Native and unit tests cover concurrent refresh, delayed cache reads, hour rollover and disposal. |
| Saved-account CLI | Actual saved CFM config/factory/start and CLI account formatter run on native state; compiled Bun executable embeds real WASM and starts outside the repository. Offline routing fixture is not a proxy/live-auth claim. |
| MCP | Actual protocol reads native catalog, balance, positions and orders; native public feeds keep store-only ticker reads current (proved through Binance protocol calls); native REST polling remains active (proved through BreakoutProp protocol calls); explicit `EXCHANGE_ENGINE=rust`; packaged isolated Node artifact owns its CJS/WASM bytes and Rust dependency notices. No fallback on an explicit unsupported engine selection. |
| Native host | Trusted host-only typed public ingress; loopback frame-server proof and command-client rejection prevent private sections entering the public seam. No deployment change. |
| Mobile | Awaited `setReactNativeRustEngineLoader` caches host initialization; real browser-module bytes drive two saved account stores under Node. A Bun subprocess loads the actual native registry and Rust class without global WebAssembly/TextEncoder/TextDecoder during import, then initializes real bytes and reads the native catalog. Frame/host I/O still requires codec globals. Hermes/JSI bridge implementation, mobile binary packaging and device verification remain open. |

The shared policy retains each native venue's own metadata/account overlays and
private reconciliation. It does not generalize product coverage, signing modes,
arbitrary `market.info`, or hosted deployments. Provider/catalog failure stops
receiving accounts; it never silently opens account public sockets. See
[RUST_CONSUMER_BLOCKERS.md](RUST_CONSUMER_BLOCKERS.md).

Additional [rewrite audit findings](RUST_AUDITS.md) remain cutover gates.
The consumer repairs and offline proofs do not settle downstream engine delivery,
hosted keyless signing, a Hermes/native bridge, full live private parity or the
production shadow/canary evidence. Keep these separate from the repaired startup,
shared-feed and package-loading boundaries documented here.

## Source-proven deletion blockers and repaired boundaries

The Polymarket landing adds typed prediction sections to
`sdk-rs/engine/src/model.rs`, `sdk-rs/engine/src/wire.rs` and the WASM reader.
`packages/safe-cex/src/store/frameApplier.ts` reconstructs prediction-specific
`Market.info` (condition/outcome/question/description/end date/slug/tick and
neg-risk) and holding metadata through both frame consumers. This is an explicit
typed metadata exception, not an arbitrary native JSON `info` bridge. The
[product-semantics follow-up](RUST_PRODUCT_SEMANTICS.md) adds typed Bybit
classification and retained Bybit/OKX product coverage, with actual consumer and
native fixture proofs. It retains TS venues and registry defaults.

The table records current boundaries after both follow-ups. Repaired engine
seams and product fixtures do not migrate old application configuration or
certify live private connectivity. Passing baseline conformance does not make
those TS paths deletable.

| Existing path / exact source | Current status and user-visible effect | Required replacement and proof before deletion |
| --- | --- | --- |
| `packages/safe-cex/src/webworker/ExchangeWorker.ts`; `exchanges/base.ts`; `exchanges/rust/rust.exchange.ts` | **Repaired:** private-only and the legacy pre-start hook select native disabled policy. Public-role sockets/retries are suppressed; private/business roles and metadata REST stay local. | Preserve typed native policy and role/reconnect tests. Shared data must enter native state; `publicDataOnly` remains distinct from `privateOnly`. |
| `packages/safe-cex/src/exchanges/base.ts` (`initExternalPublicData`); TS provider modules; `exchanges/rust/publicFeed.ts` | **Explicit replacement implemented:** worker-safe shared-public-feed ports ingest typed tickers/books/candles natively and retain account metadata/overlays. In-process `externalPublicData` is served through `serveInProcessPublicData`; websocket gateway config rejects with migration guidance on Rust. | Application/gateway callers must adopt `RustPublicFeedProvider` and transferred `publicFeed` ports before their default cutover. Provider identity/catalog/readiness/disposal and private fan-out are tested; this does not claim an existing deployed gateway was migrated. |
| `sdk-rs/venues/bybit/src/lib.rs`; `sdk-rs/engine/src/model.rs`, `wire.rs`; `packages/safe-cex/src/store/frameApplier.ts`; `packages/cli-core/src/order/prepareCliOrder.ts` | **Metadata gap closed in documented native fixtures:** optional typed classifications preserve native `symbolType`, `marketRegion` and `underlyingTicker` across actual Rust discovery, worker application, transferred buffers and main application. | The real `prepareCliOrder` refuses stock/ETF/commodity/FX; explicitly empty and `innovation` crypto pass, absent and unknown new labels remain unknown. Preserve this regression and minimal typed metadata when migrating defaults/readers; this is not an arbitrary native JSON bridge or live TradFi/private connectivity proof. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts`; Rust adapter `fetchMarkets` | **Repaired:** cache miss explicitly starts Rust; Redis hits remain lazy. Forced refresh starts a replacement native engine and publishes it after successful cache persistence. | Preserve cache/lock/fallback semantics and native cold/refetch/error/disposal/hour-rollover regressions. Re-read `handle.exchange` after refresh; cached metadata does not prove a live engine/feed has started. |
| `sdk-rs/venues/okx/src/{lib,account,trading,history}.rs`; `packages/safe-cex/src/store/frameApplier.ts`; OKX TS reference family | **Product filter gap closed:** global linear/inverse swaps and EU X-Perp `FUTURES` retain native instrument/type/underlying metadata. EU uses account-scoped discovery for authenticated accounts and joins its product feeds, reconciliation and supported histories. | Keep actual inverse quantity/position/PnL/margin/ledger fixtures, native placement/chaser bridge, all four host/demo profiles and USD-M/PM control gates. Private documented fixtures and source/unit proofs do not certify live private units or connectivity. External-public overlays and default/deletion migration remain separate blockers. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts`; `exchanges/rust/rust.exchange.ts` option forwarding; `sdk-rs/venues/bybit/` category states | **Combined-product gap closed for the `bybit` linear profile:** saved flags are forwarded and all four inverse/options combinations retain independent public categories and shared private-account reconciliation. Typed option terms, quote/position/account Greeks, chain/expiries and native option order sizing/caller IDs are explicit. | Actual TS/native WASM fixtures compare discovery, private read categories, option APIs and price/limit-IV order payloads; Rust option seeds and depth-25 books have native protocol/frame proofs because TS generic tickers/sockets omit or misroute that product. Unsupported option controls/settings/queries fail explicitly; standalone demo/inverse/spot/Hyro keep their narrower profiles. Preserve external-public policy and live combined/private validation boundaries; do not infer default/deletion readiness. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `sdk-rs/venues/grvt/src/lib.rs` (`on_item` catalog filter); `packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts` (`fetchMarkets` catalog mapping), `packages/safe-cex/src/exchanges/grvt/CLAUDE.md` | Product gap resolved by [explicit retirement decision](GRVT_PRODUCTS_DECISION.md): TS posts `{}` and the official API defaults to `PERPETUAL`. Its generic mapper accepts injected Future/Call/Put rows but drops semantic metadata and cannot correctly cancel their symbols; explicit public non-perpetual catalogs were empty on production/testnet. | Retire only the unexposed mapper-only paths from cutover parity. Preserve TS, public exports and all other consumer/dependency gates. Any future enablement needs real listings, typed kind/expiry/strike/settlement metadata through frames/consumers and complete product lifecycle evidence; generic dispatch and perpetual replay do not prove it. |

## WASM packaging for web consumers

The browser default loader imports only the account venue's family engine
(`@tealstreet/safe-cex-wasm/engine` → `pkg/web/<family>`), with one literal
import per family. **tealstreet-next web builds must run
`yarn rust:build-wasm --families` before bundling and ship every family engine
the bundler emits**; all 28 family modules must resolve at build time.
`loadWebEngine(venue)` and `setWasmEngineLoader((venue) => …)` receive the Rust
venue. Family decisions: [RUST_WASM_FAMILIES.md](RUST_WASM_FAMILIES.md).

The #1661 source/build inventory distinguishes these shipped artifacts:

| Consumer | Artifact / delivery requirement |
| --- | --- |
| Browser SDK and browser worker | The venue's web family module via the browser `./engine` export. |
| Node/Bun SDK hosting | Combined `pkg/node` bindings and WASM via the Node `./engine` export. |
| MCP bundle | Combined Node module copied beside the bundle in `dist/wasm/rust` by `packages/exchange-mcp/tsup.config.ts`. |
| Compiled Bun CLI | Combined web bindings and embedded WASM bytes, registered by `apps/cli/src/rust-engine.ts`; this is not the filesystem-based Node loader. |
| Cloudflare Workers/Durable Objects | Deployment-supplied `loadEngine(venue)` passed to `createEngineObject`; it may select a family. The host does not own a separate Rust build. |
| React Native | Host-injected module or compatible native bridge registered before account construction; no separately built Rust artifact is selected automatically. |

The combined build comes from `yarn rust:build-wasm`; deployments serving both
combined and family consumers need both outputs. Explicit `./web` and `./node`
exports expose the combined binding layouts. #1661 changes WASM compiler
profiles only: the combined build uses a host-only size override, and WOO keeps
its original isolated-family arguments. Loader APIs, native release profiles
and fixed budgets stay unchanged. Poseidon is a separate Go WASM artifact,
outside these Rust compiler settings. This inventory does not certify which
binaries NX has deployed. Details: [RUST_WASM_ARTIFACT_SIZE.md](RUST_WASM_ARTIFACT_SIZE.md).

### Chrome module-readiness observations (#1688, #1703)

The measurement job and its asset uploads run only on landing pushes to
`feat/rust-rewrite` or manual `workflow_dispatch`, and are skipped on PR events.
It uses same-run family web assets and the combined Binance web build captured
before standalone overwrite, checking source/run/build-input fingerprints and
JS/WASM hashes. Three fresh Chrome processes/profiles per asset load the
production family loader for 28 families; combined web is a separate diagnostic.
Missing assets, browser errors, wrong WASM fetches and incomplete samples fail;
latency never fails the job. #1703 adds informational warnings when the
unrounded `moduleReadyMs` median is strictly above 75 ms for a family or
125 ms for combined web. Completed reports retain `raw.json.warnings` (empty
when none exceed the limits); escaped GitHub warning annotations carry the
same source/run and runner/runtime cohort. Individual sample, bootstrap,
Chrome-launch and end-to-end timings have no warning threshold.

A hard timing gate remains a separate decision, to revisit only after ten
additional complete landing reports beyond the frozen three recorded in
[the browser plan](RUST_BROWSER_COLD_START.md#warning-only-follow-up-plan),
with runner/browser cohorts reviewed. Dispatches and skipped/cancelled runs
do not count. This does not change production budgets or caller readiness.

The [first own-head dispatch](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39686838626)
passed at `371804a425f`, with 29 assets and 87 complete worker samples. Its
[raw report and median table](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39686838626/artifacts/12387059038)
retain browser/JS/Node/runner/source provenance and fetched WASM hashes; actions
artifacts retain for 14 days. This is distinct from a post-merge landing baseline.
Worker bootstrap and worker-creation-to-result timings use the parent clock;
loader import plus WASM initialization use the worker clock. Chrome launch is
recorded separately. No account is constructed and no exchange I/O is measured.

These are descriptive fresh-profile loopback module observations. OS caches
may be warm, and three samples do not establish device/network latency,
account startup or deployed NX behavior. B1 delivery, device/consumer proof and
other caller migrations remain open. No NX edit or new default loader follows
from this measurement. See [the measurement scope](RUST_BROWSER_COLD_START.md).

## Runtime callers and their replacements

These are the complete direct callers/importers outside the venue runtime
subtree. A type import of `Exchange`/`BaseExchange` is an API dependency, not a
construction of the old implementation. Rust currently extends `BaseExchange`,
so deleting that base first would also delete its working adapter.

| Exact path | Role / current dependency | Replacement before deletion |
| --- | --- | --- |
| `packages/safe-cex/src/index.ts` | Async class registry/cache; constructs every direct SDK/private/public exchange | Preserve `createExchange`/`createPublicExchange` API and explicit engine choice; move default aliases in both registries after evidence. Later return a neutral exchange interface, not `typeof BaseExchange`. |
| `packages/safe-cex/src/webworker/ExchangeWorker.ts` | Registry fallback, explicit Rust constructor, frame forwarding, store cadence, consumers/postprocessors | Keep Rust frame path and compatibility store mirror; replace fallback only per ready alias. Keep `instanceof RustEngineExchange` behavior or replace it with an explicit frame-source capability. |
| `packages/safe-cex/src/candleCache/types.ts` | `Exchange` interface for candle fetcher | Neutral candle-read interface; preserve history/query semantics. |
| `packages/safe-cex/src/crossTrade/getCopierWeightRatio.ts` | Exchange/store weighting contract | Neutral exchange/store interface; same balances/positions. |
| `packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.ts` | Exchange lifecycle, fills, store and order placement | Neutral interface backed by Rust; preserve fills, native position units, reconciliation. |
| `packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.ts` | Copier order entry | Neutral order interface with native placement-unit extras and caller-ID contract. |
| `packages/safe-cex/src/webworker/commandExecutor.ts` | Exchange supplied to CLI worker commands | Keep adapter; replace imported interface path only after extraction. |
| `packages/safe-cex/src/webworker/TelgramWorker.ts` | Exchange/store observer | Same neutral read/lifecycle interface; keep notifications. |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts` | Order update/ticker notification and task lifecycle | Keep on frame-backed store; preserve immediate order updateId reactions and throttled ticker reactions. |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.ts` | Live orders/positions/book, amend/cancel/place | Same interface backed by Rust; preserve caller ID replacement/native units and terminal orders. |
| `packages/safe-cex/src/webworker/twap/TwapInstance.ts` | Child chasers, book/position/store reads | Keep shell and frame-backed store; preserve scheduling, timeout and fill semantics. |
| `packages/safe-cex/src/webworker/postProcessors/ExchangeDependentPostprocessor.ts` | Base type for chaser/TWAP/message stats pipeline | Neutral exchange interface; retain derived state and pipeline order. |
| `packages/safe-cex/src/webworker/handlers/BaseHandler.ts` | Worker handler exchange type | Neutral exchange interface and existing command contract. |
| `packages/safe-cex/src/webworker/handlers/CallMethodHandler.ts` | Runtime `CallableMethodsInWorker` allowlist | Extract allowlist independently of implementation; preserve strict method dispatch. |
| `packages/safe-cex/src/safeCexAxiosCreate.ts` | TS venue Axios signing/retry/stats helpers accept `BaseExchange` | Remove only after all TS API factories and reference harness use cease. Rust requests go through engine transport; preserve shared Axios use outside this module if any. |
| `packages/safe-cex/src/createApiWithSecondaryKey.ts` | TS primary/secondary API construction | Retire after every TS venue/reference dependency is gone; prove Rust secondary-key routing first. |
| `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts` | Creates public exchange and reads/refreshes/hydrates markets; return type `BaseExchange` | Keep tested native startup/replacement and cache/lock API; re-read `handle.exchange` after refresh. Extract the neutral interface before deleting BaseExchange. |
| `packages/cli-core/src/commands/trading/ChaseCommand.ts` | Exchange type for shared chaser and book subscription | Neutral interface; preserve store-native units and cancellation. |
| `packages/cli-core/src/worker-host/router.ts` | Routes CLI commands to a live exchange | Keep worker adapter and structural `ExchangeLike` API. |
| `packages/exchange-mcp/src/server.ts` | `createExchange` then start/lifecycle; Exchange type | Same factory API, Rust loader/transport, neutral interface. |
| `packages/exchange-mcp/src/tools.ts` | Account/store reads, order/history calls, runtime `isInertCapability` | Keep RPC schemas; extract capability branding/probe helpers from base and preserve unavailable-query errors. |
| `packages/exchange-mcp/src/config.ts` | Registry-derived exchange-name validation | Stable supported-name registry independent of implementation; do not advertise an absent Rust alias. |
| `apps/cli/src/exchange-factory.ts` | Saved-account factory/start/dispose typed as `BaseExchange` | Preserve account normalization and lifecycle through neutral SDK API. |
| `apps/cli/src/repl.ts` | Exchange type, live account/store reads, refresh/action dispatch | Preserve `ExchangeLike`/worker adapter and frame-backed store; no venue constructor required. |
| `apps/cli/src/cross-trade-daemon.ts` | Casts platform-created exchange to full `Exchange` | Replace cast with explicit copier interface and Rust-backed implementation. |
| `apps/cli/src/record/recorders/orderbook.ts` | Direct keyless `createExchange` plus lifecycle/book callbacks | Same public Rust factory/subscription contract; prove book depth/cadence and disposal. |

Indirect construction paths are also deletion dependencies:

- `packages/safe-cex/src/createExchangeFromAccount.ts` normalizes credentials,
  options, product flags, broker settings, secondary keys and routing, then
  delegates to `createExchange`; preserve every field.
- `apps/cli/src/platform-adapter.ts` dynamically imports that saved-account
  factory; `apps/cli/src/exchange-manager.ts`,
  `packages/cli-core/src/commands/account/AccountCommand.ts`, and
  `packages/cli-core/src/legacyCommands.ts` create through the platform adapter.
  Replacing a registry constructor covers them only after current account
  options and the CLI build's WASM loading are verified.
- `packages/cli-core/src/worker-host/WorkerPlatformAdapter.ts` wraps the existing
  worker exchange; `packages/cli-core/src/types.ts` intentionally uses structural
  `ExchangeLike` instead of importing the concrete TS base.
- `packages/script-api-core/src/adapter/HostBindings.ts` deliberately defines
  a neutral structural host interface; `adapter/createCoreAdapter.ts` consumes
  store reads through it. Retain this boundary instead of introducing concrete
  Rust/TS class dependencies.
- `packages/safe-cex/src/webworker/store.webworker.ts` constructs
  `ExchangeWorker`; its worker/main protocol, frame transfer, restart, error and
  hosted/object compatibility paths must keep working.

`apps/web/` has no direct BaseExchange or concrete venue import in this mirror's
source audit. The downstream tealstreet-next consumers are inventoried in
[tealstreet-next callers at cutover](#tealstreet-next-callers-at-cutover).
Existing in-repo consumers still need their factory and store contract.

## tealstreet-next caller changes shipped at cutover

These land in tealstreet-next together with the cutover, never before it. Each
row is the exact edit the SDK contract change requires.

### Capability record (RUST_QUALITY item 8; #1400, #1411)

`isBaseCapabilityFallback`, `isInertCapability` and `isEmptyStubCapability` are
no longer exported from `@tealstreet/safe-cex/src/exchanges/base` (they were
never on the package index). The only reader is `exchange.capabilities`
(`ExchangeCapabilities`, keys in `CAPABILITY_METHODS`); it also counts an
unconditional `return []` as absent. A Rust exchange reports its record once the
engine exists: after `start()`, or after `await exchange.loadCapabilities()`,
which creates the engine without starting it. Every row is a compile-time break.

| File | Old call | New call |
| --- | --- | --- |
| `apps/web/src/components/openPositions/ClosedPositions.tsx:114` | `isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')` | `!exchange.capabilities.closedPnl` (drop the line-11 import) |
| `apps/web/src/utils/historical/collectAccountAxes.ts:162` | `!isBaseCapabilityFallback(exchange.fetchHistoricalOrders, 'fetchHistoricalOrders')` | `exchange.capabilities.orderHistory` |
| `apps/web/src/utils/historical/collectAccountAxes.ts:164` | `isBaseCapabilityFallback(exchange.fetchHistoricalFills, 'fetchHistoricalFills')` | `!exchange.capabilities.fillHistory` (drop the line-5 import; reword the line-143 comment; `HistoryCapableExchange` already extends `Exchange`) |
| `packages/venue-data/src/journal/symbolScopedFills.ts:74-76` | `typeof exchange.fetchIncomeHistoryRange === 'function' && !isBaseCapabilityFallback(…)` | `exchange.capabilities.incomeRange` (parameter type `{ capabilities: ExchangeCapabilities }`; drop the line-1 import) |
| `packages/venue-data/src/journal/symbolScopedFills.test.ts:94-99`, `symbolScopedFills.bybit.test.ts:35,47` | `{ fetchIncomeHistoryRange: … }` fixtures | `{ capabilities: { incomeRange: … } }` |
| `packages/venue-data/src/journal/axis-grading.registry.test.ts:89` | `!isBaseCapabilityFallback(exchange.fetchHistoricalFills, 'fetchHistoricalFills')` | `(await exchange.loadCapabilities()).fillHistory` (the instance is never started) |
| `scripts/history-audit/lib/axis-inventory.ts:72,74` (called at `:128`) | `isBaseCapabilityFallback(fn, method)` → `'base-fallback'`; `isEmptyStubCapability(fn)` → `'empty-stub'` | `await instance.loadCapabilities()`, then map `fetchClosedPnl`→`closedPnl`, `fetchIncomeHistory`→`incomeHistory`, `fetchIncomeHistoryRange`→`incomeRange`, `fetchHistoricalFills`→`fillHistory`: `'implemented'`/`'unavailable'` (the two absent states merge) |
| `scripts/history-audit/lib/closedPnl.ts:234` | `isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')` | `!exchange.capabilities.closedPnl` |
| `scripts/history-audit/lib/venue-survey.ts:86` | `!isBaseCapabilityFallback(exchange.fetchIncomeHistoryRange, 'fetchIncomeHistoryRange')` | `exchange.capabilities.incomeRange` |
| `scripts/history-audit/lib/venue-survey.ts:107` | `!isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')` | `exchange.capabilities.closedPnl` |
| `scripts/history-audit/lib/venue-survey.ts:165` | `isBaseCapabilityFallback(exchange.fetchHistoricalFills, 'fetchHistoricalFills')` | `!exchange.capabilities.fillHistory` |
| `CLAUDE.md:662` (root) | names `isBaseCapabilityFallback` as the brand check | name `exchange.capabilities` |

`packages/exchange-mcp/src/tools.ts` is a mirrored package: #1400 already
rewrote it, and Copybara carries that change.

Main-thread mirrors (`createExchangeFromAccount` objects that apply worker
frames and never create an engine) adopt the record from the worker's snapshot
frame (`CAPABILITIES` section, #1411) or, since #1462, through the
`fetchEngineCapabilities` worker method once markets load. Before adoption, a
Rust mirror's history methods are unbranded forwarders, so fills, orders and
income report **present** and `closedPnl` reports absent until the record arrives.
`apps/web/src/order/lostSendResolver.ts:82` needs no change: without an engine,
`findOrderByClientOrderId` answers from open orders, never definitively, as
before.

`RUST_VENUES` is now a `ReadonlySet<ExchangeName>` (use `isRustVenue`), and the
engine host no longer has `accountId()` (`getAccount()` answers for every venue
that reports an account lookup). No tealstreet-next file outside the mirrored
packages uses either. #1411's market quantity units travel as frame flag bits
and need no tealstreet-next edit.

### Runtime capability readiness (#1637)

Runtime bridge choices now come from the engine record: journal/closed-position
formats, fill-history time handling, history filters, STP/execution validation
and book isolation. There is no runtime venue-name or construction-policy
fallback. Synchronous pre-engine choices still use the shared construction
rules. Existing started behavior, FrameApplier sizing, `contractsNative` /
`remainingNative` computations, native nuke and OKX contract conversion remain
unchanged.

| Host API | Landed fields / compatibility |
| --- | --- |
| `EngineCapabilities` enums | `positionHistory`, `closedPositionFormat`, `fillHistoryTime`. |
| Runtime capability flags | `stpControls`, `bybitExecutionControls`, `rejectClientSlippage`, `finiteCommandNumbers`, `finiteHistoryBounds`, `rejectOrderHistoryFilters`, `publicCatalogueValidation`, `isolatedBookSubscriptions`, `inverseClosedPositions`. |
| Optional `EngineConstruction.bridge` | `spot`, `demo`, `primaryCredentialsOnly`, `payloadSigning`, `apiKeyIndexRequired`, `isolatedBookSubscriptions`; TS derives these synchronously from the shared JSON, while native construction reports the same policies. This metadata is separate from the runtime record. |

The CAPABILITIES payload remains eight bytes with existing flag/enum values
preserved; new policies occupy previously unused bits/nibbles. Ship matching
engine, decoder and SDK bridge so runtime policy is available on both worker
and main-thread consumers. Custom hosts/test fixtures constructing raw
`EngineCapabilities` must supply the added fields; construction metadata alone
cannot answer runtime policy. No application/account option shape changes.

Affected public methods include history fills/orders/positions, placement,
STP reads/writes, cancel-all, order-book queries, leverage reads/max leverage
and reported closed-PnL mapping. Their readiness behavior is now explicit:

| State at a capability-dependent call | SDK behavior |
| --- | --- |
| Existing startup is pending | Wait for that same lifecycle, even if its capability record arrives before startup completes, then apply policy/dispatch. No second startup, mirror engine or new timeout knob is introduced. |
| No runtime record and no startup in progress | Reject as `ExchangeError` with `kind: 'invalidInput'`, `outcome: 'notSent'` and a message naming `name.method` and asking for `start()` or `loadCapabilities()` first. The method does not silently start the exchange. An already-dead engine rethrows its stored terminal error. |
| Startup failed, including an undispatched call queued during disposal/trap | Reject with the same typed classification and a method-named startup-failed/reconnect message. Available startup cause is retained; a queued write whose startup traps remains `notSent` with `cause.code: 'ENGINE_DEAD'`, since it was never dispatched. |
| Ready runtime record | Existing immediate dispatch timing and venue policy remain; internal startup commands use their separate synchronous path and cannot await themselves. |

**Required caller handling:** catch typed pre-start/failed-start rejections and
handle them as unsent operations. Also handle an existing terminal engine error
through the engine-death path, preserving its classification/outcome. Ordinary
no-start errors use the existing
classified SDK error notification and `errorReported`; honor that marker to
avoid a duplicate toast in the promise catch. A terminal engine notification
already reports a startup trap, so queued call rejections do not emit another
terminal notification. Disposal also settles queued calls without dispatching
writes. This adds no automatic write retry or new coded engine-event format.
`fetchLeverageForSymbol` performs readiness checking before its warning/`null`
read-failure fallback, so callers must now handle that readiness rejection too.

**Recommended NX ordering:** acquire the record through worker-aware
`loadCapabilities()` when needed; prefer explicit engine/worker startup
readiness before history or local runtime-policy work to avoid queued calls
during slow startup. SDK pending-start protection applies even
when callers omit that extra wait; handling typed rejection remains required.
`loadCapabilities()` acquires a record without starting streams and is not
proof of successful account startup. On a main-thread mirror it asks the
worker; never start a second mirror engine. Trading/history forwarding remains
ahead of local runtime checks, so policy executes in the authoritative realm.

| NX caller evidence at read-only `65e086a3c90` | Ordering implication |
| --- | --- |
| Web controller: creates exchange `exchangeAccountController.ts:1743`, optionally hydrates `:1747-1752`, wires `callMethod` to worker `:2838-2849`, posts init `:2855-2870`, returns mirror `:3004` and publishes it `:1722`. | Mirror/worker presence does not prove startup completed. Cached markets can arrive first; prefer worker-aware readiness for local runtime-policy work. |
| History ingestion: `runHistoricalWindow.ts:44-45,62-69` checks only `store.loaded.markets`; `collectAccountAxes.ts:230` passes `{from,to}` to order history. | Cached market readiness is insufficient. Await record/readiness where appropriate and retain failed versus incomplete versus complete coverage outcomes (B11). |
| Web order helpers: `createLimitOrder.ts:62-76,243,293`, `createMarketOrder.ts:87-101`, `createScaledOrder.ts:255-268`; `sendOrder.ts:16-26` mints an ID then places. | Market/exchange checks do not prove runtime readiness. Preserve forwarding first and handle the typed failure; pure synchronous construction/ID policy stays separate. |
| Worker/hosted dispatch: NX SDK `ExchangeWorker.ts:316,324,330` creates/wires before awaiting start; `handlePayload:696-715` buffers only until an exchange exists; `store.webworker.ts:273-286,457-462` registers then dispatches. | Worker existence is not a startup barrier. The updated SDK waits a pending startup before runtime policy, including forwarded calls. |
| Standalone CLI: `apps/cli/src/exchange-manager.ts:108-112` awaits `startExchange` before publishing; `platform-adapter.ts:245-263` awaits start with its existing timeout. | Normal enable already orders calls after start. Keep that order; custom/direct callers still handle typed rejections. |

No explicit production `stpMode`, `loadCapabilities` or Orderly/TxFlow
`orderId`/`status` history-filter call was found in the searched NX roots;
external dynamic scripts are not bounded by this source audit. This is caller
guidance and a read-only inventory, not a completed NX implementation.

### Bybit explicit catalogue refresh

Existing `refreshMarkets()` callers now request atomic native public discovery
and receive added/removed membership after host publication; `markets:changed`
reports that same delta once. Cached `fetchMarkets`/`fetchTickers` remain reads.
The native capability flag is `marketRefresh` (wire bit15), false for other
venues; unsupported engines retain cached refresh behavior. Worker refresh runs
on the authoritative engine as a read. Shared consumers cannot refresh alone:
refresh the direct source and restart provider/accounts through their existing
catalogue-identity contract. New symbol fee discovery and degraded optional
startup categories require restart; account/trading state is preserved.

### Warm start skips Rust accounts (B4)

`apps/web/src/utils/exchange/eagerExchangeWarmStart.ts:123-124`: keep
`applyCachedSnapshot`, then return `{ hydratedFromCache }` before
`ensureMarkets` (`:58` calls `fetchMarkets`) and `startEagerUserDataFetches`
(`:127`) when `exchange instanceof RustEngineExchange` (import from
`@tealstreet/safe-cex/src/exchanges/rust/rust.exchange`). Otherwise warm start
starts a second private engine on the main thread before the worker exists. The
SDK cannot make `fetchMarkets` inert, because candle-server and the public
factory rely on it starting the engine (B10).

### Error classification reaches the health monitor (B9; #1484)

`ExchangeHealthMonitor` uses a classified kind only when the host passes it.
The SDK puts it on the `'error'` event options (`opts.classified`), the same
object that already carries `sendToSentry`/`enhancedContext`. The calls below
remain required for ordinary command/host errors and legacy TS diagnostics.

For received native diagnostics, #1583 adds `processEvent(event, classified?)`:
classification follows stable codes and available typed detail without matching
message text. Recognized codes may classify even Info/Debug events; unrecognized
Info/Debug returns `null`; unrecognized Warn/Error uses typed classification
or `unknown`.
A generic `request.failed` message alone does not establish auth failure.
`processHealth(exchange.getHealth())` copies lifetime counters into
`HealthState.engineHealth`; it does not increment observed `errorCounts` or
reconstruct events hidden by log-level filtering. Polling/calling it remains
host work, and an absent snapshot leaves the last one unchanged. Collect snapshots
in the engine-owning realm: an unstarted/disposed Rust exchange or a main-thread
mirror without a host returns `undefined`. Hosts must deliver snapshots to the
monitor owner; forwarding `engine-event` alone does not forward counter snapshots.

Use either `engine-event` or the event attached to a legacy notification for
that diagnostic. Metadata-bearing legacy Error/Info/Warn is compatibility
delivery, not evidence of another failure. Keep ordinary command/host errors
without metadata. See the [caller inventory](#coded-diagnostic-caller-cutover-pr5)
for web/CLI listeners, worker dispatch and telemetry migration. #1583 added no
native notice format; #1586 now removes legacy engine `log` actions/notices and
changes the DO diagnostic envelope to coded `op: 'event'`. Upgrade matching
WASM/JS hosts and DO clients together, and deploy compatible safe-cex-server
before native Info+ events. The [PR6 section](#log-shim-removal-pr6) records the
protocol; the deployment remains Sam's decision.

| File | Old call | New call |
| --- | --- | --- |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:1986-2019` (`exchange.on('error', (err, opts) => …)`) and the warning handler `:2041-2051` | builds `{ message, sentryData, enhancedContext, forceToast, suppressConsoleLog }` or passes `err` | add `classified: opts?.classified` to each object (and to the `err` type at `:1779-1788`) |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:1831` | `healthMonitor.processMessage(errorMessage, msgType === 'error')` | `healthMonitor.processMessage(errorMessage, msgType === 'error', classified)` |
| `apps/cli/src/repl.ts:959-962` (`'error'`) and `:1015` (`'warning'`) | `monitor.processMessage(error, true)` | `monitor.processMessage(error, true, (args[1] as { classified?: ClassifiedError } \| undefined)?.classified)` (`ClassifiedError` from `@tealstreet/safe-cex/src/commandOutcome`; it is not on the package index) |

### Command timeout code (B9)

`apps/web/src/utils/exchange/callMethodInWorkerFireAndForget.ts:3-7`:
`isCallMethodInWorkerTimeout` also accepts
`(error as { code?: string }).code === 'COMMAND_TIMEOUT'`. A Rust worker
command times out with `Rust worker command timed out; reconcile interrupted
writes before retrying` instead of `Timeout callMethodInWorker`.

### Incomplete history (#1542, #1554)

The SDK journal fold checks the host-style marker before consuming income
symbols/funding or folding fills. Flagged fills and symbol-scoped income reject;
complete account-wide fills remain usable while all flagged income enrichment is
discarded and funding stays unattributed. This protects direct SDK consumers;
SDK worker/hosted result forwarding now preserves that assertion as described below;
NX ingestion and coverage tracking remain cutover work.

Native sessions and Workers now translate event command IDs through the pending
owner and send those notices only to that client. Retired, disconnected, malformed
and unknown owners cannot leak notices to another client. `history.incomplete`
remains Warn but survives an Error minimum level as result metadata. The
structured marker arrives before its command result; JSON clients must still
track it to carry completeness onward.

**Open cutover work (B11), not a completed caller migration.** In the JS WASM
host, a `history.incomplete` event with a pending `fields.command` marks that
command's array result with non-enumerable `incomplete: true`. Kraken futures
emits it for a saturated 500-row income page that its since-only cursor cannot
traverse; Binance emits it when an income chunk walk exhausts its bound. The
newly attributed paths below now use the same existing flag. Available rows
remain useful, but totals are lower bounds. An unflagged result is not proof
that every venue's every history endpoint is exhaustive; an unattributed notice
does not mark an arbitrary result. Native/DO hosts drop ownerless coverage
markers rather than broadcast them.

#### Query ownership in capped history

| Venue / merged PR | Newly command-attributed `history.incomplete` path |
| --- | --- |
| BloFin #1628 | Closed-position page cap. |
| Bybit #1629 | Closed-PnL window page cap and order-history page cap. |
| Hyperliquid #1630 | Rolling-retention limit in order history (2,000 raw rows). |
| OKX #1631 | Closed-position page cap; noncomposite history page cap and non-advancing cursor. EEA composite paths retain their strict failures. |
| Paradex #1632 | Bounded history page-budget exhaustion; unbounded cap and other stalled walks still fail. |
| Bitunix #1633 | Order/income paging with more rows possible but a cap or stalled progress; complete terminal pages do not get a cap marker. Failed reads reject. |
| WEEX #1627 | Income lifetime request budget, truncated chunks, advertised additional cursor rows, failed pages and terminal query failure. Existing continuation versus rejection policy remains. |
| WEEX #1670 | Parsed empty native fill identities after query filters. Valid peers remain; an all-skipped in-scope result is a flagged empty success. |

The six attribution-only patches (#1627–#1632) retain Warn code/text. Their
markers precede retained-row completion, later window/chunk work, or the
existing terminal failure, depending on the path. `fields.command` is the
query's engine command ID, separate from venue/context data and query options.
These fixes change ownership metadata, not the retained rows, paging bounds or
failure policy of those six venue patches. No new frame or result schema is
introduced; producer and existing host behavior now join up for these paths.
Bitunix #1633 additionally rejects history transport/HTTP/envelope or malformed
row failures instead of returning accumulated earlier rows as a successful
array. Its genuinely capped/stalled order or income walks still return retained
rows with the attributed marker; a final page with no more work no longer gets
a false cap warning. Keep query failure, partial retained rows and a complete
result distinct; an error is not an empty/complete history window.
A WEEX marker can likewise precede its existing terminal rejection; it does
not turn that rejection into partial success. The #1627 host regressions use
the real native WEEX failure-event fixture to prove delivery to its pending
caller before rejection and owning-client command translation, keeping a
concurrent query unflagged. Existing host tests separately cover result-array
marking. These proofs cover SDK/native-event/DO boundaries, not NX worker/hosted hops.

WEEX #1670 makes skipped empty fill identities visible to the owning query.
After existing order, symbol and time filters, a parsed empty native `id`
(including `tradeId`/`fillId` aliases) is skipped without a replacement ID.
Valid peers remain, and `row.skipped` with `field: id` plus `history.incomplete`
emit once per query across its pages/windows. All-skipped in-scope rows yield a
flagged empty success; genuinely empty or out-of-scope-only results are not
flagged by this repair. Strict typed parsing and existing failure policy remain.
The semantic marker precedes success and survives Error-only logging; ordinary
Warn `row.skipped` remains subject to the log threshold. Native-verified fixtures
prove JS result ownership and Worker command translation, without marking a
concurrent query. Raw native/DO result messages retain their existing array
shape, so those consumers correlate the earlier owned event. The SDK #1666
transport below preserves the resulting array flag where adopted; NX ingestion
and coverage work remains required, including for flagged empty arrays.

**SDK query-scoped result transport (#1666):** `CallMethodHandler` now adds optional
`data.completeness: { incomplete: true }` beside the existing `requestId`,
`result` and `success` fields on a marked successful array response. The shared
`BaseExchange.callMethodInWorker` receiver restores the non-enumerable array
flag only after matching its pending `requestId`, before both resolving and
`afterMethodCalledInWorker`. Rust forwarded methods use this same receiver.
Concurrent unflagged requests remain unflagged; late/unknown IDs cannot mark
another request, and terminal failures remain failures. The record is plain
serializable data, so both structured clone and the hosted JSON hop preserve it.
There is no global warning latch or warning-text matching.

The public SDK exports `HistoryCompleteness`, `HistoricalRows`,
`historyCompletenessOf`, `withHistoryCompleteness` and
`copyHistoryCompleteness` (also available from `src/historyCompleteness`). The
record is a positive incompleteness assertion only. An absent, false or malformed
record does not promise complete history. SDK Rust Bybit/OKX/BloFin/WOO derived
position histories and reported closed-PnL mapping preserve the assertion;
Bybit's derived result retains it when either the closed-PnL or order axis is
incomplete. Existing failed-read policies and financial mappings are unchanged.

MCP UTC mapping preserves the marker for `get_order_history`, `get_fills`,
`get_closed_pnl` and `get_income`. A flagged response's text JSON is now
`{ requestId, rows, completeness: { incomplete: true } }`, where `requestId` is
that call's JSON-RPC ID. Unflagged responses retain their existing array shape;
there is no new assertion that they are exhaustive. Empty flagged rows remain
visibly incomplete, and terminal tool failures remain errors.

**Required NX consumption (B11 remains open):** consume completeness on the
result of the specific pending call before `map`/`filter`/spread or aggregation.
Upgrade SDK sender and receiver together. At read-only NX `65e086a3c90`, VWC
serializes the whole message (adding only outer account context), and
ExchangeServerWorker, hostedClient, hostedFrameClient and the web controller
forward the whole `callMethod:done` data. No field-picking adapter edit is
indicated; both sides must run the matching SDK for restoration. This source
audit is not a deployed-runtime or NX test proof. Direct JS consumers can read
`historyCompletenessOf(rows)` or `rows.incomplete` before transforming rows.
NX must still distinguish incomplete outcomes and leave coverage uncompleted.
Its records route currently writes no coverage, so coverage gating alone does
not require a records schema change.

Exact NX forwarding evidence at that pin:

| NX source | Behavior relevant to `data.completeness` |
| --- | --- |
| `services/exchange-server/src/virtual-worker-context.ts:173-176,319-325` | Keeps the whole message, adds outer `accountName`, then JSON serializes it; no `callMethod:done` field allowlist. |
| `apps/web/src/utils/shim/worker/ExchangeServerWorker.ts:576-578` | Forwards the ordinary response frame intact as `{frame: raw}`. |
| `apps/web/src/utils/shim/worker/hostedClient.ts:79-96`; `hostedFrameClient.ts:178-188,315-316` in the same directory | Routes the whole parsed message to its account facade; only outer account/instance context changes, and nonspecial response data passes through. |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:2731-2734` | Emits the whole `callMethod:done` payload data. |
| NX's SDK `packages/safe-cex/src/exchanges/base.ts:1463-1469` | Matches `requestId` but currently resolves only `event.result`; adopting the matching #1666 receiver is necessary to restore the sibling metadata. |

#1666's SDK regressions exercise structured clone and JSON serialization,
including an actual command-correlated host event. They do not execute NX's
VWC/facade/controller stack. Keep that full hosted round-trip proof separate
from the source audit and SDK transport tests.

For a raw native/DO client, associate `history.incomplete` by the translated
`fields.command` with that session's pending query before its `done`; retire
the association when the call settles or the session/engine is disposed. Do
not match warning text, symbol, arrival time or a global "last query" latch:
concurrent queries and clients must keep flagged and unflagged results separate.
The command belongs to the request receipt, not an extra filter to pass to the
venue.

At NX `65e086a3c90`, `collectAccountAxes.ts:43-57` has no incomplete
`AxisOutcome`, and `runAxis` (`:117-119`) labels any returned array `ok`.
**Required:** inspect completeness before reshaping and represent partial data
separately from `ok`, including an empty flagged array. Carry this explicit
outcome through `CollectedAxis`/`AccountAxes` and `toAccountBatch` (`:318-328`).
Income symbol enumeration (`:193-198`) must retain its incompleteness even if
it discovers no symbols; symbol-scoped fill aggregation (`:238-243`) must retain
it if the income list or any symbol response is incomplete. A later unflagged
response cannot clear an earlier partial assertion for that derived axis.

`runHistoricalWindow.ts:101` currently returns before both posting rows and
completing coverage for every non-`ok` outcome. Merely adding an incomplete
outcome would therefore also skip usable partial rows. If those rows are
persisted, retain the live-user check (`:106`) before posting (`:108`) and skip
`completeWindow` (`:109`) for the partial result. POST failure also must not
complete coverage. `postHistoricalRecords.ts:38-58,70-79` slices and JSON
serializes rows, so keep completeness in the collection outcome rather than
relying on an array property surviving those operations. The records API
(`app/api/historical-ingest/records/route.ts:262-282` under `apps/web/src`)
reads the row arrays and writes no coverage; completion is a separate request.

Required NX regressions remain: direct flagged nonempty/empty results, a
truncated income symbol list with successful fill queries (including zero
symbols), any flagged per-symbol response, and isolation between axes/calls.
Exercise the real hosted forwarding path with concurrent/out-of-order flagged
and unflagged replies. Prove usable partial rows can be posted without
completing their window, flagged empty windows stay uncompleted, ordinary
successful empty windows retain existing policy, and user changes or failed
POSTs cannot complete coverage. B11 stays open until caller implementation and
these proofs land; #1666 alone does not certify journal coverage.

The following responsibilities distinguish implemented SDK transport from the
remaining caller work before affected history can claim journal coverage.
NX references in this subsection were checked at `fdd2389a825`, with query/result handling rechecked at `65e086a3c90`:

| Boundary | Required edit / proof |
| --- | --- |
| SDK `webworker/handlers/CallMethodHandler.ts` → `BaseExchange.callMethodInWorker` / Rust forwarding; NX VWC → ExchangeServerWorker | SDK now sends optional `data.completeness` beside the existing `requestId` and restores it on that request’s array. SDK regressions cover structured clone, hosted JSON, concurrent peers, timeout/late IDs and errors. NX adapters already pass whole data in the read-only audit; upgrade SDK sender and receiver together. NX ingestion/coverage tests remain required. |
| SDK/MCP history reshaping: `tools.ts` (`withUtc`) → `server.ts` | SDK now preserves the marker through UTC mapping and serializes flagged history as `{requestId, rows, completeness}`. Real MCP protocol regressions distinguish concurrent flagged, empty flagged and unflagged calls; JSON-RPC IDs match their owning responses and failures stay errors. MCP consumers must accept the flagged envelope rather than treating all history results as arrays. |
| NX `apps/web/src/utils/historical/collectAccountAxes.ts` (`runAxis`, `enumerateTradedSymbols`, symbol-scoped fill aggregation) and derived-history adapters | Inspect the flag before `map`/`filter`/spread/folding/enumeration can drop it. A truncated income symbol list also makes the derived fill/position axis incomplete, even if each subsequent fill read succeeds. Preserve that state in `CollectedAxis`/`AxisOutcome`; do not return `status: 'ok'` for it. #1591 already guards direct SDK journal folds as described above; NX derived axes must still preserve their own completeness state. |
| NX `apps/web/src/utils/historical/runHistoricalWindow.ts` (`collectAccountAxis` → `postHistoricalRecords` → `completeWindow`) | Leave `historical_coverage` uncompleted for incomplete results. Currently only `status: 'ok'` reaches `completeWindow`, so the collection outcome must distinguish partial data from a successful empty/full window. Keep the available rows distinct from a claim of complete coverage; persisting partial rows must not complete that claim. Add a truncated-window regression. |
| Native sessions / Durable Object clients, if adopted (`sdk-rs/hosts/native/src/server.rs`, `sdk-rs/hosts/wasm/js/workers.mjs`) | The array flag is attached by `EngineHost`, not encoded as a new frame or `done` field. A direct session client must correlate `history.incomplete` itself and carry completeness onward. Both hosts now remap the event's `fields.command` through the same pending owner as `done` and target the owning client; remote consumers must correlate the translated marker before consuming rows. No current NX `NativeFrameSession` caller was found. |

The BloFin `historical_orders` base-unit backfill (#1529) is a separate
owner-run data migration, described above. The SDK metadata transport does not
perform that backfill or implement the remaining NX collection/coverage changes.

## Shared helpers that a venue-directory deletion would otherwise remove

The class import list alone is insufficient. These surviving runtime imports
must be extracted to stable shared API/policy modules (and imports/re-exports
updated) or retained deliberately when deleting the venue implementation:

| Caller | Venue modules / replacement |
| --- | --- |
| `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` | `asterdex/asterdex.types` auth/STP validation/types; `blofin/blofin.types`, `okx/okx.types`, `phemex/phemex.types` broker IDs; `bybit/bybit.types` Hyro endpoints; `hyperliquid/hyperliquid.cloid` caller-ID rules. Extract these without carrying TS network/venue classes into the shared module. Keep `clientOrderId` and base capability semantics. |
| `packages/safe-cex/src/index.ts` | Public exports from Bybit `bybit.order-params`; Hyperliquid `hyperliquid.builderFee`, `hyperliquid.types`, `hyperliquid.agent`; Aster `asterdex.builder-approval`, `asterdex.builder-fee`; GRVT `grvt.builder`; Lighter `lighter.builder-approval`, `lighter.account-tier`. These wallet/approval/public types remain an API even when a venue runs Rust. Move them preserving export names and independent browser wallet functionality. |
| `packages/safe-cex/src/createExchangeFromAccount.ts`, `accounts/buildExchangeAccountConfig.ts`, `transport/engine.ts` | `bybit/bybit.site` normalization/site/routing policy. Extract shared policy before deleting Bybit directory. |
| `packages/safe-cex/src/transport/tealstreet/routing/routingTable.ts` | `bitget/bitget.types` WebSocket constant. Keep routing policy/hostname fidelity in shared transport config. |
| `packages/safe-cex/src/types.ts`, `webworker/postProcessors/MessageHandlerStatsPostprocessor.ts` | `exchanges/types` shared telemetry/API types. Extract independently of venue removal. |
| `packages/safe-cex/src/exchanges/base.ts` | `packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts` exports shared `chaserOrderUpdates`. Preserve the function when extracting the base shell/main-frame consumer; it is shared with the worker postprocessor and keeps derived tags independent of message arrival order. |
| `packages/safe-cex/src/exchanges/base.ts`, `base.ws.ts`, `external-public-data.ts`, `in-process-public-data.ts` | Shared lifecycle, emitter, health, capability, public feed/store and TS socket infrastructure. Base remains a runtime dependency of RustEngineExchange. Extract shell contracts and public-feed policy before removing the base. Remove TS socket machinery only after all old venue/reference uses cease. |

The worker's postprocessor registry also retains `DisplayedPositionsPostprocessor`,
`DisplayedMarginPostprocessor`, `ChaserOrderEnrichmentPostprocessor`,
`OrdersRiskProfilePostprocessor`, `ChaserOrderPostprocessor`, `TwapPostprocessor`
and `MessageHandlerStatsPostprocessor`. None is replaced by deleting a venue:
keep derived display/store fields and sequencing while inputs become Rust frames.

## NX deep-import and method reachability recheck

Read-only NX `65e086a3c9039a79e4c7e9f354aba31d8f83009e` recheck for the
Cutover tail. These are source dependencies, not proof of deployed binaries;
negative caller searches do not cover third-party or dynamic user code.

| Module / surface | Exact callers and evidence | Remaining responsibility |
| --- | --- | --- |
| `hyperliquid/hyperliquid.constants` | web `atoms/builderAssignment.atoms.ts:1-9`, `utils/exchange/approveBuilderFeeLink.ts:3-8`, `app/api/affiliate/builder-assignment/route.ts:3-8`, `app/api/internal/hyperliquid-builder-fills/ingest.ts:3-8` and `route.ts:7`; `packages/venue-data/src/revenue/hyperliquid.revenue.ts:3` | Preserve builder addresses, tiers and fee conversions as stable shared exports before deleting venue files. |
| `bitget/bitget.family` | web `components/openOrders/header/OrdersHeaderActionButtons.tsx:13,144,163` calls `isBitgetVirtualAttachedTpSlOrder` | Preserve Buys/Sells bulk-cancel exclusions for virtual attached children; a generic ID/status check is insufficient. |
| `asterdex/asterdex.types` | web `components/newApiFlow/ApiKeyFormFragment.tsx:4-8,21,69-84`, `components/newApiFlow/useAccountForm.ts:10`; `components/settings/accountsSettings/AsterBuilderFeeApproval.tsx:8` | Retain wallet/address/key validation and main-wallet error routing independently of the TS exchange class. |
| Bybit inverse/spot classes | No direct production NX class import outside SDK runtime found. SDK registry/native-registry and WS types still use them. NX `sdk-rs/safe-cex-venue-bybit/scripts/{position-fetch-fixtures,inverse-position-fixtures}.mjs` both parse inverse source; spot remains a registry/WS-type dependency. | Preserve SDK/reference and AST-fixture dependencies; do not label a source comment as an active web class import. |
| SDK registry | `scripts/history-audit/lib/axis-inventory.ts:4,109,137` enumerates names and constructs an unstarted exchange to inspect methods | Preserve supported-name inventory and migrate probes deliberately to capability acquisition. The extension imports its own registry, not this SDK module. |
| Base shell/types/brands | `apps/web/src/utils/historical/collectAccountAxes.ts:5,162-164`, `apps/web/src/components/openPositions/ClosedPositions.tsx:11`, `packages/venue-data/src/journal/symbolScopedFills.ts:1,71-73` include runtime branding; other imports are Exchange types. RustEngineExchange still extends BaseExchange. | Preserve shell/interface, worker allowlist and stable capability API; existing capability-reader migration requirements still apply. |
| BloFin helper | No production mount/import found; `ExchangesContainer.tsx:59,223` uses GenericExchangeReactiveStateHelper. `onPositionMetaChange.ts:34-35` returns for BloFin. | Unused import/tests still compile, so remove or deliberately maintain them before deleting the TS class. Do not revive inert behavior by mechanically replacing `instanceof`. |

Ordinary NX books use `OrderbookComponentWorker.tsx:26-62` →
`utils/market/listenOrderBook.ts:316-324,412-424` → SDK
`OrderBookSubscriptionsHandler.ts:47-78` → `exchange.listenOrderBook`.
No direct application/CLI/MCP/service/mobile producer of these public REST
snapshot methods was found in the searched NX roots. This includes the MEXC
recheck for #1681; it does not exclude venue-internal TS calls. Without an
external public provider, MEXC
`mexc.exchange.ts:606-610` delegates to its TS listener, which requires a market
and calls `parent.fetchOrderBook(symbol, 100)` before the first callback and
subscription (`mexc.ws-public.ts:128-153`). TS WEEX
internally queries a snapshot before subscribing and after sequence gaps;
its native stream path is a separate contract. NX metadata marks WEEX
dev-only for new-account selection, but an existing instance can still subscribe
without that picker gate. Backpack's TS socket owns a
direct XHR snapshot instead of calling the exchange method. Paradex's optional
capped Market+IOC preflight queries a book, but the standard web market path
uses GTC and supplies automatic slippage only for Lighter; native Paradex
already owns its capped-order read. No production Hyperliquid dead-man-switch
arming/renewal loop was found. Generic worker dispatch alone is not a producer.

SDK-owned MCP has a static tool list in `packages/exchange-mcp/src/tools.ts`
with neither snapshot books nor `scheduleCancel`. CLI commands likewise expose
neither; `cli-core/src/types.ts:96-97` exposes `listenOrderBook`.
Script API `getOrderbook` delegates through `createCoreAdapter.ts` to the
host-owned subscription cache (`HostBindings.ts:106-110`), not these REST
methods. Manual/third-party SDK parity remains required; this bounded search
set batch priority: WEEX metadata (now repaired through the native shared
policy), then GRVT typed builder errors, then the unobserved direct-query/
dead-man API gaps. #1688 now supplies Chrome module-readiness observations;
device/network, account-startup and deployed NX timing evidence remain open.
No NX source was changed by this audit.

## GRVT builder authorization failures: required caller migration

**REQUIRED for cutover:** NX must migrate its live reauthorization handler;
#1679 fixes SDK transport but does not implement this application change.

Definitive native GRVT placement or amendment-preflight failures now expose
`code: 'GRVT_BUILDER_UNAUTHORIZED'`, `reason: 'unauthorized' | 'cap-too-low'`,
`requiredRaw: 300` and, for a low cap, `authorizedRaw`. The original message,
command classification/outcome and exactly-once reporting survive. Native
`VenueError.builderAuthorization` is optional; the JS host validates the complete
record and exposes flat fields. SDK command serialization preserves only the
validated fields across both worker structured clone and hosted JSON; it does
not reconstruct the old class prototype. Unknown/pending/transport/malformed
checks carry no approval record and must never open the reauthorization modal.

Use the SDK-root `builderAuthorizationOf(error)` helper. It returns the validated
code/reason/caps or `undefined`; code-only or message matching is insufficient.
Keep the existing `GrvtBuilderUnauthorizedError` export for TS callers, but
`instanceof` alone cannot recognize Rust or deserialized failures. Import
`builderAuthorizationOf` (and, if needed, `BuilderAuthorizationDetail`) from the
SDK root, validate once, and read the returned record:

```ts
const approval = builderAuthorizationOf(error);
if (!approval) return false;
// Use approval.reason/requiredRaw/authorizedRaw in the existing modal path.
```

The helper accepts a positive uint32 `requiredRaw`; `unauthorized` has no
`authorizedRaw`, while `cap-too-low` requires a uint32 cap below `requiredRaw`.
Invalid/code-only records return `undefined`. Native #1679 sets the required
cap to `300`. Preserve the command outcome: placement preflight retains
`notSent`, while amendment
inspection retains `unknown`, even though its regression sends no create/cancel
write. Preserve the original error and normal outcome handling; do not replay
the placement or amendment automatically.

Read-only NX `65e086a3c9039a79e4c7e9f354aba31d8f83009e` findings:

| Caller | Required migration |
| --- | --- |
| `apps/web/src/order/handleGrvtBuilderUnauthorizedError.ts:19-35` | Replace its class-only check with the validated structural helper. Copy reason/caps into the existing modal and preserve reason-specific toast behavior. |
| `apps/web/src/order/createMarketOrder.ts:359`, `createLimitOrder.ts:365`, `createConditionalOrder.ts:263`, `createScaledOrder.ts:686` | Preserve cleanup/rethrow in each catch while routing recognized approval errors through the handler. |
| `apps/web/src/components/modals/GrvtBuilderReauthorizeModal.tsx` and `atoms/modals.atoms.ts:31-35` | Preserve reason-based content, stored raw caps and approval reload behavior; no new modal schema is required. |

The NX audit found whole-message hosted forwarding, but its audited embedded
SDK sender/receiver dropped reason/caps. Upgrade the SDK sender and receiver
together; downstream forwarding cannot repair fields already removed. Ordinary
error-event telemetry does not open this modal. The native and SDK tests prove
the transport contract, not deployed NX adoption. Both reasons now traverse the
actual SDK sender and matching receiver under structured clone and JSON;
`unauthorized` explicitly retains absence of `authorizedRaw`. Required NX proof
still includes each placement catch reaching the reason-specific modal,
malformed/unrelated failures remaining on their normal path, duplicate
event/rejection handling, cleanup/rethrow and approval-triggered reload. The
modal stores the raw caps but currently renders reason-based content rather
than numeric caps. No NX source was edited.
See [the implementation plan and regression scope](RUST_GRVT_BUILDER_ERROR.md).

## Ordered deletion plan

**Owner decision:** keep executable TS references as the CPU/memory oracle
through cutover. No venue or reference deletion until after cutover. The
following dependency order is a future plan, not deletion authorization.

1. **Prove the seam first (this PR).** Run the `*.wasm.test` conformance matrix
   over every `RUST_VENUES` name through worker/frame/store and consumer paths;
   fix adapter/FrameApplier failures. Preserve TS runtime and the frozen reference
   corpus. Keep absence of optional data distinct from an authoritative empty
   snapshot, updateId bumps, native units, terminal fences and serialize/stub
   parity. Run integration/build/replay controls as applicable.
2. **Close unsupported aliases/products.** Port the 4 missing names or obtain an
   explicit supported-product retirement decision in a separate change. A
   perps port never retires a saved spot/inverse account implicitly. Audit
   all credential/signing modes, proxy/tunnel/region, secondary keys, product
   toggles, hosted public data and query capabilities before changing defaults.
   Preserve the closed Bybit classification/combined-product and OKX inverse/EU
   proofs in [RUST_PRODUCT_SEMANTICS.md](RUST_PRODUCT_SEMANTICS.md). Remaining
   blockers include private-only/external-public feed modes, public-factory
   cold/forced discovery and live account/consumer migration. Registration alone
   does not remove these blockers. GRVT's unexposed non-perpetual mappings have
   a separate [retirement decision](GRVT_PRODUCTS_DECISION.md); its other gates
   remain, and no TS venue is deleted here.
3. **Make default creation converge.** Switch ready aliases in both
   `registry.ts` and `registry.native.ts`; keep all current factory entry points
   and account normalization. Prove actual CLI, public factory and MCP creation,
   module loading/bundling, forced refresh, and worker legacy/object consumers.
   The explicit `engine: 'rust'` selector added here is not this default switch.
4. **Extract shared contracts and wallet utilities.** Move the modules above,
   public exchange interface, method allowlist, capability branding and shell
   lifecycle out of `BaseExchange`/venue directories. Update public re-exports
   without changing consumer APIs. Keep `DefaultStore`/FrameApplier as the
   compatibility seam until their consumers have replacements.
5. **Delete implementations in dependency order, with fresh evidence per PR.**
   Coinbase/Orderly/TxFlow production implementation is already replaced and their
   frozen references remain. Next choose independent ready families (Backpack,
   BingX, Crypto.com, MEXC, BloFin, Bitrue, Phemex, WEEX, WOO, ApeX, Paradex,
   Aster, Polymarket, GRVT, Breakout legacy) after defaults and helper extraction. Delete
   Bybit/Hyro and OKX groups together (subclasses depend on parents), then
   Bitget's three profiles together.
   Remove KuCoin futures, Kraken futures, Hyperliquid perps and Lighter only
   while retaining shared code required by still-unported spot/broker siblings.
   Remove the four Binance TS profiles only after each default path and native
   unit/account/query contract is proven and their shared oracle is retained
   independently; the new Spot/COIN-M ports do not retire USD-M/PM evidence. Retire the remaining 6 implementations only after step 2.
   This ordering is a dependency order, not an assertion that all are already
   deployment-approved.
6. **Retire each old oracle deliberately.** Rewrite useful deterministic venue
   regressions to drive Rust using independent documented fixtures. The replay
   harness dynamically loads TS classes from production paths; freeze/move the
   intended oracle before removing those paths, following Coinbase/Orderly/TxFlow's
   test-only reference pattern. Keep a fixed baseline/performance oracle or
   explicitly replace that gate with a reviewed absolute budget. Never turn a
   TS-vs-Rust gate into Rust-vs-itself.
7. **Remove shared TS machinery last.** Only after no runtime/test/reference/
   harness import remains, remove TS API factories, Axios/signing-only helpers,
   `BaseWebSocket` and `BaseExchange`. The TS store and object payload bridge
   need a separate consumer migration: worker chasers/TWAP/CLI/postprocessors,
   hosted consumers, serialization/warm boot and downstream frame adoption must
   all have explicit replacements. Their deletion is not authorized or proven
   by this phase-9 readiness work. Keep the master merge/deployment decision
   separate, per the rewrite plan.

## Audit method and scope

The audit used repository-wide `rg` for `BaseExchange`, concrete `*.exchange`
imports/constructors/inheritance, registry calls, factory calls, dynamic import/
require and `instanceof`, then resolved relative and `@tealstreet/safe-cex` module
paths in tracked source against both production `src/exchanges/` and frozen
`src/__tests__/reference/` modules. The original resolver omitted imports into
the latter root; PR review caught that omission and the frozen-root pass below
closes it explicitly. The appendices group every direct static/dynamic
class/base/registry import plus mock/source-reference literals by exact caller
file. Paths are relative to repository root; targets below are relative to
`packages/safe-cex/src/exchanges/`, except explicitly repository-rooted
targets for the frozen `src/__tests__/reference/` corpus. Both roots are
resolved: test-only classes remain real inheritance/import dependencies even
though their production registry has switched to Rust. A `.test` file is not runtime evidence;
`__tests__/reference` is deliberately executable only in tests/harness.
Generated bundles, dependency directories and non-source documentation are not
production callers. Comments mentioning BaseExchange without an import are
context, not another call site. Arbitrary consumer-supplied dynamic paths or
external/downstream repositories are outside this audit's proof.

The harness is outside the requested `packages/`/`apps/` inventory but is an
explicit deletion blocker: `sdk-rs/harness/venues.mjs` and `kucoin.mjs` specify
TS module paths dynamically; `replay.mjs` loads them and
`known-ts-fixes-*.mjs` patches the independent TS oracle. Update that mapping
and fixture provenance when a production class moves to test-only references.

## Appendix A: venue runtime import graph

These callers remain until their family is migrated. `base` consumers switch to the extracted shell/interface; websocket/API implementations are removed with their venue; subclass groups migrate together. Registry entries are replaced by Rust constructors per readiness above. The `rust/` adapter and loaders stay.

| Exact caller path | Imported base/class/registry targets |
| --- | --- |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.ws-private.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.ws-public.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/exchanges/backpack/backpack.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/exchanges/backpack/backpack.ws-public.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/exchanges/base.ws.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binance.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binance.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binance.ws-private.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/exchanges/binance/binance.ws-public.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.ws-private.ts` | `binance/binanceinverse.exchange` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.ws-public.ts` | `binance/binanceinverse.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancepm.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancepm.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancepm.ws-private.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancepm.ws-public.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancespot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancespot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancespot.ws-private.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancespot.ws-public.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/exchanges/bingx/bingx.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bingx/bingx.ws-private.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/exchanges/bingx/bingx.ws-public.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitget.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitget.ws-public.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitget.ws-secondary-public.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitgetinverse.exchange.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitgetusdc.exchange.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.ws-private.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.ws-public.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-futures.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-private.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-public.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/exchanges/blofin/blofin.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/blofin/blofin.ws-private.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/exchanges/blofin/blofin.ws-public.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.api.ts` | `breakoutprop/breakoutprop.exchange` |
| `packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.ws-public.ts` | `breakoutprop/breakoutprop.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybit.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybit.fee-overlay.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitdemo.exchange.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitinverse.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybitinverse.ws-private.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitinverse.ws-public.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybitspot.ws-private.ts` | `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitspot.ws-public.ts` | `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/exchanges/bybit/hyrotrader.exchange.ts` | `base`, `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/hyrotrader.order-recovery.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/hyrotrader.ws-private.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.ws-private.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.ws-public.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/exchanges/external-public-data.ts` | `base` |
| `packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/grvt/grvt.ws-private.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/exchanges/grvt/grvt.ws-public.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-private.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-public.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.ws-private.ts` | `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.ws-public.ts` | `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/exchanges/in-process-public-data.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/kraken.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/kraken.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/kraken.ws-private.ts` | `kraken/kraken.exchange` |
| `packages/safe-cex/src/exchanges/kraken/kraken.ws-public.ts` | `kraken/kraken.exchange` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.ws-private.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.ws-public.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.ws-private.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.ws-public.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-private.ts` | `kucoin/kucoinspot.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-public.ts` | `kucoin/kucoinspot.exchange` |
| `packages/safe-cex/src/exchanges/lighter/lighter.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/lighter/lighter.ws-private.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/exchanges/lighter/lighter.ws-public.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/exchanges/mexc/mexc.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/exchanges/okx/okx.api.ts` | `base`, `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okx.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/okx/okx.ws-private.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okx.ws-public.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okxdemo.exchange.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okxeu.exchange.ts` | `base`, `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okxeudemo.exchange.ts` | `okx/okxeu.exchange` |
| `packages/safe-cex/src/exchanges/paradex/paradex.api.ts` | `base`, `paradex/paradex.exchange` |
| `packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/paradex/paradex.ws-private.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/exchanges/paradex/paradex.ws-public.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/exchanges/phemex/phemex.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/phemex/phemex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/phemex/phemex.ws-private.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/exchanges/phemex/phemex.ws-public.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.ws-private.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.ws-public.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/exchanges/registry.native.ts` | `apexomni/apexomni.exchange`, `asterdex/asterdex.exchange`, `backpack/backpack.exchange`, `base`, `binance/binance.exchange`, `binance/binanceinverse.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitget/bitgetinverse.exchange`, `bitget/bitgetusdc.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `breakoutprop/breakoutprop.exchange`, `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `bybit/hyrotrader.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange`, `kraken/kraken.exchange`, `kraken/krakenspot.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `lighter/lighter.exchange`, `mexc/mexc.exchange`, `okx/okx.exchange`, `okx/okxdemo.exchange`, `okx/okxeu.exchange`, `okx/okxeudemo.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `polymarket/polymarket.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange` |
| `packages/safe-cex/src/exchanges/registry.ts` | `apexomni/apexomni.exchange`, `asterdex/asterdex.exchange`, `backpack/backpack.exchange`, `base`, `binance/binance.exchange`, `binance/binanceinverse.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitget/bitgetinverse.exchange`, `bitget/bitgetusdc.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `breakoutprop/breakoutprop.exchange`, `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `bybit/hyrotrader.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange`, `kraken/kraken.exchange`, `kraken/krakenspot.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `lighter/lighter.exchange`, `mexc/mexc.exchange`, `okx/okx.exchange`, `okx/okxdemo.exchange`, `okx/okxeu.exchange`, `okx/okxeudemo.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `polymarket/polymarket.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange` |
| `packages/safe-cex/src/exchanges/rust/loadNodeEngine.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/exchanges/rust/loadWebEngine.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/weex/weex.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/weex/weex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/weex/weex.ws-private.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/exchanges/weex/weex.ws-public.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/exchanges/woo/woo.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/woo/woo.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/woo/woo.ws-private.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/exchanges/woo/woo.ws-public.ts` | `woo/woo.exchange` |

## Appendix B: tests and frozen references

These are all direct base/concrete-class/registry callers in test source at the audit checkpoint. Type-only mocks retain the neutral interface; implementation tests migrate to Rust-backed tests or reviewed test-only frozen references before the target is removed. Assertions are not silently dropped. Entries importing only `rust/rust.exchange` already exercise the new adapter and stay. The new `cutover/consumers.wasm.test.ts` drives the shared consumer/store seam; `cutover/realVenue.wasm.test.ts` supplies real native venue startup/frame coverage. Their support modules `packages/safe-cex/src/__tests__/cutover/consumerFrame.ts` and `packages/safe-cex/src/__tests__/cutover/realVenue.ts` construct inputs; the latter deliberately depends on Bybit endpoint constants and the replay harness/typed tape builders, so those test-only dependencies must move with any helper/oracle extraction.

| Exact caller path | Imported base/class/registry targets |
| --- | --- |
| `packages/safe-cex/src/__tests__/audit-regressions/binance-variants.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.breakoutprop-wrapper.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-private-rust-port.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-public-delta-rust-port.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-signing-integer-precision.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-trading-rust-port.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.grvt-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.price-history.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.private-executions.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.public-book-state.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.polymarket-frame.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.account-identity.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.candle-handler-guard.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.cfm-private-ws-contract-size.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.ohlcv-stats-cleanup.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.open-orders-polling.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.place-order-response.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.private-ws-auth.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbasespot.spot-only.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.cancel-algo-orders.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.expected-order-rejections.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.isolated-margin.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.leverage-clamp.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.market-leverage-limits.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.orderbook-snapshot-retry.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.private-ws-fill-mapping.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.startup-leverage-error.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.trailing-stop-rejection.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/kodiakfi/kodiakfi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/mode/mode.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/salsadex/salsadex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/what/what.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/woofi/woofi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/cutover/consumers.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` | `base`, `rust/rust.exchange`; retain the optional-metadata/multi-market frame contract, replacing the TS base with a neutral consumer bridge before base deletion |
| `packages/safe-cex/src/__tests__/cutover/realVenue.wasm.test.ts` | `rust/rust.exchange` |
| `packages/cli-core/src/commands/utility/tasksAndKill.test.ts` | `base` |
| `packages/cli-core/src/worker-host/router.test.ts` | `base` |
| `packages/exchange-mcp/src/tools.test.ts` | `base`, `registry` |
| `packages/safe-cex/src/__tests__/asterdex.leverage-brackets.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.partial-amend.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.remaining-audit.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.rust-parity.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.split-tpsl.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.builder-attribution.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.orderbook-snapshot.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.private-balance.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.testnet-clock.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.trailing-stop-capability.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.v3-auth-context.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.v3-stp.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.execution-history.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.native-batch.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.order-mapping.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.orderbook-depth.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.state-reconciliation.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.trigger-reference.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.dispose.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.http-failure-context.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.watchdog.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-websocket.contract.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-websocket.reconnect-policy.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.cancel-symbol-orders.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.income-pacing.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.order-snapshot-failure.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.position-mode-regression.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.private-stream-recovery.test.ts` | `binance/binance.exchange`, `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.response-shape.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.shared-public-leverage.test.ts` | `binance/binance.exchange`, `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.v3-account-contract.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binanceinverse.exchange.test.ts` | `base`, `binance/binanceinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancepm.position-sign.test.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancepm.response-shape.test.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancespot.private-wire-shape.test.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancespot.rest-contract.test.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancespot.user-data-stream.test.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bingx.audit-regressions.test.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bingx.native-amend.test.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.account-info.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.public-ws-product-type.test.ts` | `bitget/bitget.exchange`, `bitget/bitgetusdc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.rest-freshness-and-market-shape.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.sized-stop-plan-type.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.trailing-stop.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.uta.test.ts` | `bitget/bitget.exchange`, `bitget/bitgetinverse.exchange`, `bitget/bitgetusdc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.validate-account.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.ws-fill.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.attached-tpsl-payload.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.history-identity.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.leverage-read-probe.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.order-snapshots.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.private-executions.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.remaining-audit.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.trading-safety-defaults.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.websocket-transport.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.native-modify.test.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.remaining-audit.test.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.validate-account.test.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/blofin.auth-failure-backoff.test.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/blofin.remaining-audit.test.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/blofin.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/breakoutprop.order-contract.test.ts` | `breakoutprop/breakoutprop.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit-inverse.conditional-orders.test.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.advanced-order-params.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.auth-failure-backoff.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.demo-url-resolution.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.initial-position-snapshot.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.margin-mode.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.market-slippage.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.monthly-candles.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.positions-pagination.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.recently-placed-order-preservation.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.reinit-on-signing-failure.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.uta-default.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybitdemo.order-risk.test.ts` | `bybit/bybitdemo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/capability-branding.test.ts` | `base`, `registry` |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` | `apexomni/apexomni.exchange`, `asterdex/asterdex.exchange`, `backpack/backpack.exchange`, `base`, `binance/binance.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `breakoutprop/breakoutprop.exchange`, `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `bybit/hyrotrader.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange`, `kraken/kraken.exchange`, `kraken/krakenspot.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `lighter/lighter.exchange`, `mexc/mexc.exchange`, `okx/okx.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `polymarket/polymarket.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.forwarding.test.ts` | `backpack/backpack.exchange`, `binance/binance.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `okx/okx.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.order-writes.test.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.valuation-streams.test.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.websocket-protocol.test.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/exchange-metadata-capabilities.test.ts` | `base`, `registry` |
| `packages/safe-cex/src/__tests__/audit-regressions/external-public-data.liveness.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/fill-event.incremental.test.ts` | `asterdex/asterdex.exchange`, `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-fill-p0-regressions.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-update-order-p0-regressions.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt.instrument-metadata-cache.audit-regression.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.cloid.test.ts` | `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.hip3-startup-budget.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.market-data-normalization.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.mismatched-api-wallet.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.schedule-cancel.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.spot-markets-merge.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.terminal-cancel.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.tif-and-reduce-downsize.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.configuration.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.contract.test.ts` | `base`, `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.order-recovery.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.private-recovery.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.public-snapshot.test.ts` | `bybit/bybit.exchange`, `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.signing.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kraken.remaining-audit.test.ts` | `kraken/kraken.exchange`, `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.client-order-id.audit-regression.test.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.native-amend.audit-regression.test.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.algo-update-rejection.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.book-buffer-gap.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.cancel-replace-contract-size.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.cancel-symbol-v3.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.normal-order-rejection.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.remaining-audit.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/lighter.public-data.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/lighter.remaining-audit.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/lighter.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.audit-regressions.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.depth-sequence.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.order-reconciliation.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.trading-reference.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx-full-depth.audit-regression.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx-rust-oracle.audit-regression.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.conditional-orders.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.monthly-candles.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.orderbook-unsubscribe.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.remaining-audit.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.sequenced-orderbook-adoption-gate.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.split-remainder.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.websocket-routing.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okxdemo.transport.test.ts` | `okx/okxdemo.exchange`, `okx/okxeudemo.exchange`, `registry` |
| `packages/safe-cex/src/__tests__/audit-regressions/okxeu.xperp-markets.test.ts` | `okx/okxeu.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/order-payload-normalization.test.ts` | `binance/binance.exchange`, `bitget/bitget.exchange`, `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.rust-transport.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.batch-cancel.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.batch-orders.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.candle-fences.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.empty-history-cursor.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.execution-stream.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.followups.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.full-depth.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.heartbeat.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.history-auth-readiness.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.native-amend.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.opening-trigger.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.sbe.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.single-order-client-id.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.stop-loss-order-types.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.get-account.audit-regression.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.margin-type-flat-position.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.order-write-routing.audit-regression.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.positions-balance.audit-regression.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.remaining-audit.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.rust-public-parity.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.market-positions.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.safety-state-placement.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.websocket-contract.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/private-websocket.auth-resubscribe.test.ts` | `bitget/bitget.exchange`, `bybit/bybit.exchange`, `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/reconciliation-fetch-results.test.ts` | `apexomni/apexomni.exchange`, `hyperliquid/hyperliquid.exchange`, `lighter/lighter.exchange`, `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust-frame.inverse-units.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.apexomni-wrapper.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.asterdex-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.client-order-id-rules.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.default-wasm-loader.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.kraken-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.lighter-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.woo-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/shared-public-account-overlay.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `okx/okxeu.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/socket-diagnostics-summary.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/transport-remote-signing.audit-regression.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.conditional-orders.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.dynamic-tickers.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.order-errors.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.private-ws.audit-regression.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.public-kline.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.regular-batch-orders.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.tpsl-amend.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.account-info-readback.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.funding-refresh.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.legacy-list-building.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.orderbook-update.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.remaining-audit.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.rust-parity.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.websocket-routing.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/base.remote-signer.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/base.ws-auth-signing.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/binance-market-websocket.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/binance.tick.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/bitget.liquidation-price.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/bitrue.execution-history.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/blofin.contract-scaling-regression.test.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/__tests__/bybit.brazil-inverse-amend.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.conditional-order.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.execution-history.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/bybit.income-history.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/bybit.inverse-exchange.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/bybit.inverse-spot.startup-rejection.test.ts` | `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/bybit.market-startup.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.markets-failure.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.options-exchange.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange` |
| `packages/safe-cex/src/__tests__/bybit.private-socket-category-gate.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.startup-auth-log.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybitinverse.order-units.test.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/coinbase.rust.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/badLink.scenarios.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/binanceLookup.scenarios.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/bybitClockSync.scenarios.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/bybitLostCreate.scenarios.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/hyperliquid.exchange.startup.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/hyperliquid.hip3-market-loading.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/hyperliquidspot.balance.test.ts` | `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/__tests__/in-process-public-data.health.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/in-process-public-data.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/kucoin.exchange-docs.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/kucoinspot.exchange.test.ts` | `kucoin/kucoinspot.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.account-lookup.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.builder-fee.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.ohlcv-fallback.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.place-orders.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.signer-gate.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.startup.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/okxeu.validate-account.test.ts` | `okx/okxeu.exchange`, `okx/okxeudemo.exchange` |
| `packages/safe-cex/src/__tests__/orderly.rust.wasm.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/paradex.error-details.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts` | `base` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` | `base` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` | `base` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` | `base` |
| `packages/safe-cex/src/__tests__/rust.exchange.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/targeted-fill-arrows.execution-history.test.ts` | `apexomni/apexomni.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `lighter/lighter.exchange`, `okx/okx.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/transport-adapter-selection.characterization.test.ts` | `base`, `registry`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/transport-direct-ws.test.ts` | `bitget/bitget.exchange`, `grvt/grvt.exchange`, `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/weex.utils.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/woo.execution-history.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/createApiWithSecondaryKey.test.ts` | `base` |
| `packages/safe-cex/src/crossTrade/getCopierWeightRatio.test.ts` | `base` |
| `packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.test.ts` | `base` |
| `packages/safe-cex/src/index.test.ts` | `registry` |
| `packages/safe-cex/src/safeCexAxiosCreate.test.ts` | `base` |
| `packages/safe-cex/src/telemetry.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/transport/direct.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/transport/engine.test.ts` | `base` |
| `packages/safe-cex/src/webworker/ExchangeWorker.startup-phase.test.ts` | `registry` |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.terminate.test.ts` | `base` |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.test.ts` | `base` |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/BaseHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/BybitTickerSubscriptions.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/webworker/handlers/CallMethodHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/ChaserOrderHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/CommandHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/DevHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/OHLCVSubscriptionsHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/OrderBookSubscriptionsHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/TwapHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/UiStateHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/UpdatePositionsHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/postProcessors/ChaserOrderPostprocessor.test.ts` | `base` |
| `packages/safe-cex/src/webworker/postProcessors/MessageHandlerStatsPostprocessor.test.ts` | `base` |
| `packages/safe-cex/src/webworker/postProcessors/TwapPostprocessor.test.ts` | `base` |
| `packages/safe-cex/src/webworker/twap/TwapInstance.test.ts` | `base` |
| `packages/safe-cex/src/webworker/twap/TwapInstance.timeout.test.ts` | `base` |


## Appendix C: frozen oracle dependencies and their replacement policy

At the Coinbase/Orderly audit checkpoint, resolving the frozen root found **32 direct class-importing caller files**:
11 Coinbase audit files (including the two client-ID suites already listed in
Appendix B), 10 Orderly audit files, three Coinbase reference subclasses/socket
types and eight Orderly reference broker subclasses/socket types. Appendix B
now names all 32 exact class targets, adding the 30 previously absent callers
and extending the existing two client-ID entries. These counts are test/reference
source counts for that checkpoint; the current runtime totals are 49 names / 48 Rust aliases.

That checkpoint's Coinbase/Orderly frozen-module graph below additionally includes signing/API/type/
utility imports: **79 static imports in 40 caller files** at this review
checkpoint. TxFlow's later frozen-reference additions are recorded separately below. Full target paths avoid confusing frozen classes with removed
production implementations. Each row remains test-only. Keep these fixed
oracles while the replay and audit tests need them; their replacement is a
reviewed Rust-backed test using independent documented inputs, or deliberate
retention in `__tests__/reference`. Do not reintroduce them into production,
delete their assertions implicitly, or replace TS-vs-Rust evidence with
Rust-vs-itself. Broker subclasses inherit the fixed Orderly reference rather
than selecting a production registry adapter. Coinbase spot inherits its fixed
Coinbase reference, and socket type imports depend on that same class.

| Exact caller path | Frozen reference targets (repository-relative) |
| --- | --- |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.forwarding.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.account-identity.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.candle-handler-guard.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.cfm-private-ws-contract-size.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.ohlcv-stats-cleanup.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.open-orders-polling.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.place-order-response.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.private-ws-auth.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.rest-signing.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbasespot.spot-only.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/no-retried-writes.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.cancel-algo-orders.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.expected-order-rejections.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.isolated-margin.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.leverage-clamp.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.market-leverage-limits.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.orderbook-snapshot-retry.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.private-ws-fill-mapping.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.startup-leverage-error.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.trailing-stop-rejection.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.orders.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.orders.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/kodiakfi/kodiakfi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/mode/mode.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/salsadex/salsadex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/what/what.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/woofi/woofi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.encryption.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.utils.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-private.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.encryption.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.utils.ts` |

Source-inspection strings in `coinbase.starbase-private.test.ts`,
`coinbase.starbase-ws-session.test.ts`, `coinbase.starbase-public-frames.test.ts`,
`coinbase.starbase-cutover.test.ts`, `coinbase.starbase-session.test.ts` and
`coinbase.starbase-orders.test.ts` (all under
`packages/safe-cex/src/__tests__/audit-regressions/`) read the frozen Coinbase
source with file reads; they are not static class imports. Their exact
paths and source assertions must be updated deliberately if the frozen reference
moves. `packages/script-api-core/src/adapter/orderEntryPoints.test.ts` also
names the frozen Coinbase class path to verify that test/reference code is
classified as test source. These source-policy assertions survive independently
of runtime venue deletion.


## Appendix D: TxFlow frozen reference and retained helper

TxFlow now defaults to Rust in both production registries. The removed
`src/exchanges/txflow/txflow.api.ts` and `txflow.exchange.ts` paths are not
production callers. The implemented test oracle lives in
`src/__tests__/reference/txflow`; `sdk-rs/harness/venues.mjs` dynamically loads
its `txflow.exchange.ts` for independent replay. Keep that reference until its
oracle and audits have an explicitly reviewed replacement, without changing
TS-vs-Rust comparisons into Rust-vs-itself.

The current TxFlow-only static frozen-module graph has **27 distinct module
edges in 13 caller files** (type imports included):

| Exact caller path | Frozen reference targets (repository-relative) |
| --- | --- |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.account.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.history.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.public.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.rust-wrapper.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.signing.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.signing.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.socket-pacing.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.api.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.api.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.signing.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |

`src/exchanges/rust/rust.exchange.ts` imports the production
`src/exchanges/txflow/txflow.client-id.ts` rule; the frozen
`txflow.trading.ts` also imports/re-exports that helper. It is an active
shared dependency, not a deleted venue implementation. The frozen
`txflow.signing.ts` imports Hyperliquid's TS action-preimage helper, so preserve
or deliberately extract that helper before deleting the Hyperliquid TS tree.
The reference also retains its normal BaseExchange/BaseWebSocket, API-factory
and transport dependencies. Generic consumer fixtures do not authorize deleting
those oracle dependencies or enabling measurement-gated order controls.

### AsterDEX bounded fill reads and flat events

Rust AsterDEX callers can request fill ranges longer than seven days; limit is a
per-page native limit and no longer caps the final bounded range. Native rows
are deduplicated across boundaries, then cumulative PnL is normalized over all
orders before an optional order filter. V3's search precedence cannot override
caller time bounds. Saturated timestamps and incomplete reads fail visibly.
Closed hedge legs can omit entry price when their amount is explicitly zero;
other exposure and balance provenance remain strict. Shapes are unchanged.

### Orderly port-review display/read corrections

Orderly `fetchOHLCV` callers supplying `from`/`to` now receive native history
within those bounds rather than a locally filtered latest-only window. Consumers
keep the same candle shape. Order lists/chart overlays see a real trailing
trigger derived from native extreme/callback value or rate: BUY adds, SELL
subtracts. This corrects TS's BUY/rate display, with no write-contract change.

### ApeX Omni decimal remaining and candle row isolation

Order displays/default amendments now preserve native decimal remainder (for
example0.3-0.2 is0.1) without snapping to market amount precision. REST chart
reads keep sorted valid candles when a typed zero-price row occurs and emit a
coded warning; malformed or wholly unusable responses fail. Caller signatures,
order IDs and explicit leverage-write semantics remain unchanged.

### BingX port-review metadata and stream recovery

Rust BingX `updateSymbolMetadata(symbol)` now reads native leverage and margin
mode for flat symbols and publishes them together when the read remains current.
Revision/query-generation races and reads issued during a pending settings write
prevent stale store publication; they still return current cached buy/sell rails,
with an independent validated-read fallback for each missing rail. Overlap alone
never fails the read or produces `null`; transport failures and invalid native
leverage responses can still fail the read. A read that is no longer current
when its leverage response arrives (including one issued during a pending write)
skips the margin-mode request. A read issued during a pending write stays
ineligible to publish even after that write settles. Current reads retain atomic leverage/margin publication.
`fetchLeverageForSymbol` and selected metadata refreshes keep their existing APIs.
No extra polling or write retry is added. Malformed position-stream rows retain prior exposure
while valid peers update, followed by authoritative REST repair. Hedge leverage
and bracket trigger-source contracts remain as documented.

### Backpack nullable trigger quantities

Rust readbacks accept missing/null base quantity only when native trigger fields
prove a positive amount and consistent execution. Orders remain visible and
repairable under their real IDs; malformed ordinary quantities preserve prior
state and fail rather than creating zero-size rows. This applies to account and
order history reads and repair acknowledgements without changing method shapes,
placement units, WS fields or cancellation identity.

### Lighter port-review confirmation and failures

A Rust Lighter reduce-only limit order without matching known exposure now waits
for account-read completion, up to10 seconds, before any nonce/signing request.
Unconfirmed, failed or timed-out reads refuse the unsent leg; a prior accepted
leg remains reflected in the aggregate outcome. Late reads cannot release later
queued orders. Both Lighter deployments keep their existing placement units and
result shapes. HTTP405/429 include neutral guidance with native diagnostics;
explicit invalid-margin21613 rejection refreshes state without changing cached
mode by guess or automatically replaying a placement.


### Bitrue private conditional display

Newly observed Bitrue conditional orders use native conditionOrder/triggerType
for SL/TP classification even when execution type is MARKET/LIMIT. Existing
protective child identity, direction and trigger reference remain authoritative.
Order overlays and protective controls receive corrected kinds/prices with no
method, quantity, placement or cancellation contract change.

### Phemex explicit index triggers and daily cutoff

Explicit SL/TP index-price sources now produce the documented `ByIndexPrice`
wire value instead of native refusal or TS's mark/last fallback. Default sources
are unchanged. OHLCV consumers retain the caller's daily cutoff; Rust deliberately
keeps the latest valid daily bars that TS's prior-day shift omitted. Shapes and
caller signatures are unchanged.

### Backpack REST book queries (#1680)

The existing SDK `fetchOrderBook(symbol, limit?)` now dispatches an unsigned
Backpack `GET /api/v1/depth?symbol=<encoded caller symbol>` with the existing
broker header. The native query needs neither cached markets nor a live
subscription. The shared SDK capability-readiness behavior remains: an existing
pending startup is awaited, and no-start/failed-start or terminal-engine errors
must still be handled. An unseeded sans-I/O native test does not establish
pre-start SDK readiness.

Caller limit is ignored as in TS. Both sides are required arrays of two-string
levels; server order, duplicate prices, lexical price-string IDs and zero
quantities are retained. Each level has `total: 0`; no `lastUpdateId` is exposed.
Empty or one-sided arrays succeed, while malformed structures or invalid/nonfinite
financial rows reject the whole query. Callers keep ordinary typed error
handling, with `notSent` public failures and non-auth classification even for
credential-looking endpoint refusals. Usable HTTP429 Retry-After metadata uses
the existing shared one-hour cap; no automatic failed-query retry is added.

REST success/failure completes its own command without publishing or repairing
the subscribed book. Native regressions preserve the next WS delta and an
outstanding query across unsubscribe/resubscribe. Keep subscription readiness,
sequence continuity and disconnected-book invalidation as separate contracts.
No direct default NX app/SDK CLI/MCP snapshot producer was found; Backpack's TS
socket seed uses its own XHR rather than this method. Manual/third-party SDK
query parity remains supported without a new caller argument.

### Crypto.com REST book queries (#1685)

The existing SDK snapshot method now uses an independent unsigned live/UAT
`GET /public/get-book?instrument_name=<encoded raw symbol>&depth=50`.
Caller limit is ignored. Native requests need no market seed, credentials or
subscription; the existing SDK capability-readiness rules still apply.
Native side order, duplicate prices, lexical price-string IDs and zero
quantities remain, with `total: 0` and no `lastUpdateId`. Only the first book is
mapped; trailing tuple metadata and later books are ignored.

Native code0 is required. A present result takes precedence over outer data;
missing/null/empty data or a null first entry produces an empty book, while an
explicit null result, malformed non-null structures or invalid/nonfinite
financial rows reject the whole query. Callers retain ordinary typed query
failure handling: public credential-looking refusals are non-auth `notSent`,
and usable rate-limit hints retain the existing shared cap without adding a
failed-query retry. REST completion leaves subscribed depth, ticker/account
state and subsequent WS updates intact. No default direct NX/CLI/MCP snapshot
producer or new caller argument was established by the bounded audit.

### GRVT REST book queries (#1686)

The existing SDK snapshot method now uses an unsigned public MDG POST on
live/testnet roots. A known market supplies its native catalogue ID in
`{i: nativeId, d: 50}` at `/lite/v1/book`; caller limit is ignored. This
corrects TS's `instrument_id` request against the
[official lite schema](https://api-docs.grvt.io/market_data_api/#orderbook-levels)
without changing caller arguments. An unknown market resolves an owned empty
book without HTTP. Existing SDK capability-readiness rules still apply.

Full/lite wrapper, side, price and size aliases retain server order, duplicates
and zero sizes. Each level's `total` is cumulative base amount on its side;
there are no level IDs, update ID or decimal/contract rescaling. Missing/null
results or sides yield empty arrays. Malformed non-null objects/rows,
invalid/nonfinite financial values, an explicit null native code or cumulative
overflow reject the whole query.

Requests carry no private cookie/session or signing. Public credential-looking
and native1000 refusals remain non-auth `notSent`; native1006/HTTP429 carries
usable bounded rate metadata. Existing network-failure handling permits at
most five read reissues, while native/HTTP refusals do not retry. Query
success/failure does not publish streaming depth, satisfy stream readiness or
replace private session/builder state. The REQUIRED GRVT structural
builder-error migration above remains unchanged. No default direct NX/CLI/MCP
snapshot producer or new caller argument was established by the bounded audit.

### Paradex REST book queries (#1687)

The existing snapshot method now sends unsigned live/testnet
`GET /orderbook/<encoded raw symbol>?depth=N`, defaulting to depth20 and
forwarding an explicit caller depth unchanged. Native queries need no
catalogue seed, credentials, JWT or subscription; existing SDK
capability-readiness rules still apply.

Required bids/asks arrays may be empty or one-sided. Their first two string
tuple fields are price and base amount; trailing metadata is ignored. Server
order, duplicates and zero quantities remain. Each level's `total` is
`price * amount` in quote units, not cumulative. No level IDs, update ID or
contract/decimal scaling is added. Malformed rows, nonfinite financial values
or quote-product overflow reject the entire query.

Public auth-looking refusals remain owned non-auth `notSent`; structured
code/message is bounded and usable HTTP429 hints retain the shared cap.
No automatic failed-query retry is added. REST completion/failure leaves
live SBE/JSON book sequencing, account/JWT state and native trading quote
admission unchanged. The bounded audit found no default direct NX/CLI/MCP
snapshot producer; the existing TS method's explicitly capped Market/IOC
quote path is recorded separately above and does not become a new caller
feature through this query implementation.

### WEEX REST book queries (#1690)

The existing snapshot method now sends unsigned
`GET /capi/v3/market/depth?symbol=...&limit=N`, using the encoded raw symbol,
default limit200 and explicit limit unchanged. Native queries need no catalogue
seed or private credentials; existing SDK capability-readiness rules apply.

Sides use `bids ?? b` and `asks ?? a`; missing/null sides become empty arrays.
Preserve server order, duplicates, zero quantities and cumulative base-amount
totals, without contractVal scaling or level IDs. `lastUpdateId` retains its
exact string, including integers beyond u64; missing/null yields an empty
string. Malformed non-null shapes, financial fallback/prefix input, nonfinite
values or cumulative overflow reject the whole query. This tightens permissive
TS financial parsing without changing method arguments. Native code0/200/absent
is accepted; malformed explicit code is rejected.

The query shares existing GET pause handling. HTTP429 keeps header/window delay
precedence and the 60-second bound; later queries may defer with their owner
intact, while the failed query is not retried. Native/HTTP/transport failures
remain owned non-auth `notSent`. REST completion/failure does not publish
subscription depth or replace streaming generation, ticker/account,
metadata/settings or fill-completeness state. Existing TS internal snapshot
seeding/gap repair is separate; the bounded NX/CLI/MCP audit established no
direct default producer or new caller argument.

### Hyperliquid signed cancellation scheduling (#1693)

The existing perpetual `scheduleCancel(time?: number | null): Promise<void>`
now dispatches a signed native control, with `scheduleCancel` at appended
capability flag bit25. At #1693 it also enabled hostClock on the perpetual
engine, affecting all perpetual work/signing rather than only scheduleCancel.
**#1709 supersedes that opt-in:** the flag is removed, bit14 is reserved, and
the default SDK synchronized clock now applies to every venue, including spot.
Upgrade paired SDK/host artifacts; explicit injected clocks remain authoritative.
Native spot scheduleCancel still returns unsupported/`notSent`; later capability
positions, including bit25, are unchanged.

To arm, pass a safe integer absolute UTC timestamp in milliseconds at least
five seconds beyond synchronized current time. SDK validation precedes JSON
and worker forwarding; the mirror forwards the owned call without starting
another engine. Direct `EngineHost.command` also rejects unsafe/negative input
before JSON can turn NaN/Infinity into a clear. That host guard does not itself
enforce the five-second bound; native admission and immediately-before-signing
checks do. A deadline can expire in the existing paced FIFO and then fail
`notSent` before signing. Existing network/vault-aware signing and the shared
monotonic nonce remain. Invalid/missing signing credentials or public-only mode
cannot dispatch even a clear.

Null or omitted time clears by signing an action without `time`. Do not coerce
an invalid input or failed arm into a null/omitted retry. The SDK returns void
only after the owning accepted control completion. The implementation accepts
HTTP2xx `status: ok` with response `type: default` and absent/null data or an
empty statuses array. This strict ACK shape is derived from the SDK model;
no live schedule ACK capture validates it. A structured `status: err` with a
string response on a non408 HTTP status below500 proves `rejected`; HTTP408/5xx
and other malformed/unproven ACKs remain `unknown`.

Transport loss, command timeout, host disposal and worker interruption preserve
unknown write outcomes. No failed schedule write is replayed, late completion
cannot revive its retired owner, and account/order snapshots do not prove an
arm or clear. There is no automatic lifecycle arm/clear, renewal or application
watchdog. Caller strategies own renewal, deadline slack and uncertainty
handling. The bounded audit found no default NX/CLI/MCP schedule producer or
renewal loop; manual and third-party SDK callers still own this contract.

### Hyperliquid account/vault fee tiers (#1701)

Native perpetual `userFees` now uses the same subject as other account reads:
configured `vault` first, otherwise `applicationId`. It no longer substitutes
the API agent/signer address. For unresolved private identity, the existing
signer-address `userRole` read must establish identity before queuing fees.
Failed, malformed or timed-out resolution does not query an unresolved fee
subject; late/duplicate replies cannot bootstrap fees again. Configured/resolved
fees remain queued before mode/account reads. Valid fee data is not a new
startup or readiness gate.

The maker/taker response fields still update native market rates and SDK
`Market.makerFee`/`takerFee`. The anonymous public/no-identity path and its
422/default-fee fallback remain; the native spot branch is unchanged. There is
no new fee retry, caller option, transport/signing contract or fee arithmetic.
Initial defaults or market readiness do not prove a successful account-tier
query.

Existing NX fee estimates consume these fields in
`apps/web/src/components/placeOrder/classicOrderForms/MarketOrder.tsx:510-513`
and `LimitOrder.tsx:662-665`; the order-line fee suffix uses them in
`apps/web/src/components/chart/broker/lineManagers/OrderLineManager.ts:626-642`.
No new argument or serialization field is required beyond adopting/delivering
the corrected SDK engine. At the read-only NX audit pin, TS
`packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts:1405-1439`
still sends the signer key for fees; that analogous defect is not changed by
#1701 and remains owner-managed. These source observations do not prove a
deployed NX session uses corrected rates. B8 stays closed; separate delivery,
caller migrations and evidence limits remain.

### MEXC port-review snapshot and plan corrections

Rust MEXC `fetchOrderBook(symbol, limit)` now returns an independent public REST
snapshot without requiring a feed subscription. Existing callers keep the same
shape: sorted bids/asks with base-asset `amount`, cumulative base `total`, and
an exact string `lastUpdateId`. Default depth is 100; the SDK accepts 1–1000
levels. Query results never replace subscribed books. Rate limits fail with a
retry delay; callers choose whether to retry. Public endpoint refusals cannot
invalidate private account credentials.

#1681 repairs standalone MEXC JSON admission for that existing implementation.
Its engine dependency now enables the precise `command-order-book` decoder
introduced by #1680, which retains AsterDEX/Polymarket admission. No MEXC mapper,
capability, limit, financial unit or HTTP/auth/rate policy changes here. The new
standalone regression decodes actual JSON into an unsigned native depth GET,
retains version `9007199254740993` and base amounts, and leaves the live book
absent. The family-artifact test separately probes the advertised command in
isolated builds; its returned sans-I/O requests are not dispatched. Neither
proof establishes NX runtime adoption or changes the TS listener's internal
snapshot-seed path described above.

Plan orders now expose the native reduce-only flag or closing-side semantics;
opening plans no longer appear reduce-only. Position stops stay reduce-only.
Order overlays and reduce-only sizing consumers receive corrected flags; no
placement units or signatures change. Native fill-fee sign remains unchanged
pending independent debit/rebate evidence.

### Coinbase port-review retained read semantics

Coinbase Rust spot currency balances continue using native total/free and their
held difference; chart reads continue honoring an explicit from bound. These
intentional corrections to the TS reference are not parity defects. CFM summary
unrealized-PnL/equity aggregation remains an explicit accounting risk pending
portfolio and downstream consumer evidence. This revalidation changes no API,
selector, deployment status or runtime behavior.

### WOO port-review warnings and confirmation

Rust WOO keeps isolated exposure visible and logs one coded warning per symbol
that isolated margin settings cannot be changed. Exhausted leverage readback
reports an accepted write with unconfirmed setting and Unknown outcome, keeping
cached settings until authoritative confirmation. No write is replayed and no
unconfirmed success is returned. Caller method signatures and quantities remain
unchanged; native algo reduce-only defaults and protective trigger semantics stay
as documented.

### Bitunix catalogue and ticker readiness (#1636)

A usable nonempty catalogue now begins private initialization while its ticker
seed is still pending or retrying. Public ticker subscriptions still wait for a
usable ticker seed. Ticker recovery does not reset private settings, restart
private initialization or replay trading writes. Private I/O starting does not
prove completed ticker/public readiness or successful completion of SDK `start()`.

Typed malformed catalogue/ticker rows emit Warn `row.skipped` while valid peers
commit; eligibility/API-support filtering remains. Empty, filtered-only or
all-unusable required seeds retry instead of publishing loaded-empty readiness.
Malformed envelopes or truncated streams do not publish their valid prefix.
Consumers can therefore see a usable partial catalogue and delayed tickers;
row skipping is not proof that every venue row was refreshed.

Separate catalogue/ticker retries use the shared capped backoff and server/native
rate-limit floors. Public discovery diagnostics retain parsed venue fields and
sanitized messages instead of raw response bodies. Extend the required
code-based degraded/recovered visibility below to Bitunix too; these retrying
public reads are not terminal startup failures. Private/history strictness and
caller arguments/results remain unchanged.

### Bitunix disconnected books (#1639)

Private, futures and public ticker-shard feeds retain independent socket
supervisors across reconnects, with capped jittered backoff and a bounded connect
attempt. Retired messages, opens, closes and queue/login timers cannot affect a
replacement socket. Current subscription interests survive backoff and replay
once at the replacement open; callers should not replay trading writes to recover
public data. Outbound pings do not count as received traffic. The existing private
silence compatibility remains until a real pong is observed.

Retiring the **futures socket** immediately clears its subscribed books and emits
empty BOOK updates, before retry or reopen. Reopen alone does not restore depth;
only fresh current-socket data does, and that snapshot can have one empty side.
The public ticker shards carry separate ticker channels; retiring one does not
invalidate books on the still-live futures socket. URLs, channel/feed routing,
book units and the wire format are unchanged.

**Required cutover caller change, high priority: prevent stale displayed depth
after disconnect.** At NX `65e086a3c90`, the raw web listener queues book updates
(`apps/web/src/utils/market/listenOrderBook.ts:415-422`, batched storage at
`:52-96`), but its consumer callback drops both-empty books (`:562-564`). The
canvas also requires both sides to be nonempty before drawing
(`apps/web/src/components/orderbook/Orderbook.tsx:1465-1471`); clearing the main
canvas happens inside that skipped draw (`:1335`). Therefore the old displayed
levels can survive an explicit invalidation. This is static source evidence,
not a deployed-runtime reproduction.

Smoothing must preserve the same invalidation: `listenOrderBook.ts:558-560`
prefers the smoothed atom over raw data; `components/componentWorkers/OrderbookSmoothWorker.tsx:43-52`
writes it only through the smoother, and `components/orderbook/useSmoothOrderbook.ts:80-81`
returns without emitting when either current/previous side is empty. Propagate
empty snapshots immediately, reset interpolation and cached rendered depth, and
clear the canvas or show an explicit unavailable state. On fresh one-sided depth,
render the available side or keep an explicit unavailable state without retaining
old opposite-side levels. Do not interpolate across a disconnect.

Required caller proof covers a seeded book followed by futures disconnect,
immediate empty invalidation through worker/hosted delivery and both smoothing
settings, no stale rendered levels during backoff/reopen, and fresh one-sided then
two-sided recovery. Source `sdk-rs/venues/bitunix/tests/socket_lifecycle.rs`
already verifies immediate empty BOOK publication, retired-epoch exclusion,
replay once and one-sided recovery; it does not prove NX rendering. NX remains
read-only in this docs lane; the caller repair and runtime proof are still open.

### Bitunix secondary GET identity and egress (#1640)

Existing SDK `secondary_key`/`secondary_secret` options now reach Bitunix through
registry `readCredentials`. A signed GET uses that identity when configured;
otherwise it uses the primary identity. The SDK recognizes the `api-key` header
so GET signing identity and secondary proxy/HOW egress agree, including the
existing native serialized routing policy. Mutations, reads issued as POST and
private WS login continue using the primary identity/egress; public discovery
remains unsigned. No caller argument or host runtime path is added.

Formed secondary credentials require a nonempty key and secret before startup
I/O, including public-only construction. Account balance/position/order/TP-SL
reads and symbol settings tag the credential slot actually selected and their
existing scope. History, public discovery and trading requests do not create
account-auth evidence. #1640 added tagging only; #1662 now supplies the scoped
native success/refusal classifier described below. Unrelated successful traffic
still cannot clear a credential badge.

### Bitunix scoped account evidence and recovery (#1662)

Current signed balance, positions, regular/TP-SL orders and per-symbol settings
GETs now emit the existing typed account-auth outcome for the credential slot
actually used. HTTP 200 with strict native code `403`, `10003` or `10004`
proves Rejected. Bare HTTP 401/403, signature/nonce/timestamp codes
`10007`/`10010`/`10011`, proxy HTML, malformed responses and transport failures
do not prove invalid credentials. Acceptance requires native success and the
required valid balance/settings object or complete list, including an empty
list. Skipped financial rows can coexist with Accepted; this does not establish
snapshot completeness or initial readiness.

Regular and TP-SL endpoints share the orders scope while retaining each
endpoint's refusal; one sibling cannot heal the other. Settings likewise retain
refusals per symbol, so one successful symbol cannot clear another's rejection.
Callbacks failing the currentness checks cannot heal current auth evidence
or reset/extend its failure backoff. Since #1665, ordinary balance reads remain
current across WS-only generation changes; #1668 similarly keeps per-symbol
settings current during position-ID notifications. Attempt serials and the
respective accepted-write revisions still fence both paths. The SDK already
consumes this typed channel for isolated
credential/scope streaks and polling policy; custom hosts must use it rather
than infer credential state from diagnostic text.

Sanitized `request.failed` events report the failure, with Error severity for
strict credential refusal and Warn otherwise, and available `kind`,
`venueCode` and `retryAfterMs` fields. These account-read diagnostics do not emit
legacy `auth.rejected`, which would invent primary/account evidence for a
secondary or narrower scope. The existing private-WS login diagnostic is a
separate path; #1662 does not reclassify login, history or trading responses.

Each account axis/settings symbol uses the existing jittered backoff with
nominal one-second through thirty-second delays. HTTP 418/429 or native
`10005`/`10006` imposes a shared account/settings-read pause of at least sixty
seconds, honoring a longer numeric Retry-After up to one hour. New interests,
polling, debounce, lost-read repair and preflight cannot bypass that pause;
a sibling success cannot clear it. Matching validated success resets only its
read source. Financial row admission and snapshot fences remain in force.

A prerequisite blocked by recovery fails its command once with `notSent` while
ordinary background repair remains scheduled. Repair cannot resurrect the
retired command or replay a POST. Callers should retain normal typed failure
handling and let SDK/native read recovery run; no new option, constructor field
or caller-issued resync is required. This read policy does not add a trading
retry.

### Bitunix balance progress during notifications (#1665)

Ordinary balance reads now use the current serial and a separate accepted
position-mode write revision, so current REST balances and scoped credential
evidence advance despite intervening WS notifications. Notifications retain
the first one-second repair deadline; they do not replace financial REST state.
Command-owned admission keeps its stricter generation check. An accepted mode
ACK fences older balance responses for financial publication, auth evidence and
recovery resets. Refused, malformed or transport-lost ACKs do not establish an
accepted mode change.

Ordinary issuers, including reconnect refreshes, coalesce while a balance read
is younger than ten seconds from its own actual dispatch. Lost-read replacement
and late old completion/failure keep the newer serial owned. Synchronous signing
failure releases its serial before ordinary repair, avoiding a fabricated
ten-second in-flight wait. #1662's retries and shared rate pause still gate
issuance before request allocation; notification repair cannot bypass an
outstanding failure backoff. Balance mapping, polling controls, caller signatures
and write retries stay unchanged. Consumers can see fresher balances and scoped
auth outcomes under busy private traffic; no caller-issued resync is required.

### WEEX selected-symbol metadata refresh (#1675)

Native WEEX now selects the shared `onReadAndMetadata` policy instead of
`onRead`. With a loaded capability record, the private SDK hook
`updateSymbolMetadata(symbol)` dispatches `fetchLeverage` even when rails are
cached or the symbol has no position. Repeated hooks and explicit
`fetchLeverageForSymbol` calls read fresh configuration. Malformed responses
reject without replacing the last good rails; a later valid refresh can recover.

The existing hook returns without private I/O for public-only accounts or when
the capability record is unavailable. It does not itself wait for startup, so
callers should await worker-aware capability readiness before requesting the
refresh and retain ordinary rejection handling. No argument change or
caller-issued cache invalidation is needed. The three actual-WASM regressions
cover repeated cached/flat refresh, malformed retention/recovery and public-only
isolation; they do not establish NX runtime adoption or close other B8 gaps.

### Bitunix per-symbol settings progress (#1668)

Flat-symbol leverage/margin metadata can now advance during continuous position
notifications, including traffic on another symbol. Response publication,
scoped auth evidence and retry/reset decisions share the current symbol attempt
serial and accepted-position-settings write revision. An accepted leverage,
margin or mode ACK supersedes older metadata; refused, malformed or
transport-lost ACKs do not establish an accepted change.

Canonical-market admission, selected credential tags, grouped settings
refusals, bounded backoff/shared rate pauses and the existing empty margin-mode
fallback remain. This improves metadata freshness without proving complete
account state. Command-owned/private prerequisite generation fences remain
strict, and no write is replayed. Existing caller signatures and readiness
guidance apply; no caller-issued resync is required.

### Engine-owned financial snapshot admission (Binance and Bitunix)

Native order snapshots now use one Store-owned write-freshness token instead of
Binance/Bitunix local order revision counters. Older in-flight reads cannot
apply native terminal/filled/bootstrap/binding metadata before being admitted.
Binance keeps post-ACK lagging-server and native identity protections. Bitunix
still publishes healthy REST peers beside newer WS rows, while writes invalidate
whole older reads; both complete initial endpoint tokens are admitted together.
No FILLS quantity, wire ID, snapshot retention, command retry or SDK/worker call
shape change is intended. Permanent exhaustion sends no unfenced snapshot read
and rejects an owned prerequisite once, without disabling accepted native writes.

Rust Venue implementers receive an added read-only `&Store` argument in
`account_auth_outcome`; the four built-in overrides and native/engine fixtures
are updated together. Auth observation still precedes account callbacks and
retains current-read, expected-list, credential-scope and retry checks. There is
no new NX/CLI/MCP argument or caller migration. Safe late ACK patch adoption and
other snapshot consumers remain follow-ons; see
[RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md](RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md).

### Bitunix private snapshot progress and retention (#1641/#1643/#1645)

Ordinary position reconciliation now progresses during continuous WS position
notifications, which update routing IDs rather than replace REST financial state.
An active read keeps its serial; activity coalesces a follow-up instead of
continually postponing or superseding that read. An unanswered read is eligible
for replacement after 10 seconds, and a late old response/failure cannot retire
the replacement. Accepted leverage, margin and mode acknowledgements still fence
older snapshots through a separate write revision. Command-owned preflight keeps
the full generation/serial checks; this change adds no write retry.

For complete successful REST envelopes, typed malformed position rows emit Warn
`row.skipped` and retain the known symbol's cached hedge legs, settings and REST
position IDs while healthy peers and safely absent symbols progress. An unknown
identity protects absence removal, while an admitted explicit zero can still
close its own position. A known rejection quarantines valid same-symbol
duplicates in either input order; bounded rejection-key overflow conservatively
holds the scope. Partial mode disagreement retains the whole previous snapshot
instead of mixing incompatible position modes. Held rows do not republish their
settings as fresh observations.

Regular and TP-SL REST snapshots isolate malformed rows by raw native order ID.
Retained families include the regular parent with attached brackets, or the
standalone TP/SL siblings, plus their native cancel/amend routing metadata.
Unknown identity holds absence only within that endpoint's scope. An admitted
raw parent can still replace its old family and remove omitted children; a
rejected duplicate quarantines that raw family. Only admitted groups advance
execution baselines, preserving held rows' pending fill accounting.

Consumers can therefore see newly admitted peers beside retained older rows;
row skipping does not prove a full refresh. Partial initial position/order
snapshots do not establish readiness. Initial orders require both current
regular/TP-SL serials and write revisions; command-owned prerequisites also
require the global generation checks (#1658). A partial command-owned read can update
healthy UI peers but cannot approve a trading prerequisite: the command rejects
without sending the dependent write. Malformed/failed envelopes retain the
previous snapshot rather than publishing a parsed prefix. Keep readiness and
existing command-error handling; method signatures, quantities and worker error
fields are unchanged. Subsequent #1655/#1658 address order/TP-SL WS scheduling
and keyed reconciliation; see [Bitunix order reconciliation](#bitunix-order-reconciliation-1655-1658).

### Bitunix order reconciliation (#1655, #1658)

Regular and TP-SL REST reads now have independent serials and ten-second
lost-read deadlines. Ordinary refreshes coalesce around an active read, and later
notifications do not postpone the first one-second repair deadline. Only the
matching completion/failure releases its token, including synchronous signing
failure; a lost endpoint is replaced independently of the other endpoint.

Ordinary snapshots use the endpoint serial and a separate command-mutation
revision rather than discard every peer after any WS generation change. Newer
validated WS native-order families, their routing metadata and terminal absence
survive older REST rows while healthy peers advance. Malformed WS identities
retain the affected raw family; unknown identity protects endpoint absence.
Bounded overlap coverage holds conservatively when exhausted. Superseded or
quarantined families do not consume execution baselines, while sparse unknown
execution updates can still obtain REST metadata for pending fills.

Command mutation revisions cover optimistic placement/cancel, actual write
dispatch, responses before partial-ACK error exits, and cleanup. Older snapshots
cannot reopen an accepted cancellation or restore a removed temporary order.
Command-owned prerequisites retain global generation/serial checks and the write
fence for store publication, even when the prerequisite rejects. Both complete
initial halves keep their WS overlap until atomic publication; malformed or
overflowed coverage cannot establish readiness. Caller signatures, quantities
and error fields are unchanged; no trading write is replayed.

Live `store.orders` can retire orders already reported terminal sooner than TS,
shifting subsequent array positions. Consumers must use order IDs and status,
rather than array index or a terminal-row retention window. At NX `65e086a3c90`,
`apps/web/src/utils/order/getOpenOrders.ts:16-17` filters open status and keys by
ID; its optimistic merge deduplicates by ID and removes closed/canceled rows
(`:149-150`). These reviewed helpers need no edit for earlier terminal removal.
This focused source check does not prove every downstream consumer.

#1655 adds exactly 44 private known-gate paths for terminal removals and their
array shifts, pinned to tape SHA
`e8d90edafa93fabcbd9cefb0e84be32a077399e0e3a2f4a906999d9d48a3850c`.
The archived CI comparison has identical common-ID rows at all 16 checkpoints,
13 command outcomes and seven writes; removed rows are closed/canceled. The 90
previous declarations are unchanged. Read counts differ, so this is not a claim
of identical transport receipts. Other fields/tapes remain checked; no comparator,
threshold, tape or integrity rule was relaxed.

### Bitunix host time and login controls (#1648/#1649)

At #1648 Bitunix enabled the existing `hostClock` capability. **#1709 now
supersedes that opt-in:** the capability is removed, bit14 is reserved, and one
shared SDK clock ingress supplies synchronized time to every venue. The bound
clock is read before startup and on later callbacks, including private socket
open; an explicitly injected clock remains authoritative. REST requests
and private login sign with that engine time using their existing timestamp units
and signing formats. SDK callers add no account field or constructor argument.
Venue server-time synchronization and signature-rejection recovery remain
separate decisions; this change adds no trading write retry.

Login controls are inspected before requiring `data`. An explicit typed boolean
`data.result: false` retains the existing Error `auth.rejected`. Missing, null or
malformed results now emit sanitized Warn `stream.malformed`, without echoing the
response body or inventing a credential rejection. Code-aware diagnostic
consumers must preserve that distinction rather than infer bad credentials from
the malformed-control warning.

Both unsuccessful forms use the existing two-second login retry policy on the same
private socket. Repeated controls coalesce without postponing its deadline. A
typed success clears the pending retry; socket retirement cancels it, and a late
timer from an old socket cannot send login on a replacement. Authentication
deactivation policy, private first-pong compatibility and trading write retries
are unchanged. This batch does not change the separate proxy-route or order
reconciliation scheduling contracts.

### Bitunix explicit WebSocket routing (#1650)

For engine transports with routing support, Bitunix `tealstreetWsRoute: true`
selects each available declared fallback as the forced host for public ticker
shards, private login and futures/business depth/candles. The first connection
and reconnect use that proxy; the normalized forced route has no further fallback
candidate. False/unset retains the existing two direct attempts before fallback
and recovery routing. The native serialized routing plan uses the same selected
hosts with its existing schema. Shared routing resolution for TS consumers and
other venues is unchanged.

The flag does not add routing support to a direct-only provider: that provider
keeps sockets direct and emits its existing unsupported-option warning. Callers
requiring proxy egress must select a routing-capable transport rather than rely
on the flag alone. REST identity/egress, feed identities, authentication metadata
and trading retries are unchanged.

NX already saves the same flag through web `apps/web/src/components/newApiFlow/useAccountForm.ts:171`
and CLI `packages/cli-core/src/commands/account/AccountsCommand.ts:210,257`.
Hosted `services/exchange-server/src/server.ts:950` forwards it, and the shared
`packages/safe-cex/src/createExchangeFromAccount.ts:130,187` maps it into engine
options (read-only `65e086a3c90`). No new caller field is needed; test configured true versus
false/unset through the selected transport, including first connection and
reconnect for all three channels. SDK transport and native routing fixtures prove
the route selection; deployed proxy reachability remains an operational check.

## Required startup degradation visibility

**Required cutover caller work; tealstreet-next remains read-only here.**
#1615 (`c63acfc19c2`) reports OKX/BloFin/Bitget public startup; #1636
(`62adf7651b0`) extends the same shared reporting to Bitunix. It emits
`startup.degraded` at Warn once after six failures of one required read, while
capped retries continue. Fields include `kind`, `read`, `attempts`, and parsed
`venueCode`/`venueMessage` when available, plus `retryAfterMs` for a positive
finite cooldown. After a prior degraded report, Info `startup.recovered` follows
usable markets and tickers, including usable partial catalogues; repair timers
continue. These use the existing Event wire and do not emit terminal
`StartupFailed`. Warn preserves SDK `validateAccount` behavior by avoiding its
Error listeners. Private/history failure policies, Bybit and post-loaded recovery
remain separate.

The caller must key on the codes, regardless of level. Keep diagnostic delivery
at an Info minimum so recovery is not filtered out; the engine defaults to Info.
Required visible state is independent of a terminal startup failure.

Read-only evidence at NX `65e086a3c90`, with the matching Rust SDK:

| Boundary / file:line | Current behavior and required cutover change |
| --- | --- |
| SDK `exchanges/rust/rust.exchange.ts:1307-1312`; `webworker/ExchangeWorker.ts:1248-1277` | The SDK emits `engine-event` and legacy delivery: Error uses `error` with `opts.engineEvent`; Warn/Info use `log` with event metadata as the third argument. Forward and consume the structured event, including its code and fields. |
| NX `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:2705-2720,1977-2011,2046-2074` | The worker switch forwards legacy error/log but has no `engine-event` case; the error handler ignores `opts.engineEvent`, while log only logs/reports. Add `engine-event` forwarding and code-based consumption. With Warn/Info, the new startup receipts take the log path and do not reach `handleErrWarning` or its toast path at all. |
| Same controller `:1770-1822,1838-1867,1895-1901`; `apps/web/src/atoms/settings.atoms.ts:466`; `utils/exchange/getExchangeAppearsConnected.tsx` | Error compatibility delivery logs before toast suppression, but the default `showErrorsWhileReconnecting: false` can hide a disconnected startup (and hidden-tab notices). A 30 s connectivity grace, never-initialised sockets and hosted mode can allow some toasts; they do not guarantee visibility. Maintain a visible per-exchange degraded state keyed on `startup.degraded`, independently of that setting, and clear only the corresponding exchange on `startup.recovered`. Never route recoverable degradation to the terminal startup-failure/reload latch. |
| SDK `webworker/ExchangeWorker.ts:411-423`; NX controller `:193-225,1838-1867` | The existing `worker_error_phase: 'startup'` identifies a thrown `start()` error. A retrying startup event does not acquire that terminal meaning from its message or level. Preserve genuine thrown-start handling separately. |
| NX `apps/cli/src/exchange-manager.ts:108-119`; `apps/cli/src/repl.ts:957-1002,1863-1864,2709-2733` | `enable` awaits `startExchange` before returning, and REPL installs error listeners afterward; early startup receipts can be missed. Attach structured/legacy listeners and their per-account state before awaiting startup on every connect/enable path. Print/update the degraded state and clear it on its recovery code. |
| NX `services/exchange-server/src/virtual-worker-context.ts:179-205,320-325` | Hosted `log` messages go to server stdout and are dropped before client delivery. Structured startup state must reach the web client; server logs alone do not satisfy user visibility. |

Deduplicate each structured receipt against its matching legacy report using
`opts.engineEvent` or the log event metadata. Preserve unrelated errors and
logs; do not suppress them globally by matching message text. Prove degradation
is shown with reconnect toasts disabled, recovery clears only that exchange,
repeated/legacy delivery does not duplicate the report, and a pre-start CLI
listener observes the first receipt. The native/DO event deployment prerequisite
below still applies. This records required caller changes, not proof they have
shipped or that the current UI displays a persistent degraded state.

## Coded diagnostic caller cutover (PR5)

The SDK exposes `engine-event` (`EngineEvent`), also forwarded by ExchangeWorker
as `{ op: 'engine-event', data: [event] }`. Until callers migrate, Error still
uses `error` with `opts.engineEvent`; Info/Warn uses `log` with the event as its
third argument. Choose one path per report; skip a legacy notification only
when its event metadata is present. Keep ordinary command/host error listeners. Use
`ExchangeHealthMonitor.processEvent(event, classified?)` for native diagnostics
and `processHealth(exchange.getHealth())` for authoritative lifetime counters;
observed event counts cannot substitute for filtered/deduplicated engine counts.
Keep `processMessage` only for legacy TS/unstructured diagnostics.

Read-only tealstreet-next inventory; application changes remain cutover work:

| Caller | Required change and preserved behavior |
| --- | --- |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:180-383` and its worker listener | Accept/forward `engine-event`; classify code and typed errors before toast, auth badge and terminal reload latch. Preserve 120 s auth badge/reload behavior and startup-phase tags. Avoid classifying the same diagnostic on both legacy and structured paths. |
| `apps/cli/src/exchange-manager.ts` (health-monitor owner) and its error/log listeners | Feed events to `processEvent` and refresh engine counters via `getHealth()`; keep legacy fallback for TS venues. |
| `apps/cli/src/error-classifier.ts` | Use typed venue codes/fields for Bybit environment mismatch rather than the message prefix; preserve critical dedup by account/category. |
| `apps/web/src/utils/telemetry/safeCexTelemetrySink.ts` | Preserve `event_code` tags and use stable codes for grouping; builder-fee reporting stays independent. |
| `packages/exchange-pretty-error-messages/src/bybit/bybitHandler.ts` and `bybitRejectReasonToLocale.ts` | Select translations from typed venue code/fields and event code, including signing failures and order rejection; message text is display data. |
| `packages/exchange-pretty-error-messages/src/bingx/bingxHandler.ts` | Keep display passthrough; use the monitor's code-based severity/category rather than adding text classification. |

`auth.rejected` identifies a credential rejection; typed `authExpired` and
`authPermission` distinguish expiry and permission. A generic `request.failed`
must not be upgraded to auth solely by message wording (Paradex signing/network
failures still share that code). Preserve the current legacy Error notifications
until callers and any remaining venue classifications are migrated together.

Deployment ordering: **safe-cex-server must be deployed before Info+ venue events
matter to native clients**. Old NativeFrameSession rejects the native `event`
notice. Deployment is Sam's call; neither logging PR deploys anything.


## Owned history completion core: BloFin reference

Merged #1710 at `1910baa65a3` (source and caller section audited at that pin).

BloFin closed positions now finish through the shared engine history owner and
cursor pager. The successful partial-result contract stays the same: an owned
Warn `history.incomplete` precedes retained rows, including at Error-only log
level. Failure before any accepted rows rejects with a typed error. A failed later
page now returns only prior accepted rows plus the owned marker; unvalidated rows
from that failed response are excluded. Callers must honor incompleteness before
using these rows as complete coverage. Encoding or lifecycle invariant failures
still reject. A repeated non-adjacent continuation now stops with an owned incomplete result instead of
cycling to the page cap; short pages, repeated identities and immediate unchanged
cursors keep the existing termination behavior. No SDK method arguments, worker
completeness envelope or NX ingestion requirement changes. B11 remains open for
NX query association and coverage; subsequent closed-position migrations are
scoped below and general-history fan-out remains active. [Implementation scope](RUST_OWNED_HISTORY.md).


### Windowed / seeded closed-position fan-out

Bybit and OKX closed-position reads now use the same staged/accepted history
owner and cursor core. First failures before accepted rows are typed rejections;
later failures return only accepted rows with one command-owned incomplete marker
before the final result. Callers must carry this through B11's request-scoped
completeness transport and avoid claiming complete history coverage.

Bybit requires the successful `result.list`; missing/null/malformed data no
longer becomes an empty complete result. Its 20-page budget resets per window,
raw duplicate rows stay intact, and one owner retains earlier incompleteness
across later windows. Adjacent repeated cursors and non-adjacent cycles now stop
early with incomplete coverage. OKX keeps its initial upper-bound cursor, lower
bound, 100 ms continuing-page delay and immediate unchanged-cursor exhaustion;
non-adjacent cycles stop with incomplete coverage. Neither method's arguments
change. This slice covers closed-position reads only; both venues' general
history reads and the broader query-marker fan-out remain unfinished. No NX
migration, deployment or B11 closure is claimed.

## Shared SDK clock ingress supersedes venue opt-ins (#1709)

Actual merge `6f815755c37` removes `host_clock`/`hostClock` from the native
capability record, JS reader/types and all five venue opt-ins. Wire bit14 is
reserved rather than reused; later fields keep their positions. The historical
#1520, #1648 and #1693 rows above describe their earlier merges, not a current
capability to test or set. Ship paired SDK/native artifacts; do not depend on
`capabilities.hostClock` or a venue-name clock policy.

The default SDK provider awaits shared initial synchronization before engine
startup and reads synchronized time dynamically for later events. All Rust
venues use that ingress, including commands, HTTP/WS/sign callbacks and timers.
The bound clock also supplies SDK schedule-deadline validation and default
Bybit/WOO journal windows. An explicit host/transport/replay clock stays dynamic
and authoritative; it is never replaced by synchronized wall time. Custom
native/server/Workers hosts remain responsible for the clock they supply.

Existing account/factory callers add no argument. This is not evidence of a
particular NX deployment or live signing session. Bybit/Aster protocol-time
calibration remains separate from this merged ingress change; see
[RUST_ENGINE_CLOCK.md](RUST_ENGINE_CLOCK.md).


### Bybit general-history completion and finalization

Bybit fills, order history, income and public trades now complete through the
same engine owner. Each response requires a successful `result.list`, decoded
in full before any of its mapped rows are admitted. A missing/null/malformed
result or list no longer becomes an empty complete result. Before any accepted
rows, native/HTTP/transport/parse failures reject with a typed error; after
accepted rows, a failed page returns only those prior rows with one command-owned
Warn `history.incomplete` immediately before completion, even at Error-only log
level. A failed body's valid-looking prefix cannot leak into that result.

The owner retains rows across Bybit's category and seven-day window stages.
Final sorting, compound income/hedge-leg dedupe and the global caller limit apply
to accepted rows on both normal and recovered completion. Limits still count
mapped rows before final dedupe within each category; satisfying a requested
limit takes precedence over a continued cursor or safety cap. Income's empty
native list still ends that window. Public trades remain an unsigned single-page
read, and keyless private history retains the existing no-I/O empty result.

Pagination-policy refusals stay strict: bounded page caps and repeated cursors
reject rather than manufacture partial success. Non-adjacent cursor cycles now
reject earlier instead of walking until the page budget. Unbounded order-history
page caps retain accepted rows with one owned marker across any later windows;
a subsequent failed page preserves the earlier missing-coverage cause. Encoding
and owner-lifecycle failures always reject. Method arguments and the B11 transport
are unchanged; NX still must honor request-scoped incompleteness before marking
history coverage complete. Other venue histories and background-coverage notices
remain outside this slice; B11 and global enforcement stay open.


## Send-time signer core and trusted bootstrap (#1714)

Actual merge `ad0e6fa1e42` introduces the engine's send-time provider boundary
with Binance USD-M/PM/COIN-M/Spot as the reference. Standard `ExchangeOptions`
credential inputs remain accepted by the trusted default adapter, which extracts
primary/secondary credentials before `BaseExchange`; retained generic
`exchange.options` and engine-options JSON no longer contain those credentials.
A one-use setup capsule passes optional raw-WASM constructor argument 3,
`signerSetupJson`, to the local provider separately. Custom loaders must forward
that argument; ordinary default-factory callers add no constructor argument.
Incomplete secondary setup rejects synchronously as `invalidInput`/`notSent`.

**REQUIRED NX trusted bootstrap migration:** pinned NX
`65e086a3c90` passes `options: exchange.options` into the worker
(`apps/web/src/controllers/exchangeController/exchangeAccountController.ts:2861`),
and hosted initialization registers the signing account from those options
(`apps/web/src/utils/shim/worker/ExchangeServerWorker.ts:1030-1037`;
`signingHost.ts:77,88,106-117`). Provision provider setup separately in the
engine-owning realm, including rotation/reload/disposal; never restore credentials
in generic options. Replace raw `options.key === ''` public/private assumptions
in `getExchangeAppearsConnected.tsx:45`,
`getOrFetchPositionsForSymbolSync.ts:30` and `onPositionMetaChange.ts:28` with
public/signing-state metadata. The old main-thread WS sign-request path
(controller `:3009-3025`, SDK `exchanges/base.ts`'s `signWsAuth`) must reach the
provider instead of reading the sanitized `options.secret`. Reload and CLI
construction from saved account configuration remain trusted setup inputs;
this static audit does not establish deployed cross-realm behavior.

Raw JS hosts can inject `EngineHost`'s `options.signer`; native trusted callers
use `Engine::set_signer`, `Host::with_signer` or
`FrameServer::add_venue_with_signer`. Workers deployments can supply
`createSigner(state, env, venue) -> { signer, identities }`; only opaque refs
enter `EngineOptions.signing`, never callback objects in the client command wire.
`EngineHost.options.ownsEngine` defaults false, preserving raw borrowed callers'
explicit `free()` duty; managed SDK/Workers hosts set it true. Rust provider setup
wipes owned buffers; JS releases references without a physical zeroization claim.
Unmigrated families do not gain provider/ref support from this reference merge.

A provider refusal or signing timeout before dispatch is `notSent`; earlier
partial/unknown write outcomes stay intact, and a signature is neither an ACK nor
permission to replay an unknown write. This merge adds no remote HTTP/WS signing
wire and makes no NX migration or server-held-key deployment claim. External
remote signing and later family migrations remain required. The legacy raw-input
adapter is transitional: remove it before TS venue/reference deletion after
trusted bootstrap adoption. [Current core contract](RUST_SIGNER.md).


## Bybit send-time signer adoption (#1728)

Actual merge `3b20e602258` extends the shared dispatch signer to all five Bybit
aliases: `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo` and `hyrotrader`.
Their shared construction rules now set `bridge.dispatchSigning: true`.
The trusted SDK adapter extracts primary/secondary signing credentials before
`BaseExchange`; retained generic `exchange.options` and engine-options JSON
therefore no longer contain those credentials. The default factory still accepts
ordinary `ExchangeOptions` setup without an added caller constructor argument.

**REQUIRED NX trusted bootstrap migration now includes Bybit:** the #1714
worker/hosted/signingHost changes above apply to these aliases too. Supply
provider setup separately in the engine-owning realm, including reload, rotation
and disposal; copying sanitized `exchange.options` to the worker or hosted
signing account cannot reconstruct the credentials. Keep public/private checks
on construction/signing-state metadata rather than `options.key` or
`options.secret`. Pinned NX file/line evidence and the static-versus-deployed
boundary remain the #1714 evidence above; this merge does not implement NX work.

Raw WASM constructor argument 3, optional `signerSetupJson`, carries the one-use
trusted setup separately. Custom WASM loaders must forward it; ordinary managed
SDK factories already do. Valid `ExchangeOptions.localSigner` metadata is now
accepted for Bybit through the shared validation/setup path, where unmigrated
venues still reject it as `unsupported`/`notSent`. Standard UTF-8 setup needs no
new metadata. Native GET secondary selection and POST/private-WS primary
selection remain; Bybit venue user/inviter identities are preserved, so this
migration requires no new persisted stable-ID mapping.

Hosted `signHttpRequest`/`signWsAuth` for migrated Bybit use #1745's shared
provider-backed signing path. This is signing-side adoption, not completion of
the keyless remote HTTP/WS sign-request/sign-response wire or a deployed
server-held-key/bootstrap claim. Provider receipts remain release evidence, not
venue ACKs or permission to replay an unknown write.
[Shared signer contract](RUST_SIGNER.md),
[Bybit adoption and evidence](RUST_BYBIT_DISPATCH_SIGNER.md).


## Cumulative-fill reference correctness (#1720)

Actual merge `9fc2a7c174ad` introduces shared engine financial preparation for
Hyro's Bybit adapter and Bitget Classic order-derived fills. Quantity, price and
next delivery progress are validated before native dedupe/progress commits.
An unpriced Hyro observation no longer consumes a later corrected cumulative
fill; an invalid/unpriced Classic observation neither emits a negative-price
fill nor consumes the corrected report's execution eligibility. Late authoritative
Classic fills retain the cumulative anchor, preventing a subsequent cumulative
catch-up from re-emitting an already delivered interval. When cumulative quote
is missing, Hyro's zero average remains unavailable quote evidence, preserving
its authorized fallback instead of deriving an inflated price from a fabricated
zero-value seed. An explicitly supplied cumulative quote of zero stays distinct.

These five repairs intentionally change notifications on the demonstrated bad /
corrected / late report paths; they are not merely internal refactoring. Existing
FILLS shape, native identity/duplicate rules, bootstrap-versus-live separation
and direct Bitget UTA delivery remain. No new SDK method argument or mandatory
NX handler migration is introduced. Other cumulative-fill consumers still need
adoption of the shared boundary. [Reference scope and regressions](RUST_CUMULATIVE_FILLS.md).


## Shared engine protocol-clock calibration (#1722)

Actual merge `d3e34e6e105` replaces Bybit/Aster local clock offsets with one
calibration service per engine. Bybit server-time rejections and Aster `/time`
observations replace that engine's correction; they never accumulate it or
change another engine. Outbound signing timestamps, absolute expiry and
time-derived nonces consume calibrated protocol time. Host scheduling time
still owns timers, admission/preparation age, network deadlines and emitted
market timestamps; explicit replay/host clocks retain their authority.

A changed calibration revision invalidates pending and materialized signed
output before release, including stale provider errors. The shared signer
reprepares immutable unsigned input under a new attempt ID while retaining FIFO
position and the original host-time deadline; timeout wins. Already transmitted
requests are never replayed because of calibration. Bybit's existing eligible
once-only resend and Aster's bounded sync/nonce policies remain.

No SDK/NX constructor argument or caller migration is added. Custom signing
providers must sign the supplied canonical bytes rather than substitute their
own wall-clock stamp. This calibration complements #1709's shared SDK ingress;
it does not restore the retired hostClock capability.
[Clock domains and regressions](RUST_ENGINE_CLOCK.md).


### Binance general histories: shared range admission and completion

The Binance reference adapts fills, orders, income and public trades to the
engine's accepted-page owner; ranged income uses `RangeHistory` for bounds,
identity selection and missing-coverage decisions. An HTTP/native/transport or
whole-array parse failure before selected rows are accepted rejects typed.
Previously fills/orders/single income could report an error then return `[]`,
public trades could silently return empty or a malformed prefix, and ranged
income could continue past a failed chunk. Those paths no longer prove complete
empty coverage. After accepted rows, an actual page failure finishes only prior
accepted rows with one command-owned Warn `history.incomplete` before the result,
including at Error-only logging. A failed page's valid-looking prefix is excluded.
Signing refusal/timeout remains a strict typed rejection (`notSent`) rather than
partial success, even after accepted rows.

Successful request limits, fill-window order/output duplicates, ranged-income
compound identity and sorting, two-second chunk pacing, single-income/native
trade order, missing-symbol/keyless no-I/O and unsupported profile boundaries
remain. Client-order lookup stays separate, retaining primary credential routing,
identity validation and definitive-miss proof. No SDK method argument or mandatory
NX handler migration is added; B11's incomplete-query coverage and typed rejection
handling remain required. Other histories and background notices still need
fan-out; this reference does not claim universal enforcement.


## Shared signer prerequisites: provider result and trusted setup (#1721)

Actual merge `802e9c8c6e9` extends the shared provider contract; Binance remains
the reference, with no additional family or remote HTTP/WS wire migration.
Custom Rust and JS/Workers signing providers must return `SignResult { values,
receipt }`, echoing `request.receipt` exactly; old bare arrays reject. `values`
contains the same ordered operation bytes. Optional `stampMs`/`expiryMs` are
canonical protocol UTC safe integer milliseconds, not a provider's replacement
wall clock; either field can be absent. Engine release validates receipt,
calibration revision, expiry and the original host-time deadline. The receipt
callback is final engine-release evidence, never network delivery or venue ACK;
unknown writes still require reconciliation rather than replay.

Native `LocalSigner::from_setup` now returns `Result`. Trusted setup supports
passphrases (missing and empty stay distinct), explicit secret encoding and
credential trimming; opaque identities can carry caller-assigned stable IDs.
SDK `ExchangeOptions.localSigner.{primary,secondary}` exposes `stableId`,
`secretEncoding` (`utf8|base64|hex|pem`) and `trimCredential`. Credentials,
passphrases and metadata are consumed by the one-use setup channel and removed
from retained generic options. Default UTF-8 Binance setup needs no new argument;
raw WASM constructor argument 3 and #1714's required NX trusted worker/hosted
bootstrap separation remain. Unmigrated Rust venues reject this metadata as
unsupported/notSent. Assign stable IDs consistently across engines/slots sharing
a nonce domain; this does not itself implement a new venue identity or migrate
persisted MEXC key-derived user IDs. Those mappings are later cutover work.

[Provider contract and prerequisites](RUST_SIGNER_PREREQUISITES.md).


### BloFin general histories: owned cursor completion

BloFin fills, orders, income and public trades now retain one shared engine
cursor/completion owner across pages. A transport, HTTP/native, malformed-row or
missing/malformed-list failure before any mapped rows are accepted rejects typed.
An actual later-page failure returns only earlier accepted rows plus one
command-owned Warn `history.incomplete` immediately before completion, including
at Error-only logging. A valid-looking prefix from a failed page is excluded.
Previously later income and filtered symbol-scoped fill failures rejected and
discarded their accepted rows; this conversion applies the shared retained-row
policy instead. Legacy transport and stream-decode callbacks share the same
unclassified failure path, so these first-page errors use the engine's
`internal` fallback rather than claiming a distinct `network` classification.

Native paging refusals remain strict: missing/repeated cursor or the page budget
never become partial success. Header/signing preparation refusal also remains
strict after accepted rows. Native request sequences, cursor selection before
filters, global mapped-fill limit, one native trade's separate PnL/fee income
rows, final sorting/limits, single-page orders and unsigned public trades remain.
Keyless no-I/O still returns an empty array. Unsupported client-order-ID lookup
now reports `unsupported`/`notSent`; invalid ranges and unknown markets report
`invalidInput`/`notSent`, where those admission errors previously had no kind.
Caller error-category/telemetry tags therefore become explicit user-error kinds.
The separate closed-position query is unchanged. No SDK argument or required new
NX handler is introduced; B11's incomplete-query coverage and typed rejection
handling remain required. Other histories/background notices still need fan-out.
[Owner contract and scope](RUST_OWNED_HISTORY.md).


## Background account coverage: explicit scope without a query owner

The six automatic account-reconciliation diagnostics now use the shared engine
`history::BackgroundCoverage` reporter. The existing `history.incomplete` code
carries `scope: "account/background"` and no `command`: Bitunix's unreconciled
fill metadata and Lighter's three outage-backfill diagnostics remain Warn;
TxFlow's execution coverage/deduplication exhaustion diagnostics remain Error.
Native messages, triggers, caps, retries and reconciliation state are unchanged.
Repeated triggers still deliver repeated notices, including at Error-only logging.

These notices describe account coverage, not the completeness of a pending
history query. Raw/native/hosted consumers must not associate them with whichever
query is awaiting a response. The JS host already requires a matching
`fields.command` before flagging query rows; no host protocol or SDK argument
changes. Background coverage does not resolve, reject or flag an unrelated
query. B11's request-scoped ingestion/coverage work remains required, and other
unconverted paged histories remain open; this follow-on does not close the full
history fan-out or prove deployed NX behavior.


### Bitunix histories: shared cursor and accepted-page completion

Bitunix fills, orders and income now use the existing engine CursorHistory owner.
A transport, HTTP/native, malformed-row or missing-list failure before any mapped
rows are accepted rejects typed. After accepted rows, an actual page failure now
returns only earlier accepted pages with one command-owned Warn
`history.incomplete` before completion, including at Error-only logging. This
supersedes #1633's strict later-page rejection; a failed page's staged prefix is
excluded. A full raw page with no accepted mapped rows does not qualify for
partial recovery. The legacy transport/stream-decode callback uses the engine's
`internal` fallback, rather than claiming every failure is `network`.

Header/signing preparation refusal remains strict even after accepted rows. It
now reports `internal`/`notSent`, where the old rejection had neither kind nor
outcome (the missing-credential guard uses `authInvalid`/`notSent`; keyless queries
still short-circuit before signing). Native cursor stalls and page caps still finish with incomplete
coverage. Single-page fills, order time cursors, income offsets, raw progress
before mapping filters, 50-order/100-income page budgets, native financial
mapping, final sorting/limits, keyless and unknown-fill-market no-I/O, and
unsupported admission behavior remain. SDK arguments and result row shapes do
not change. B11's request-scoped incomplete-query ingestion/coverage and typed
rejection handling remain required; this slice adds no mandatory NX handler and
does not close the remaining history fan-out.
[Owner contract and scope](RUST_OWNED_HISTORY.md).


### BingX validated cumulative execution suffix

BingX now rejects a live trade that would deliver positive quantity with a
selected zero/negative price before consuming its native trade ID or cumulative
quantity, advancing stored orders, or publishing a Fill. Corrected evidence with the same ID and cumulative
quantity can therefore deliver the valid fill; healthy peer orders continue.
Native last/average/order-price precedence remains, so an unused invalid last
price does not reject a valid native fallback. Zero-growth observations still
retain native identity bookkeeping. A terminal trade with positive delivery quantity and an
invalid selected price preserves the previous stored partial rather than closing
it; separate lifecycle and quiet-terminal policy remains native.

The shared engine helper caps last quantity to observed cumulative growth and
keeps full native observation for blind intervals. Explicit trade IDs, heuristic
growth escape, covered-history suppression, late unique executions and retention
limits remain native policy. SDK arguments, FILLS revision/row shape and IDs are
unchanged. Existing consumers should not rely on invalid prices or a rejected
row advancing order state. No NX API migration, live-capture claim or universal
cumulative-fill completion claim is introduced; remaining consumers are tracked
in [the fan-out plan](RUST_CUMULATIVE_FILLS.md).




### Paradex histories: shared cursor owner and accepted-page recovery

Paradex fills (including the native orders-history fallback), historical orders,
income and public trades now complete through the engine history owner. Actual
HTTP/native/transport/stream/row/list failure before accepted mapped rows rejects
with a typed error instead of an unclassified failure. A later failed page now
returns only validated earlier pages, with one command-owned Warn
`history.incomplete` before success, including at Error-only logging. Failed-page
prefixes never reach the result; empty/fully filtered pages do not by themselves
qualify for partial recovery. The multiplexed transport/decode callback uses the
shared `internal` fallback, without fabricated native codes or Network evidence.

Client-ID lookup failures, repeated cursors and unbounded page-cap policy
refusals remain strict even after earlier accepted rows. Queued auth/setup and
signing refusal stays strict and now reports `internal`/`notSent`; it cannot be
reinterpreted as a recoverable page failure. A capped open-order lookup remains
nondefinitive and adds no new warning; only both completed lookup stages can
prove a definitive miss. A found row in a failed envelope never proves success.

The native 5/50/200 budgets, raw cursor progress, empty-fill traversal fallback,
public trades' single page, request parameters, native IDs/financial mapping,
sort/limits, public-only and unknown-symbol no-I/O shapes remain. SDK method
arguments and result row shapes stay the same. Callers still need B11's
request-scoped incomplete-query ingestion/coverage and ordinary typed rejection
handling; no additional NX migration is introduced by this slice.
[Scope and proof](RUST_OWNED_HISTORY.md).






### Lighter histories: shared cursor owner across trade and funding stages

Lighter fills, historical orders, public trades and income now use the existing
engine CursorHistory owner. Actual HTTP/native, transport, malformed/foreign-row
or missing-list failures before accepted mapped rows reject typed with the
engine's `internal` fallback. After accepted pages, actual page failure instead
returns only earlier accepted rows with one command-owned Warn
`history.incomplete` before success, including Error-only logging. Failed page
prefixes never enter the result. Empty or filtered pages alone do not authorize
partial recovery. Transport and stream decoding share a callback, so this slice
does not claim every callback failure is `network`.

All-income keeps the same owner from trade fee/PnL rows through funding; a
funding failure can retain validated trade income and earlier funding pages.
Each stage keeps its own output-limit stopping rule, followed by the existing
global stable time sort and truncation. Native raw row counts, cursor escaping,
100-row requests, 1000-page stage budget, time/principal filters, trade/order/
funding dedupe namespaces, financial units and row shapes remain unchanged.
Repeated cursors/page-budget exhaustion and saturated full pages without a
continuation remain strict rejections without an incomplete success. Synchronous
auth-token preparation refusal also stays strict after accepted rows, now typed
`internal`/`notSent` where it previously had neither kind nor outcome. Existing
admission and keyless no-I/O behavior remains. SDK arguments do not change and
no new NX handler is required by this slice; B11's request-scoped ingestion,
coverage gating and typed rejection handling remain required. Other-family
history fan-out and deployed caller proof remain open.
[Owner contract and scope](RUST_OWNED_HISTORY.md).








### Coinbase Advanced Trade histories: one accepted-page owner

Fills, historical orders and public trades now use the engine CursorHistory
owner. The same owner spans CFM then Spot family reads. A transport/HTTP or
whole-response decode/mapping failure before any mapped rows are admitted
rejects typed; after admitted rows, an actual failed page returns only earlier
accepted pages with one command-owned Warn history.incomplete before successful
completion, including at Error-only logging. An empty or entirely filtered
earlier page cannot authorize partial recovery. A conflicting native identity
invalidates the entire page before shared dedupe, even when its earlier copy
was filtered. The failed page's prefix never enters results. Repeated cursors
and the 50-page per-family cap now return owned incomplete coverage instead of
discarding accepted history.

Native continuation/proof metadata refusals stay strict: wrong typed fields,
unusable continuing cursors and proof_token_required reject even after admitted
rows. Both immediate request preparation refusal and the later queued signing
refusal are strict internal/notSent; ordinary legacy stream/transport failures
use the shared internal fallback because the callback also carries parse errors.
These runtime errors previously had no kind, so caller error-kind telemetry
becomes explicit. Query admission errors keep their existing classification.

Native endpoints/family order, cursor escaping, immediate requests subject to the
existing REST pacing, per-family output limits, fingerprint validation before
UTC filtering, CFM contract units, public maker-side inversion, stable timestamp
sort and final global truncation remain. No SDK query argument or row shape
change, no Starbase wiring and no new mandatory NX handler; B11 request-scoped
incomplete-query ingestion/coverage and typed rejection handling remain required.
This adoption does not close the remaining history fan-out or prove live/deployed
completeness. [Owner contract and scope](RUST_OWNED_HISTORY.md).








### WEEX income: shared range coverage and accepted-page completion

WEEX bounded income now uses engine RangeHistory; unbounded/one-sided/equal-bound
income uses CursorHistory for its single page. A failed first page or a failure
before any admitted mapped rows rejects typed with the engine's `internal`
fallback and no completeness marker. Actual later HTTP/transport/native-response/
stream/list/row failure now returns only accepted earlier pages with one Warn
`history.incomplete` before success, including Error-only logging; it stops the
walk immediately. This supersedes the previous skip-failed-chunk-and-continue
behavior. Failed streamed prefixes never enter accepted history. Transport and
decode share a callback, so these errors do not all claim `network`.

A validated page advertising hasNextPage/nextKey can retain rows, including a
valid empty result, with an owned coverage gap; span continuation does not prove
an opaque cursor was consumed. Cursor, chunk and lifetime omissions retain the
first cause and publish once at final success, rather than one marker per page.
The 10,000-request lifetime limit is enforced by the shared owner before issuing
request 10,001. Native 100-day spans, 100-row ranged requests, 100 ms pacing,
shared-pause resumption, compound/time-fallback dedupe, income mapping and stable
descending sort remain. Single-page rows retain their old no-range-filter/no-
dedupe behavior, including equal bounds; no extra final limit is introduced.
Signing setup unavailability stays strict and now has `internal`/`notSent`;
keyless/admission no-I/O stays unchanged. Terminal completion removes deferred
state/timers. Fills, orders, public trades and the fill-ID omission marker are
unchanged and remain separate follow-ons.

No SDK argument, row shape or new NX handler is introduced by this slice. B11's
request-scoped incomplete-query ingestion/coverage and typed rejection handling
remain required. Native-backed host/worker tests now use the reachable later-page
partial-success event instead of the superseded first-failure marker. This is
source/fixture evidence, not deployed runtime proof or cross-family closure.
[Owner contract and scope](RUST_OWNED_HISTORY.md).


### WEEX remaining histories: one owner across windows and order endpoints

WEEX fills, historical orders and public trades now use the existing engine
CursorHistory owner. Each whole native response validates before shared dedupe
and admission; a failed page's prefix is excluded. A first/zero-accepted actual
HTTP/transport/native-response/list/row failure rejects typed with the engine's
`internal` fallback. A later failure now returns only accepted earlier windows
or endpoints with one command-owned Warn `history.incomplete` before success,
including Error-only logging, and stops the queue. In particular, failed algo
orders retain previously admitted normal orders instead of discarding the query.
Synchronous signing preparation refusal remains strict after acceptance and now
reports `internal`/`notSent`; it never becomes a partial success.

Missing fill IDs are still skipped after symbol/order/time filters, with no
invented identity. The once-per-query native `row.skipped(field id)` diagnostic
now publishes only after a valid whole page is admitted. Its coverage fact is
retained by the engine until one final incomplete success; a rejected page cannot
publish a marker or consume the valid-page diagnostic latch. Valid all-omitted
pages may succeed incomplete []; an actual later failure with zero admitted rows
still rejects. Native-backed empty-fill host/worker fixture text stays unchanged.
A valid allAlgoOrders.hasMore:true likewise records an owned coverage gap without
adding pagination; absent/false keeps the previous result, while malformed
explicit continuation metadata rejects or recovers only prior accepted pages.

Native financial mapping/units/large IDs, filters, first-wins symbol|id dedupe,
stable fill/order sorting and public-trade order/duplicates remain. Shared engine
window generation preserves seven-/90-day bounds, the existing ratio budget,
request limits/page0 and endpoint order. List continuation remains immediate;
shared pauses defer the same next native stage. Income retains #1751's behavior.
No SDK arguments or row shapes change. B11 request-scoped incomplete-query
handling/coverage and typed rejection handling remain required; no new NX handler
or deployed-runtime proof is claimed. [Scope](RUST_OWNED_HISTORY.md).


### AsterDEX validated cumulative observation admission

AsterDEX no longer lets negative cumulative execution observations consume its
bounded observation ledger and evict healthy fill anchors. Previously,10,000
distinct malformed observations could make an already delivered fill eligible
again. The native capped-suffix path now uses the existing engine helper before
committing observations; accepted quiet rows still consume native retention
slots, so this does not retain anchors beyond the existing10,000-order bound.

Explicit execution IDs still deliver their positive native last quantity in
full independently of unusable cumulative/display fields; valid duplicate-ID
observations still advance the full observed anchor. Native last/average price
precedence, the requirement for last AND cumulative evidence without an ID,
one-time initial REST seeding and later REST/read isolation remain. FILLS
revision, IDs/row shape, SDK arguments, signer/clock/lifecycle and NX interfaces
are unchanged. This is a correction to financial admission, with no mandatory
NX handler migration or universal cumulative-fan-out completion claim.


### Keyless Rust signing: shared callback provider (Binance and Bybit)

Binance, Binance PM, Binance Inverse, Binance Spot, Bybit, Bybit Inverse,
Bybit Spot, Bybit Demo and HyroTrader Rust accounts admit the existing `remoteSigner` and `PayloadSigner` options through the same host-injected
primitive provider used by local signing. The sign-request/sign-response wire
is unchanged; browser signing still uses the standalone hosted-signing module,
without constructing a TS venue. RemoteSigner takes precedence over PayloadSigner
and any supplied local secret. Failed or unavailable delegated signing never
falls back to local credentials. RemoteSigner selects primary only: work requiring
a secondary signing identity is refused before invoking the remote callback.
PayloadSigner receives the selected public key and exact native HMAC preimage.
Private PayloadSigner accounts remain REST-only, including Binance Inverse;
listen-key creation/renewal, private sockets and subscriptions are suppressed.
The four ordinary Bybit aliases also retain REST-only PayloadSigner startup and
reconciliation. Callback headers belong only to that validated attempt: Binance
keeps case-insensitive API-key replacement; Bybit protects native signature,
API key and timestamp while retaining the existing callback recvWindow precedence.
The signing-only HTTP capture uses that same declared policy before returning
headers through the existing hosted config shape; WS/credential captures deny
callback headers. This does not add a
provider queue, retry policy or separate signing deadline.

PayloadSigner receiver **contract change**: an ordinary callback is bound to a
private, frozen setup-owned snapshot, with the selected public `key` and declared
non-secret metadata. It is not `exchange.options`. No `secret`, `passphrase`,
secondary secret/passphrase, nested signing setup, callback or transport object
is retained in that receiver. A callback that previously read `this.secret`
alongside a PayloadSigner must hold its own secret instead. The snapshot retains
network/market mode flags, site/subaccount/hosted/proxy-region metadata and
`extra` polling/command timeout, recvWindow and Hyro mode values; arbitrary custom
properties and nested credential objects are excluded. Caller mutation after
setup cannot change the receiver or selected key. Disposal and failed host
initialization revoke the setup-owned provider, including retained signer wrappers.

Callbacks must be provisioned in the worker/host where signing executes; ordinary
worker initialization cannot serialize functions. Saved-account, exchange-server
and open-source hosted callers must keep callback provisioning rather than copy
credentials from generic exchange options. Unadopted families keep their existing
routing until their declared provider policy and native feed/header paths are
verified. Private Hyro PayloadSigner is typed `unsupported`/`notSent` at
setup: its TS combination never completes the private handshake. Hyro RemoteSigner
is supported with its existing streaming handshake; no REST-only readiness is
invented for it. Existing external WS signers receive the legacy labels and
metadata: `bybit` plus `{expires}` for all five Bybit aliases; Binance Spot keeps
its alias, fixed subscription id and `{apiKey, recvWindow, timestamp}`. These
values come from exact prepared work, owned public identity and its receipt;
HTTP does not inherit WS metadata or fixed ids even when query bytes coincide.
Production routing never imports a TS venue class. Wire-comparison tests retain
the legacy WS methods only as independent test references.


### Phemex income: one owner across currency ranges

Phemex income reads now retain one engine RangeHistory/Completion across USDT
and USDC. An actual transport/HTTP/malformed-row or envelope page failure before
any eligible output is accepted rejects typed, with the shared `internal`
fallback because the callback multiplexes transport and decode failures. Earlier
unclassified failures now carry that kind, including the resulting telemetry
tags. A later page/currency failure returns only accepted eligible earlier rows
with one command-owned Warn `history.incomplete` before success, even with
Error-only logging. Failed-page prefixes and type-filtered rows cannot authorize
partial recovery; no accepted output means a later actual failure still rejects.

Range saturation retains an owned gap and continues the other currency rather
than rejecting the entire query before USDC. Valid responses with no eligible
rows may still complete incomplete []; they do not prove complete coverage.
The shared owner records coverage from every validated native row before output
filtering and preserves actual native bounds for internally padded equal-bound
queries. Running-balance/raw amount identities and occurrence suffixes, native
units/categories, time/type filters, sorting, optional truncation and pause
resumption remain. The existing more-than-100,000 retained-output guard remains
a strict refusal; it cannot become a large partial success. Admission and
single-page fills/orders/public-trades/client-ID lookup stay unchanged.

No SDK arguments or row shapes change. Existing B11 request-scoped incomplete
query ingestion/coverage and typed rejection handling remain required. This is
shared-core/native-test evidence, not deployed runtime proof or universal history
fan-out closure. [Owner and scope](RUST_OWNED_HISTORY.md).

## KuCoin Spot fallback Fill amounts

KuCoin Spot matches without `matchSize` now deliver only the new suffix of
`filledSize` beyond an admitted native bootstrap/execution prefix. Previously
those rows emitted the whole cumulative amount again. Consumers of the Rust
FILLS stream must accumulate these incremental amounts without applying a second
cumulative subtraction. The wire shape, IDs (including absent public Fill ID),
side, direct `matchSize` quantities and late distinct-execution behavior remain.
The native20,000-order prefix retention is bounded; an authoritative last with
no cumulative endpoint cannot establish its overlap with a later fallback.
No caller deployment, TS removal or live evidence claim is bundled.

### GRVT histories: shared owner with strict order-fill proofs

GRVT fills, closed orders, funding/fill-derived income and public trades now use
the shared CursorHistory completion and page admission owner. A terminal actual
HTTP/stream/decode/transport failure before eligible output was accepted rejects
typed; after accepted output, a general query returns only earlier accepted pages
with one command-owned Warn `history.incomplete` before success, visible at
Error-only logging. Failed page prefixes are excluded, including when the same
page exhausts its five existing transport retries. Funding accepted before a
failed fill-derived income stage is retained. A general repeated/capped walk now
returns accepted rows with owned incompleteness; a zero-output failed walk rejects.

Order-ID-filtered fills promise complete totals and remain strict on every
terminal failure, even after accepted pages: lifecycle ambiguity/missing proof,
limit/cursor/cap refusal and terminal HTTP/decode/transport loss all reject.
Session/bootstrap/deferred preparation failure also remains strict for every
query. These paths publish neither partial output nor an incomplete-success
marker. Typed finalization now supplies `internal` where those legacy failures
previously had no kind; the shared callback multiplexes origins, so it does not
claim every failure is Network. Initial query admission and client-ID lookup
retain their existing separate behavior.

Five read retries and one session-refresh resend retain the exact native page,
cursor and body. Retrying discards only unvalidated output and native page facts;
accepted rows, page budget, first coverage cause and lifecycle ledgers survive.
Native large IDs, nanosecond bounds, units, order lifecycle isolation, row.skipped
policy, stage-specific limits and stable descending final sort stay intact.
No history row enters live Store, and no SDK argument, result shape, new NX
handler or deployment proof is introduced. B11's existing request-scoped
incomplete-query ingestion/coverage and typed-rejection requirements remain.
[Scope and controls](RUST_OWNED_HISTORY.md).


### Crypto.com histories: shared owner with exact native cursor proofs

Crypto.com fills, historical orders, income and public trades now use the shared
CursorHistory owner for staged/accepted output and final completion. A first or
zero-eligible-output terminal actual HTTP/native/decode/transport failure rejects
typed. After accepted output, a later actual failure returns only those accepted
rows with one command-owned Warn `history.incomplete` before success, including
Error-only logging. Failed streamed prefixes never enter output, even when
transport retries exhaust. Finalization now supplies `internal` where the legacy
history failures had no kind; the callback multiplexes origins, so it does not
classify every failure Network.

Native exact `u64` nanosecond bounds and overlapping `oldest + 1` end cursors stay
intact. When continuation is needed, a missing exact cursor or non-decreasing
bound remains a strict refusal after prior accepted rows, with no partial success
or marker. A short duplicate page still proves completion before requiring exact
cursors; a full duplicate page may continue to an older bound. No normal 50-page
cap is added. The shared walk's representable ceiling is a strict safety refusal.

The existing single retry counter is preserved: `40101` retries below one; other
transport loss retries below three. Retries discard only the current staged page
and newly observed native IDs; admitted overlap identities, output and owner
survive. Retry/continuation timers remain 1000ms, separately respecting existing
per-method/key pacing. Native result order, limits, fee sign, timestamp units,
large IDs, fractional bounds and income mappings remain. Explicitly ranged
income still ignores the requested output limit, including zero; other effective
zero-limit and equal-bound queries keep no-I/O success. Public reads remain
unsigned one-page GETs with native duplicates and existing retry behavior.

No SDK argument/result shape, new NX handler or live Store publication is added.
B11 request-scoped incomplete-query ingestion/coverage and typed rejection
handling remain required; other adapters, runtime proof and managed Context
completion remain separate work. [Scope and controls](RUST_OWNED_HISTORY.md).

## Orderly incremental Fill quote correction

Orderly broker Fill consumers continue to receive incremental native amounts in
the existing FILLS shape, with the same side, IDs, identity admission and late
delivery policy. From a known preceding quote or an empty prefix, a uniquely
delivered direct Fill that exactly covers cumulative growth now retains its
proven quote even when the cumulative average is missing or zero. The next
average-priced suffix is therefore priced from that quote:
1@100 followed by cumulative2/average100 yields another1@100, rather than1@200.
No consumer-side cumulative subtraction or compensating reprice is needed.

The native arbitrary-last quote-delta quotient remains unchanged for reports
that do not establish the whole interval. Quiet and duplicate observations may
advance native totals without inventing a delivered quote; blind gaps and bounded
4096 terminal retention retain their documented limits. No broker/wire API change, deployed
caller migration, live/provenance evidence or TypeScript removal is bundled.

## Shared hosted signature presentation (family admission remains gated)

Construction policy now describes signature encoding and an optional passphrase
presentation header separately from runtime admission. The shared HTTP capture
uses setup-owned credential/signing/passphrase refs and validates the complete
atomic result before returning its existing signature/header shape. Passphrases
are Present-only results, never canonical HMAC payload parts or generic exchange
options. The existing case-insensitive callback-header policy decides precedence;
combined native and callback headers must fit the shared bounds.

No external sign-request/sign-response field is added. WS keeps its existing
outputFormat and signature-only response. OKX/Bitget metadata alone does not
admit a Rust signer account or complete their hosted migration: those family
adapters still require their native SignStep and actual WASM/SDK HTTP/WS proofs.
The landed Binance/Bybit callback wire and output defaults remain unchanged.

For a family that declares passphrase presentation, callback setup must receive
the existing required passphrase material through trusted setup even when a
RemoteSigner holds the HMAC secret elsewhere. The unchanged remote response
returns a MAC, not a missing passphrase. Secondary absent/null passphrases use
the setup-authorized primary ref; explicit empty never inherits, and public mode
does not waive secondary requirements. Provider custody is separate from public
key interpolation: passphrases can be presented only with signing work owned by
that identity, not included in HMAC preimages or callback receiver metadata.
RemoteSigner remains primary-only and never falls back to a local/PayloadSigner
after refusal. The provider requests the declared format and rejects noncanonical
or incorrectly sized base64 MACs before any attempt can be published.


## Persistent row-reporting engine path

Coinbase, Crypto.com and WOO skipped-row notices now use engine-owned repeat and
re-arm memory. The observable notice levels, `row.skipped` code, message text and
reporting time remain native-compatible, including Coinbase's single Error
upgrade and WOO's changed-summary repeats. Host log filtering still suppresses
delivery without suppressing health accounting or resetting report memory.
No SDK arguments/results, order/position/FILLS publication or caller migration
is added. This slice does not complete grouped/deferred reporting or the other
consumer migrations; those remain in [RUST_ROW_ADMISSION.md](RUST_ROW_ADMISSION.md).


## Managed request caller contract (#1741)

Merged #1741 at `b2fe97324763e1e2702c69ac5b839da37102928b` introduces
managed HTTP release/outcome handling with Binance as the reference. The four
changes below are caller-visible differences from the TS path, not comparison
exclusions. Binance history/session-token work and Bybit requests remain legacy
at this pin; this does not claim NX adoption or deployment.

| Change | SDK behavior | Required NX order-error handling / migration |
| --- | --- | --- |
| Write ACK at or after the original request deadline | Rejects with `outcome: 'unknown'` even when the late ACK reports acceptance; TS reported success. The order may exist. This reference retires late replies and reconciles through normal fenced reads. | Keep the command uncertain rather than showing success or asserting nothing was sent. Resync orders/positions and reconcile the original order/client IDs before allowing a replacement or retry. Preserve local cleanup without erasing uncertain venue effects; never automatically replay the write. |
| Native partial-batch refusal | Retains independent confirmed effects and bounded exact accepted IDs, but rejects with partial failure where TS could report optimistic success. Known accepted plus definitively refused legs need not have an `outcome` value; mixed definitive cancel effects without IDs are known partial, not effects-free `rejected`. Unresolved legs keep `unknown`. | Treat the rejected promise as failure even when `outcome` is absent. Preserve `partialOrderIds`/`partialOrderCount` when present, reconcile confirmed effects, and report the refused/uncertain remainder. Scaled/protective-order orchestration must not mark the whole batch successful, infer zero effects from absent IDs, or replay the whole batch. |
| Count-mismatched placement ACK | Rejects as `unknown`; applies no Store or caller-ownership patch and attributes no returned ID to a caller leg. Valid native IDs are retained as evidence-only PartialIDs for reconciliation/cancellation, within the shared ID bound; overflow stays Unknown. Total/failed-index attribution remains unset. | Retain exact `partialOrderIds` as reconciliation/cancellation evidence. Do not zip them to input legs, invent a failed index/total, or promote them to successful owned orders. Refresh authoritative state before deciding what remains to place; no automatic retry based on the apparent unmatched count. |
| Managed custom HTTP transport | Requires explicit preparation, binding and final synchronous transport handoff; implementing only legacy `request()` cannot silently service managed work. `RestTransportContext` gains optional `credentialIntent` (`'public'`, `'primary'` or `'secondary'`) for routing unsigned requests before credential materialization. | Upgrade custom worker/hosted transport adapters with the SDK. Preserve declared credential intent in route/proxy selection; do not infer the slot from signed headers. Surface preparation/protocol failures through the existing typed error path; `notSent` requires that no write in the logical command reached transport. A later leg/retry can fail before release while the command remains `unknown` or known partial from earlier effects; preserve the accumulated outcome. Never bypass the handshake or replay via the old transport as a fallback. |

For the transport change, [`Transport.prepareHttp`](../../sdk-rs/hosts/wasm/js/host.d.mts)
returns a prepared resource whose synchronous `bind(packet)` yields a bound
resource with `ready()` and synchronous `dispatch()` returning the response
promise. Route/token/HOW awaits finish before the engine's final claim and
handoff. Preparation alone proves neither send nor acceptance. Custom
[`RestTransportContext`](../../packages/safe-cex/src/transport/types.ts) consumers
must honor the declared credential intent; its optionality preserves legacy
callers, not permission to infer managed credentials or omit managed preparation.

NX integration targets remain the audited `65e086a3c90` sources:
`apps/web/src/order/sendOrder.ts`, `createMarketOrder.ts`, `createLimitOrder.ts`,
`createConditionalOrder.ts`, `createScaledOrder.ts`, `updateOrder.ts` and
`cancelOrders.ts`, plus worker/hosted error receivers. In particular,
`createScaledOrder.ts:64-69` already reads partial count/IDs, but that does not
prove the new uncertainty, evidence-only ID or transport behavior is handled.
Upgrade sender and receiver together so `kind`, `outcome`, native details,
partial metadata and `errorReported` survive worker/hosted serialization. Use
structured fields rather than `instanceof` or message matching, honor
`errorReported` to avoid duplicate notifications, and retain cleanup/rethrow.

Caller acceptance must cover late accepted ACKs, known partial refusals (including
cancel effects without IDs), mismatched ACKs with evidence-only IDs, and a custom
transport without preparation support. Exercise the real local-worker and hosted
error round trip: preserve uncertainty/partial metadata, show one terminal failure,
reconcile through reads, and perform no automatic write replay. These are required
NX migration tests; this docs-only backfill does not execute or close them.
The engine/host contract and remaining legacy gate are detailed in
[RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md).

## Selected RemoteSigner runtime mode

When both `remoteSigner` and `payloadSigner` are configured on an admitted Rust
account, RemoteSigner remains the selected provider. Shared setup
also uses that selection for private PayloadSigner admission and the native
feed flag. It restores normal private streaming for Binance's four aliases and
Bybit's five aliases (including Hyro), instead of incorrectly treating the unused
PayloadSigner as a REST-only account. Hyro accepts the selected RemoteSigner;
private PayloadSigner alone remains typed `unsupported` / `notSent`, even if a
local secret is supplied. The eight supported PayloadSigner-only aliases retain
their REST-only behavior.

Raw `publicMode` callback-presence intent retains both fields for construction
resolution; it is separate from the selected native runtime mode. Invalid
explicit callback shapes still reject, including an invalid unused callback.
Unavailable, refused or malformed RemoteSigner work cannot fall back to the
PayloadSigner or a local secret. Caller input, callback receiver custody and the
external sign-request/sign-response wire do not change. No new caller option or
persisted identity mapping is required. See
[RUST_SELECTED_SIGNER_MODE.md](RUST_SELECTED_SIGNER_MODE.md).

### Hyperliquid perp histories: shared inclusive-page ownership

Perp fills, orders, income and public trades now use the engine CursorHistory
page-admission and completion owner. An actual HTTP/stream/decode/transport
failure before eligible output was accepted rejects typed with `internal` where
legacy failures had no kind; callers and error telemetry now see that category.
After accepted output, a later actual failure returns only accepted pages with
one command-owned Warn `history.incomplete` before success, including Error-only
logging. Failed-page prefixes and zero-income or filtered native rows cannot
authorize partial recovery. Histories never mutate live Store.

Full inclusive-page stalls and the existing 200-page stage cap now retain
accepted output with owned incompleteness instead of discarding it. A valid
coverage-limited response may still yield incomplete `[]`; that is no proof of
complete coverage. Mixed income keeps one owner and the first gap through
fill-derived PnL/fees and funding, continuing funding after a capped/stalled fill
stage. Rolling historicalOrders retention (at least 2,000 raw rows) flows through
the same owner rather than a venue-local marker. The gap remains query-scoped.

Physical time bounds remain inclusive: short positive pages still issue the
existing duplicate/empty terminal probe; same-time distinct IDs still advance.
Native transition timestamp/rank ties fold before order filtering; native sorting,
limits (including the fills default and clamp), ID/time units, gross historical
PnL, rebate signs, fee assets, builder revenue fields and vault/master selection
remain. Spot and client-ID lookup keep their existing separate behavior, including
lookup's scalar completion and admission errors. No SDK argument or history row
shape changes. Existing B11 request-scoped incomplete ingestion/coverage and typed
rejection handling remain required; this native/shared-owner scope does not close
B11, managed Context migration or deployed runtime proof. [Scope](RUST_OWNED_HISTORY.md).


## Grouped and deferred row-reporting compatibility

Crypto.com and WOO skipped-row groups use shared engine collectors while keeping
native summary level/code/text/order, first-label/count rules and cached-state
retention. WOO clean groups re-arm only that summary kind. Bybit unsupported-order
diagnostics retain unbounded per-product Once memory at accepted joined reads;
a failed/noncurrent/incomplete whole join cannot consume it, an admitted partial
join still reports, and clean/mapped rows never re-arm it. Known unsupported
orders are restored independently of whether their diagnostic was suppressed.
No SDK argument/result, remote/header/signing contract, order/position/FILLS wire
shape, caller migration or financial/snapshot authority changes. Remaining
persistent reporter consumers stay open in RUST_ROW_ADMISSION.md.


## BloFin and Kraken row-reporting adoption (2026-10-23)

BloFin, Kraken futures and Kraken Spot use the existing engine reporter instead
of five account-owned suppression collections. Caller-visible diagnostic text,
severity, ordering and repeat/re-arm timing remain native. BloFin retains its
separate position/order bounded Once memories, decode-time per-key forgetting
and existing accepted-read clear sites. Kraken futures retains bounded
Warn-to-Error upgrade memory and separate unbounded Once unlisted-symbol memory;
unlisted mapping still reports before snapshot admission, zero-quantity unknown
positions stay silent, and reconnect never rearms that memory. Kraken Spot
retains bounded Once, coin/order namespaces (including empty order identity),
admitted-versus-rejected stale balance behavior and the different REST/WS mapped
identity filters. No SDK call/result, order/position/FILLS shape, health policy,
financial or snapshot authority, signing, HTTP/WS lifecycle or caller migration
changes.
## Static public headers during managed HTTP preparation

Trusted Rust `SignStep` implementations can declare static noncredential HTTP
headers through `unsigned_http_headers`; the default remains empty. The engine
validates and snapshots this declaration once before managed preparation or
request/attempt/timer allocation, preserving native order through route/signing
refresh. Invalid declarations return the original pending value and typed
`Invalid`, without publishing preparation, timers or signing work. Credential,
passphrase and signature headers remain provider/apply-owned; this declaration
does not grant callback-header authority or change the external signing wire.

BingX's public SOURCE/Content-Type declaration is shared with its existing
legacy renderer, whose signed order remains SOURCE, APIKEY, Content-Type.
The declaration supplies public headers; private signing, provider construction
and hosted signing still follow the family's separate admission policy. Semantic
Context ownership is unchanged. Existing SDK callers need no
new option, credential transfer or persisted identity mapping for this additive
header seam. [Contract and scope](RUST_UNSIGNED_HTTP_HEADERS.md).


### Kraken futures histories: admitted pages with strict native proof refusals

Fills, closed orders, ledger and public trades now use the existing engine
CursorHistory/Completion owner. Actual HTTP/native-envelope/stream/decode/row
failure before eligible output was admitted rejects typed; later actual loss
returns only earlier admitted rows with one command-owned history.incomplete
Warn before Done, visible at Error-only logging. Failed-page prefixes, including
a newer terminal update to an admitted native order, never enter the result.
Terminal orders fold across admitted rows only, preserving strict latest-time
selection and first encounter on equal timestamps.

Native continuation-proof refusal remains strict even after earlier pages:
missing token on a saturated order page, conflicting decoded body/header tokens,
repeated/empty/no-fresh continuations and fifty-page exhaustion return no partial
result or success marker. A terminal fiftieth page completes without a fifty-first
request. The ledger's validated since-only coverage gap now goes through the
engine marker owner with its existing message, native predicate precedence and
eligible-output-limit suppression. Valid filtered-empty coverage may still be
explicitly incomplete; filtered rows cannot authorize actual-failure recovery.

Shared owner finalization supplies kind internal for formerly unclassified
actual-page errors and strict cursor/proof/budget refusals, affecting typed
caller handling and telemetry categories. Existing range admission, explicit
unsupported client-ID lookup and consuming signed-setup refusal retain their
separate paths/classification; no notSent/unknown outcome is invented. Exact
ISO/opaque/since cursor request bytes, native IDs, output fields/units, final
sort/limits, cash/position ledger filtering, public unsigned dispatch and live
Store isolation are preserved. Kraken Spot is separate and unchanged.

SDK arguments and row shapes are unchanged. Numeric Completion uses legacy
authority rather than managed Context completion. Callers must ingest command-scoped
incomplete-query warnings, gate coverage accordingly and handle typed rejection
as unavailable history. [Scope](RUST_OWNED_HISTORY.md).

### KuCoin futures and spot owned history

This slice moves the implemented KuCoin query reads to the existing engine
history owner. Futures fills, orders, transaction ledger and symbol-scoped
funding retain one owner across native pages and time windows. A terminal
HTTP/envelope/metadata-decode/row failure before any eligible page rows were accepted rejects
with kind `internal`; after accepted rows, it resolves those rows plus one
command-owned Warn `history.incomplete`. A failed page's streamed prefix is
excluded. Filters do not authorize partial success, and retries do not poison
accepted identities. Transport and decode failures share callbacks, so they
retain the shared Internal fallback rather than a blanket Network label.

Well-formed but inconsistent page census, ledger offset and funding same-timestamp continuation
proofs, clipped ranges and the retry-inclusive request safety bound remain
strict rejections, even after accepted rows. These refusals now carry the
shared `internal` kind where legacy string errors had no kind; telemetry tags
can change. Time/identity sorting, native units, filters and the output-limit
exception for explicitly ranged income/funding remain unchanged. One owner
spans every window; no venue emits its own completeness marker.

Futures public trades also gains typed Internal failures through the owner; it
remains a single-response public read with unchanged header selection.

Spot fills remains a single HF response with the existing schema/defaults and
no invented continuation. Its whole response is validated before admission;
malformed/HTTP/transport errors reject `internal` rather than an unclassified
string failure. The existing command-error diagnostic on request failure stays.
Spot lookup and disabled order-history/income stubs remain unchanged, and public
trades remains unsupported. No signing helper, credential selection, constructor
argument, capability or NX bootstrap changes are included. Callers must ingest
command-scoped incomplete-query warnings, gate coverage accordingly and handle
typed query rejection as unavailable history.

### Bitrue single-page fills: shared admission and completion owner

Bitrue's bounded native `/fapi/v2/myTrades` result now uses the existing engine
HistoryRows/Completion owner. Rows stay staged until HTTP/envelope, required
array, every row, the 1000-row guard and endpoint coverage proof validate.
Terminal native/decode/transport failures and the existing strict full-page
coverage refusal now report the shared `internal` kind where they previously
had no kind. Their messages remain; no write outcome is invented. Typed-error
consumers and telemetry should use that classification. Admission, unsupported
history, public WS trades and client-ID lookup retain their separate behavior.

A full native page that leaves fewer eligible unique fills than the requested
limit still rejects, even with a valid prefix. This one-page endpoint has no
previously accepted page: failures never return a partial or emit an incomplete
marker. A valid empty array succeeds; a missing or malformed array rejects.
This does not claim full backward venue coverage or a complete PnL ledger.

The original owner survives the single signature resend and three eligible GET
retries. Reset drops only the unvalidated page and its native attempt identities;
retry budgets, timestamp advancement, exact URLs, quantities/IDs, filtering,
final descending sort/limit and no live Store writes are preserved. No SDK
argument/result shape or new NX handler is added. Numeric completion uses legacy
authority rather than managed Context completion. Callers must handle typed query
rejection as unavailable history. [Scope](RUST_OWNED_HISTORY.md).

### Backpack histories: shared owner with native offset requests

Fills, closed orders, funding income and single-page public trades now use the
engine CursorHistory/Completion owner. An actual HTTP/stream/decode failure
before eligible rows were accepted rejects typed with the shared `internal`
fallback where legacy failures previously had no kind; resulting error-kind
telemetry changes accordingly. The shared callback multiplexes transport and
decode origins, so this does not classify all failures as Network. A later
actual failure returns only earlier validated pages with one command-owned Warn
`history.incomplete` before success, also at Error-only logging. Failed-page
prefixes and pages containing only filtered output cannot authorize recovery.

A repeated raw page or the existing fifty-page limit now resolves admitted rows
with owned incompleteness instead of rejecting the whole query. Valid coverage
may therefore return incomplete [] when no rows passed local filters; this never
proves coverage complete. Malformed fills still skip without hiding healthy
peers, but admitted skips now also supply an owned coverage gap. The existing
row.skipped summary includes only admitted pages, once per successful result;
failed-page skips cannot replace an earlier gap or enter the summary. Non-fill
invalid rows still invalidate the entire page and retain their existing eager
CommandError diagnostic, even when earlier valid pages subsequently resolve.

Native request offsets still count all raw rows, including filtered/skipped
neighbors. Query limits, page sizes, native row order, ID/quantity/fee/time units,
fractional bounds, filtering and unsigned public trades stay unchanged. Setup
refusal in the existing consuming signed helper stays strict and separate;
client-ID lookup and unsupported-ledger/time/market admission are unchanged.
There is no history Store publication or new SDK argument/row shape. Callers must
ingest command-scoped incomplete-query warnings, gate coverage accordingly and
handle typed rejection as unavailable history. [Scope](RUST_OWNED_HISTORY.md).

### ApeX Omni bounded fills: typed rejection after every REST alternative fails

Execution history now uses the engine HistoryRows/Completion owner. If every
native/display-symbol attempt on both fills and historical-order routes actually
fails, the query rejects with kind `internal` and the final terminal message.
It no longer returns background WS-cache rows or complete `[]` after those
failures. The cache was not fetched by this command; callers can continue to
consume their live cache separately. No write outcome or partial marker is
invented. Error handling must accept this rejection instead of treating the
unavailable history as a complete response.

Validated successful selection is preserved. A nonempty native primary response
stops further REST alternatives, including when every row filters out; that case
may still select matching cached rows. Any validated empty alternative likewise
retains the existing cache path if later alternatives fail. Eligible nonempty
REST output stays authoritative and never merges cache rows. Successful empty
history remains distinct from all-actual-failure exhaustion.

The original owner survives candidate moves and signature/transport resets.
Native bounded upsert/top-limit eviction, exact IDs/raw times, output shape,
filters, signing bytes, query bounds, two-versus-four alternative count and
retry policy remain unchanged. Initial consuming signing refusal and existing
wrapper-to-history error-message flattening retain their separate legacy paths;
this slice does not claim new typed signing provenance. Unsupported axes and
public-only/missing-market empty admission remain unchanged. No live Store
publication, SDK argument or managed Context migration is added. Callers must
handle rejected history as unavailable and consume live cache separately when
needed. [Scope](RUST_OWNED_HISTORY.md).


## Shared unproven write-response guard (Binance reference)

The managed send path treats HTTP 408 and statuses >=500 as unproven before a
native codec can update Store, ownership or command effects. The unproven
attempt is recorded with `kind=Timeout` for HTTP 408 or `kind=VenueUnavailable`
for statuses >=500, including 502 HTML and 503 carrying native -1007/-2019.
The logical write outcome is Unknown; existing journal rules still select the
terminal diagnostic in logical planning order. Validated native code and message
remain bounded diagnostic fields (`venue_code` and
`venue_message`); they do not turn a gateway response into a retry-safe refusal
or authentication evidence. This attempt's diagnostic uses the shared uncertainty
summary; terminal selection still follows logical planning order. Definitively accepted earlier effects
and exact IDs remain in the command journal.

NX order-error handling must use the typed outcome rather than parsing the
primary text or treating a rejection-looking native code as proof of no effects.
Show the bounded native diagnostic as detail, reconcile through reads before any
new write decision, and do not automatically replay the uncertain write. A
PartialIDs list from an unproven body is evidence only: it cannot bind IDs to
caller legs or publish accepted orders. Spot and portfolio margin retain only
native-echoed client identities in their public cancellable namespace; futures
use the endpoint's numeric order/algo identity. Never substitute an intended
client ID when the reply has no matching echo.

Required caller controls cover 503 with native timeout and rejection codes,
502 HTML, 408, an earlier accepted leg followed by an unproven sibling, and
malformed/mismatched identity evidence. Retain Store/ownership and physical-send
assertions. This reference does not migrate Hyro, history/session work or any
legacy-provenance request and does not establish NX integration or cutover.


### WOO measured CPU-margin build policy

Standalone WOO WASM family builds use the throughput defaults for the shared
host, engine and inherited dependencies, alongside the AsterDEX and KuCoin Spot
exceptions. The common serde_json level-3 policy and WOO's existing explicit
venue profile continue to apply. This is a build-profile selection: callers use
the same loader, API, request/signing contracts and public/private row shapes.
Other family selections, the combined engine and native build profiles retain
their existing policy.

## Managed paired-read publication and companion authority

Rust adapters can use `Cx.validate_read_http_delivery(root, leg)` before
synchronously publishing a paired-read response. It binds the exact original
command root and delivered live read leg, including issuer, generation and the
original raw deadline. A live sibling Context or an unrelated socket callback
cannot lend this authority. Native status/body validation and Store/native-cache
freshness checks remain required before applying any response data; a failure
callback proves ownership only. The check allocates no request or retry rights.

`validate_read_completion_context` lets the shared completion owner validate
read-only companion events before publishing them and calling `done_context`
synchronously. A command that has declared a mutation or a write journal cannot
use that read completion path. Existing generic completion APIs retain their
contract. `validate_http_context` checks live issuance without borrowing the
completion path's existing expiry tolerance. No SDK signature, result shape or
transport contract changes. NX adapters that adopt these helpers keep the exact
root/leg handles, publish and complete without yielding, and discard invalid
or stale callback data rather than falling back to numeric command completion.
Joined-read execution/observer ownership remains a separate shared capability.
## Host-safe physical engine resource IDs

Physical socket, HTTP request, signing-attempt and timer IDs use exact numeric
identities through `2^53 - 1`, without wrapping at the former u32 boundary.
Host integrations must preserve the complete integer and must not truncate it to
32 bits. Command IDs, millisecond durations and lossless decimal managed
preparation/permit tokens retain their existing contracts. Resource identities
are opaque correlation values, not permission to send or publish state.

Managed admission reserves complete resource groups atomically and reports typed
capacity refusal without partial preparation or I/O. Replacement-context child
identities use the same complete-range reservation and remain exact above the
former 32-bit boundary; a refused range admits no child or journal supersession.
A fatal legacy allocation exhaustion permanently retires the engine: hosts report a terminal engine failure,
dispose it and require reconstruction. No partially queued callback actions or
previously bound HTTP permit can release after that retirement. Prior-sent write
uncertainty remains subject to the existing reconciliation contract; terminal
engine failure never proves that a write was not sent.


WASM resource-ID ingress rejects nonfinite, negative, fractional and unsafe
integers before parsing a response or advancing clocks. Zero remains the shared
public-socket identity. Custom host adapters preserve numbers above `2^32` without
bitwise coercion. Workers retire flush intervals and notify connected clients
through typed `startupFailed` on terminal failure. Already-owned write errors
retain unknown-outcome/reconciliation text even across host replacement; stale
successes and diagnostics stay suppressed. Native account streams settle owned
waiters as Internal/Unknown once and answer later joins with the terminal notice
without constructing a snapshot from the failed engine. Recreate the engine to
resume work; neither adapter restarts it automatically.

## Bitget family WASM speed profile (option A)

Bitget and Bitget USDC family artifacts use the shared speed defaults for the
host, engine and inherited dependencies, alongside the common serde_json
throughput setting. Consumers download a larger WASM module; the fixed family
size budget still applies. SDK construction, commands, transports, payloads and
row shapes keep their existing contracts. CPU objectives and all strict
correctness checks retain their existing thresholds and known-failure policies.
Combined and native release builds keep their existing profile selection.


## AsterDEX, GRVT and TxFlow row-reporting adoption (2026-10-23)

AsterDEX position, GRVT coin/order/position and TxFlow position diagnostics use
the existing bounded engine Once reporter. Native notice text, severity and
repeat/re-arm ordering remain. AsterDEX and TxFlow keep early REST per-ID
forgetting before full snapshot admission, and zero-size rows do not gain a
per-key rearm. TxFlow retains exact composite/asset/anonymous labels and its
separate WS behavior. GRVT retains namespace isolation, unfiltered duplicate
coin rearm, filtered order/position rearm and rearming before per-row timestamp
rejection on a current response. Callers retain the same account data, command
outcomes, health policy, transports, signing and snapshot/financial authority.
## Engine-owned initial command admission

Adapters use `admit_http_command(policy)` at the start of a genuine engine command
callback, before HTTP, WebSocket, timer or signing work. The engine correlates
this entry with the command being delivered; adapters do not supply a numeric
command ID. An initial policy, clock or capacity refusal emits one structured
command failure without issuing a Context or sending work. Read failures carry
no write outcome; an unissued mutation is `NotSent`. Binance candle, account
identity and trade-root admission use this path. Native input/planning failures
keep their existing behavior.

NX handles an admission rejection once using its typed category and message;
it must not append a numeric failure, optimistic success or automatic duplicate
submission. Resource capacity reports `Internal`; invalid initial context policy
or time reports `InvalidInput`. These local failures prove nothing about an
already-owned or previously sent command. A revoked entry has no completion
rights and must not finalize another generation. The returned Context remains
the authority for successful admission, including cached/no-I/O read completion
and native single-owner WebSocket callbacks. Child legs and retries keep their
existing explicit Context APIs and original deadlines.

A caught native command-callback panic permanently fences the engine, including
held transport permits, pending signer completions and venue-bearing queries.
Hosts dispose the failed engine through their terminal-failure path; NX retains
prior-sent uncertainty and reconciles through a new healthy engine. An engine
panic does not prove `NotSent` for work released earlier.

Shared read owners can validate that a handle is the exact original Read root
before binding completion authority or publishing a no-I/O completion companion.
A same-owner child, foreign issuer, mutation-bearing owner or sealed generation
is refused. This check retains completion expiry semantics and grants no new
send, retry, publication or joined-observer authority.

A successfully admitted initial command retains its completion generation until
typed terminal cleanup, including native single-owner WebSocket reads with no
managed HTTP attempt. Unrelated work and deadline expiry cannot erase it or
reuse its command ID; NX receives the original native timeout once. Only the
issued Context can complete that generation. An expired root still grants no
send or publication rights, and unparented siblings cannot extend its bound.
Outstanding owners count toward the existing 256-owner capacity until cleanup;
adapters must finalize them through their existing native terminal callbacks.


### Coinbase deferred candles on catalogue failure

A `FetchCandles` command waiting for Coinbase public catalogue startup now fails once if its current catalogue fails. Its error has kind `internal`, message `Coinbase public catalogue unavailable`, the original discovery failure in `venueMessage`, and no financial outcome. Initial empty catalogues follow the existing discovery failure/backoff path; empty individual Products pages/families remain allowed until the complete aggregate is known. A stale Products generation cannot settle a current waiting command.

Subscriptions remain queued in their original order and resume on catalogue recovery. Discovery retries and the shared native 334-ms REST pacing remain in place. Recovery does not replay rejected candle commands; callers may submit a fresh query. This containment uses the existing failure/completion path; managed request adoption remains a separate change.


### ApeX Omni managed public candle reads

`FetchCandles` uses one engine-owned public Read context. Route preparation,
final claim, response processing and all retries share the original 20-second
raw deadline from command admission. A response at or after that deadline cannot
publish candle results or row warnings. Custom transports must implement managed
prepare/bind/ready/dispatch; a request-only adapter fails explicitly rather than
servicing the read through a fallback.

The native retry budgets remain independent: up to three actual transport read
retries and one signature-refusal resend, at most five physical attempts in
combination. Every attempt passes managed preparation and final release admission.
A route/preparation refusal retains its supplied typed error and does not enter
the network retry loop. Transport exhaustion carries kind `network`; undecodable
HTTP reply exhaustion retains the engine's `internal` kind. Original deadline
expiry carries kind `timeout`. Context admission/retry failures use the
shared engine classification. Initial pre-context refusal can still complete
through the existing numeric error path; an issued owner always finalizes with
its Context, without numeric fallback.

Arguments, unsigned URL/body/headers, daily-end omission, sorted candle JSON,
row-skipped messages, wholly invalid rejection and valid-empty success retain
their native contract. Native API/row/envelope failures preserve the existing
message-only public-read errors; they do not borrow mutation Unknown/Rejected
outcomes from the private error codec. Worker/hosted callers must preserve typed
preparation/network/internal/timeout failures and treat the rejected read as unavailable.
Private reads/writes, history alternatives, discovery, local cached commands and
capability defaults are separate from this candle migration. This behavior does
not establish downstream deployment or live venue connectivity.

The shared context-error converter's `notSent` describes the refused local
operation only. It does not establish that earlier read attempts never reached
the venue or that a whole multi-step command was unsent. Mutation journals
retain their separate authoritative release/outcome aggregation.

## BloFin managed public discovery and candle reads

BloFin instruments, ticker seeds and candle queries use the shared prepared HTTP
lifecycle with explicit public routing and no signing credential. Their original
20-second bound includes route preparation; preparation alone cannot release
HTTP or admit a response. Late, duplicate and unreleased responses cannot publish
the catalogue/ticker seed or complete an old candle command generation. Discovery
retries retain the existing startup backoff and row recovery behavior.

Candle results retain their existing JSON fields, seconds, ascending order and
native volume mapping. Unknown markets reject as `invalidInput`/`notSent` before
unsupported interval validation; unsupported intervals reject as
`unsupported`/`notSent`. A refused prepared transport retains its typed error,
including `unsupported` and `notSent`; a parked preparation that reaches its
original deadline rejects as `timeout`/`notSent`. Custom BloFin transports must
implement preparation/binding/final synchronous handoff, and cannot service these
reads through only legacy `request()`.

NX `fetchOHLCV` consumers (`components/dom/Dom.tsx:237` and
`lightweightCharts/SafeCexKLinesChart.tsx:29`), candle-server and worker/hosted
error receivers must preserve those structured fields and handle the rejected
query promise. Custom transport providers must implement the shared managed
HTTP contract when upgrading. SDK call arguments, result units and subscriptions
remain the same; this establishes no NX deployment or private-request migration.

## Bitrue FIFO public read Context

Bitrue candle and public-trade commands retain the genuine engine-issued Read
Context through their native per-channel FIFO, timer and response callbacks. Each
owner keeps its original 5000ms deadline, including queue wait. A correlated reply
at or after that bound cannot decode or complete the head, clear its timer, or
advance the queue. The existing timer still closes the socket and fails every
pending owner, including live peers on other channels.

The shared entry owner retains completion generation through delayed timer delivery
and unrelated admissions. A repeated in-flight numeric command ID cannot acquire a
new timer or request. Admission refusal is completed once by the engine, without
adapter numeric fallback. Native preflight order/messages, 32-owner queue cap,
empty-cb_id compatibility, timer-derived correlation shape, candle timestamp/volume
normalization and trade payloads retain their contracts. Wrong or duplicate explicit
correlation IDs cannot consume the next owner. Rejected/malformed heads fail once
and allow the next live owner to proceed. No HTTP operation or joined group is
introduced for these single-owner native reads.


### Kraken Spot managed command OHLC

Spot FetchCandles requires managed prepare/bind/final release under one raw 20-second deadline, including all four possible transport attempts. Native GET pair aliases, interval and floor-seconds since remain; limit/to_ms and native last remain ignored. Malformed rows reject the whole response, and successful results stay sorted. Native API/row failures remain message-only.

Three retries now re-enter managed preparation immediately rather than waiting in a zero-delay native timer queue, so expiry stays engine-owned while routing is parked. Exhaustion retains its native warning and the engine-supplied typed failure kind, including actual #1838 integrity errors. Preparation denial never spends a network retry. Expired/retired callbacks cannot publish results or diagnostics or complete a reused command. A shared context error's local notSent does not prove that earlier physical reads were never sent. Initial pre-context failure, unsupported interval, background discovery and signed/futures paths remain separate migration tails.


### Orderly managed command candles

The six Orderly broker frontends prepare, bind and claim unsigned command candle requests through the managed transport. Each invocation selects bounded `/v1/tv/kline_history` or streamed `/v1/kline`, retaining native symbol, interval, bounds, limits, filters, sorting and result codecs. Neither branch adds automatic retries. Native malformed/failed responses keep message-only read errors; a failed stream cannot publish its valid prefix.

One raw 20-second root starts before preparation. Unsupported transport preparation and actual network, timeout or integrity failures retain their typed category. Exact delivery validation precedes rows, diagnostics and contextual completion; expired/retired callbacks cannot publish for a reused command. Initial pre-Context issuance refusal, invalid/unsupported/future-empty local branches, cached account identity and signed/background/history/trading paths remain separate migration tails. A context error's local notSent is not proof that earlier physical reads were never sent.

## MEXC single public candle read Context

`FetchCandles` retains one engine Read Context from command admission through
route preparation, binding and final release. Hosts must execute managed HTTP
preparation/binding/final claim and handoff before sending the unsigned public
GET. Native test fixtures use the shared prepared-actions executor. Preparation
time consumes the
original 20-second budget. Expiry rejects once, and late/duplicate transport
callbacks cannot complete a reused numeric command or update HTTP429 pause state.
Typed route refusals, Network failures and Timeout expiry retain their causes;
public reads provide no account credential evidence. Existing unsupported
interval/unknown-market messages, native error messages and no-retry policy stay.

Candle query and results retain native symbol/interval encoding, floor-second
bounds/timestamps, OHLCV ordering, all rows when no limit is supplied and tail
selection for an explicit limit (including zero). Unequal native columns fail
atomically. A current HTTP429 response still pauses depth and account reads.
The other MEXC HTTP consumers keep their existing transport paths.

## Public managed HTTP static headers

Native adapters can use `Cx::request_managed_with_headers` for public requests
with fixed codec headers, including JSON POST `Content-Type`. The engine validates
the ordered static declaration and captures it once before allocating transport
resources. Invalid headers return the original pending value with typed refusal.
Header names, values, duplicate order and absent versus empty bodies retain their
declared bytes through the existing prepare/bind/claim path.

Credential-bearing contexts still require the signed entry point and trusted
provider. Static headers grant no additional routing, retry, command, publication
or financial authority. Original Context deadlines and atomic resource admission
apply; host, WASM and SDK transport schemas are unchanged.

## KuCoin Spot unsigned discovery Context

KuCoin Spot catalogue and ticker seeds use the shared managed HTTP lifecycle with
public intent, including when private credentials are configured. The native
X-CHANNEL-API-CODE=3zexp then Content-Type=application/json headers retain their
order and bytes. Catalogue rows come from `data`; ticker rows come from
`data.ticker`. A request identity alone cannot publish discovery rows before the
prepared transport is bound and the engine releases it. Expired or duplicate
responses cannot overwrite the catalogue or ticker seed.

The original 20-second read deadline includes preparation. An admitted preparation,
transport, envelope or list failure retires its read Context before the native
discovery retry callback. Unusable lists preserve existing quotes. Catalogue success
still starts private snapshots before ticker readiness; usable tickers still wake
wallet valuation and public/private bullet acquisition. Candle, private-signing,
setting no-op and capability behavior retain their existing paths.


### Backpack managed standalone depth queries

`fetchOrderBook` uses genuine engine command entry followed by managed unsigned
GET preparation, binding and final claim. Custom transports preserve the exact
`X-Broker-Id: 1200` header, encoded symbol, empty body and default route through
handoff and stale-route preparation. A request-only transport cannot dispatch
this path. One original 20-second deadline includes preparation; the query keeps
zero automatic retries. Expired, duplicate and superseded callbacks cannot
complete another owner or publish a late result.

The #1680 public `notSent` presentation remains for contextual preparation,
transport and native-response failures, with the engine-supplied typed kind and
all native fields intact. Network failures carry `network`, undecodable HTTP
replies carry `internal`, and root expiry carries `timeout`. Native response
classification and the capped Retry-After remain unchanged; credential-looking
public refusals remain non-auth and do not deactivate account credentials.
The explicit new exception is initial engine admission rejection: this Read has
no financial outcome, with `invalidInput` for invalid time or `internal` for
capacity. It emits one engine failure without new request/timer/owner resources;
a revoked entry has no completion rights and produces no new terminal.

All native book rules remain: every limit is ignored, server order and duplicate
prices retain lexical IDs, zero quantities/totals and empty sides are valid, and
malformed rows reject the entire query. Results remain independent of subscribed
book generations and do not update the Store book. The native query needs no
cached market or live subscription. SDK capability readiness still awaits an
existing startup, and callers still handle no-start, failed-start and terminal
engine errors; unseeded native tests do not establish pre-start SDK readiness.


### Kraken Spot OHLC initial command admission

Supported `FetchCandles` now enters through genuine engine Read admission before managed HTTP preparation. This supersedes phase one's initial numeric-refusal exception: a rejected initial admission emits one engine-owned typed failure (`invalidInput` for a non-finite event clock, `internal` for capacity) with no financial `outcome`, and starts no HTTP, preparation or deadline timer. A revoked entry has no completion authority. An existing parked command retains its ownership when another admission is rejected, and a rejected caller can reuse its command ID once admission is available.

This outcome change applies only to initial admission. Issued reads retain their original twenty-second root, up to four total transport attempts, exact query and response validation, typed preparation/HTTP failure handling and native exhausted-read warning. Native pair mapping, interval/since encoding, ignored limit/to_ms and cursor, whole-response rejection, sorted and empty results remain as in the managed OHLC entry above. Unsupported interval remains the separate native preflight. SDK worker/hosted error receivers consume the existing kind/message fields and do not need a financial outcome for a Read; no upstream deployment is claimed.


### Orderly candle initial command admission

Both bounded-history and current-candle `FetchCandles` enter genuine engine Read admission before managed preparation. This supersedes the phase-one initial numeric-refusal exception: a rejected initial admission emits one engine-owned typed failure (`invalidInput` for a non-finite event clock, `internal` for capacity), without a financial outcome or new HTTP/preparation/deadline timer. Rejected and revoked entries return without numeric completion; a parked command retains its owner, and a rejected caller may reuse its command ID after admission becomes available.

The outcome change is limited to initial admission. Both issued endpoints keep the original twenty-second root, zero automatic retries, exact delivery and typed contextual errors. Native symbol/interval/range preflight, future-empty result, URL/query encoding, filtering, limit, sorting, row/envelope validation and cached account identity remain unchanged. SDK worker/hosted error receivers use the existing Read kind/message fields without requiring a financial outcome. This slice closes no private error-classification row and claims no upstream deployment.


### ApeX Omni engine-owned initial candle admission

Initial managed candle work uses the shared genuine command-entry API. This
supersedes the initial pre-context numeric-refusal exception in the earlier
ApeX candle appendix: a refused entry is completed once by the engine, with
`invalidInput` for an invalid clock or `internal` for capacity, and no financial
outcome for this Read. Rejection creates no timer, HTTP preparation or owner;
the command ID can be submitted again, while existing parked owners retain
their authority. A revoked entry grants no completion rights and emits no new
terminal. Custom hosts must preserve these typed failures without adding a
numeric completion or automatic resubmission.

The original 20-second root, independent retry budgets, exact response fence,
query/result codec and native unsupported-interval preflight remain. Admission
of a fresh command grants no additional rights to later retries or callbacks.

## MEXC candle initial command admission

`FetchCandles` borrows initial HTTP command ownership directly from the engine,
without passing a numeric command ID to issuance or falling back to a numeric
terminal. Invalid command time and exhausted owner capacity emit one typed
InvalidInput/Internal rejection before any Context or transport exists. These
initial read rejections have no write outcome or credential evidence. A later
valid invocation may reuse the rejected ID, and capacity refusal preserves all
existing parked reads. Revoked entry custody grants no adapter completion.
The existing 20-second public Read policy, no-retry behavior and issued-Context
transport/expiry error contract remain in force.


## MEXC, Phemex and Polymarket bounded row reporting

MEXC, Phemex and Polymarket use the engine's bounded Once diagnostic memory
for their existing account-row skip notices. Repeated identities remain silent
until native recovery or the shared 4096-entry rollover rearms them. Notice
codes, messages and severity stay the same; Once does not upgrade a remembered
Warn to Error. This changes internal reporting ownership without requiring a
caller option, credential transfer or persisted identity migration.

MEXC keeps coin, position and shared order namespaces and its accepted-identity
filters. Phemex keeps currency-scoped clean recovery and position-leg rejection
filters before symbol-key recovery, inside its write/slice admission guards.
Polymarket keeps valid zero/priced holding recovery before final snapshot
admission and whole-memory recovery only for an admitted clean read. Existing
rows, financial calculations, snapshot authority, signing and HTTP/WS lifecycle
retain their native contracts.

### BingX native dispatch signing and refusal ownership

BingX HTTP signing and listen-key API-key presentation use the shared
host-injected provider. Venue options retain
opaque primary/secondary refs; generic engine/SDK options retain no signing
secret. Ordinary GET work uses the secondary identity when present, while writes
and listen-key creation/renewal select primary. Native sorted query bytes,
integer protocol stamps, typed POST parameters and API-key/source headers stay
unchanged. Worker or custom-host callers must provision the shared provider
where the engine executes; secrets are not copied back into venue objects.

Provider refusal is not a venue authentication response or a signature rejection.
Identity/history/metadata commands preserve the original typed failure; failed
history preparation discards buffered results rather than returning success.
Background read cleanup releases only its captured in-flight reservation, while
obsolete listen-key epochs cannot retire a replacement session. Existing read
backoff, metadata publication and private socket ownership policies remain.

A first refused write is `notSent`. After an earlier write or signature retry
reached the venue, a later refused signer cannot make the whole operation
`notSent`; uncertain prior effects require reconciliation. Accepted IDs remain
in the existing partial result, including when another leg fails before send.
BingX hosted/keyless admission is disabled. Hosted callers use their existing
supported runtime route. Native provider signing uses hex HMAC and API-key
presentation; WebSocket session ownership remains separate.

## BingX managed single-owner reads

Credentialed startup quotes, account identity, candle commands and recurring
private snapshot reads prepare their route before provider signing, using the same native primary
or secondary credential selection and exact header/query/body bytes. A read keeps
its original raw 20s deadline through route/provider delays and its existing
one-signature retry; identity retains its native no-retry behavior. Identity and candle
success and failures complete through generation-bearing Context authority.
Provider or route refusal stays typed and does not become native account-auth
evidence. Account snapshot, revision, settings and stale-owner publication fences
remain native and unchanged.

Candle rows keep native time sorting and seconds conversion. Refused response
rows and decode status are discarded before a signature retry; they cannot
appear in or poison its successful response. Provider, route, malformed-stream
and deadline failures settle the command once, and late responses cannot revive
it. Unsigned public candles retain their existing request path.

Session upkeep and history retain their explicit legacy ownership and existing
native retry/completion contracts. Hosted admission is disabled. These read
adapters require no new caller option or persisted identity mapping.


### Lighter managed candle reads

For both `lighter` and `lighter-robinhood`, Rust `fetchOHLCV` and the REST
fallback behind `listenOHLCV` use the engine managed preparation/binding/final
claim path. Custom worker/hosted HTTP transports must implement `prepareHttp`;
legacy `request()` alone cannot service these candle reads. Report route denial
and preparation failure through the existing typed error path, without a legacy
send fallback. Typed pre-send failures retain their kind and `notSent` outcome.

A candle read has one original 20,000 ms raw upper bound including route
preparation; a shorter host request timeout can shorten it. Streaming progress
does not renew that bound. Expired/retired replies cannot complete a reused
command ID or publish candles to a new subscription. Background failure retires
its read, clears only the matching generation's in-flight flag, emits no command
result and allows later fallback polling. Command failure rejects the existing
OHLCV promise. Candle shapes, seconds timestamps, sorted results, interval/range
validation and recovered-stream dominance remain unchanged.

Chart callers and custom `RestTransportContext` adapters consuming
`RustEngineExchange` must preserve typed failure propagation and cancellation
through their existing worker/hosted round trip. This requires no caller method
signature or chart data-shape change. Private Lighter requests, other public
reads and native Schnorr/nonce ownership are separate migrations; this behavior
does not establish NX adoption, deployment or live private connectivity.

## Paradex independent public startup Read Contexts

Paradex catalog `/markets` and ticker `/markets/summary?market=ALL` GETs use the
shared managed preparation and final claim protocol. Custom transports must
complete that handshake before delivering a response. Each independent startup
attempt has one original 20-second bound covering preparation and HTTP; expiry,
preparation denial and transport failure retire the attempt, preserve the current
Store and schedule the existing startup Backoff. Late callbacks cannot publish.
These background reads emit Warn diagnostics and no command result.

Catalog publication still starts private initialization without waiting for the
ticker seed. Native metadata/ticker shapes, row filtering, live ticker dominance,
public feed policy and successful startup publication retain their contracts.
The standard hosts already implement managed preparation. The tealstreet-next
venue setup/load consumers continue reading the same Store events and fields;
custom raw-HTTP transport adapters must implement the shared handoff.

## Hyperliquid perps candle command ownership

Perps FetchCandles, including HIP-3 markets and perps on an account with merged
spot markets, enters shared command admission before interval, market or range
validation. At exhausted capacity it receives the engine's correlated Internal
failure with no financial outcome; it cannot retire another owner. With capacity,
native validation messages and unclassified metadata remain. Standalone and
merged spot candle commands retain their existing path in this area.

The unsigned JSON POST keeps its native REST route, Content-Type, raw coin,
field order, limit/time geometry and encounter-order candle output. Its original
20-second deadline includes preparation and stream progress cannot extend it.
Preparation failures preserve shared typed metadata (NotSent); released network
or deadline failures retain Network/Timeout with no financial outcome. Native
HTTP and malformed candle response errors remain unclassified. Failed stream
prefixes never become a partial result, and stale or duplicate delivery cannot
complete a reused command ID. SDK command/result shapes are unchanged; transport
fixtures execute the existing shared Prepare/Bind/Claim contract.


## Crypto.com standalone REST depth on managed HTTP

`fetchOrderBook` for Rust Crypto.com uses the engine's prepared HTTP release
contract, including unsigned public `credentialIntent`. Custom transports must
provide preparation, binding and final synchronous claim/handoff; the legacy
`request()` interface is not a fallback. Route preparation consumes the original
20-second read budget. Success at or beyond that bound is suppressed, and one
typed timeout settles the original owner; late or duplicate replies cannot settle
a later reuse of the same numeric command ID. Typed route/setup refusal remains
distinct from a network failure and supplies no account-credential evidence.

The request remains one unsigned GET to the selected live/UAT host, with encoded
symbol, fixed depth50 and no automatic retry. Native level order, exact price IDs,
zero amounts, totals and result/error shapes retain their existing contract.
HTTP/native error kinds, native code/message and bounded Retry-After remain
available. Auth-looking public errors never invalidate account credentials;
standalone results never change subscription books, tickers or account state.
Worker/hosted callers preserve the typed error fields and surface one rejection;
no signature change or NX adoption/deployment is implied by this adapter change.

Crypto.com standalone depth now uses genuine engine command-entry admission
before any transport work. An initial clock or resource-capacity rejection emits
one typed failure with no write outcome, replacing the old pre-Context numeric
NotSent fallback. Existing owners remain intact; a rejected new command ID can
be reused later. Post-release read Network/Timeout behavior retains its existing
NotSent declaration. No retry, payload or host transport contract changes.

## BloFin candle command admission

FetchCandles admits its original Read owner through the shared genuine command
entry before market/interval preflight or managed HTTP. Initial capacity and
invalid-clock refusals now carry the engine's typed reason and Read outcome
convention; they emit exactly one failure without a fabricated Context or numeric
fallback. A duplicate entry leaves the original preparation and completion
authority intact. Existing market-before-interval validation, native candle
request bytes, result sorting and preparation/release/expiry behavior remain.

## WEEX trusted passphrase setup and managed header declaration

WEEX's trusted signer setup requires a passphrase for a complete private primary
identity. An absent or JSON-null secondary passphrase inherits the opaque primary
passphrase reference; an explicit empty secondary passphrase rejects instead of
inheriting. Public mode bypasses only the primary requirement and still validates
a supplied secondary identity. Secret and passphrase bytes and caller-assigned
stable IDs remain literal, without trimming or automatic secret decoding.

The opaque WEEX HTTP recipe declares Content-Type and User-Agent from the same
static data as its unsigned renderer, so managed transport preparation sees those
public headers before signing. API-key, signature and passphrase headers remain
provider-owned. Provider injection is enabled by the dispatch cutover below;
hosted signing stays disabled.

### WEEX dispatch signing cutover

WEEX signs every private request at dispatch through the shared host-injected
provider. Venue and engine options hold only opaque primary/secondary identities.
Raw `key`/`secret`/`passphrase`/`readCredentials` in engine options are refused
whenever signer identities are supplied. Legacy raw native/WASM input still goes
through trusted setup. Worker and custom-host callers must provision the provider
where the engine runs. GET reads select the secondary identity when provisioned;
POST requests and the private WebSocket upgrade select the primary. Wire bytes are
unchanged: header order, literal secret bytes, sorted native queries, Base64 MAC.

A missing required passphrase is now refused synchronously by trusted setup with
the shared `Invalid local signer setup` message, replacing `WEEX requires a
passphrase`. A provider-supplied primary identity without a usable
key/signing/passphrase ref still deactivates with CONFIG_INVALID rather than
publishing an empty account. An unusable secondary identity stops startup before
any I/O. Read credentials no longer stand in for an unusable primary identity.

**FetchAccount identity changes.** The returned `userId` is the trusted
`primary.stableId`. It is no longer `weex:` plus the lowercase hex of the first
12 SHA-256 bytes of the primary public key. Without a stableId the command fails
typed (`unsupported`, `notSent`) before any I/O. NX already holds the public key,
so it can compute each account's old ID once at cutover and map stored records
to the stableId it provisions. The SDK and engine never compute it.

Identity and FetchLeverage run as managed engine-owned Read commands. Provider,
route and lifecycle refusals are typed and are never venue-auth evidence. The
joined account cycle, trading and history keep their native owners, signed
through one temporary staging site until the shared Plan/execution-group API and
history Completion bridge own them. A provider refusal leaves an unsent write
`notSent`. Once an earlier step was accepted, the command's outcome is neither
`notSent` nor `rejected`. History refusals complete strictly. Hosted
`signHttpRequest`/`signWsAuth` for WEEX refuse until hosted admission passes its
own gates; only explicit `engine: 'rust'` accounts are affected.

### Bybit RefreshMarkets execution group (#1854; backfill)

`refreshMarkets` on the Bybit engines (`bybit`, `bybitinverse`, `bybitspot`,
`bybitdemo`, `hyrotrader`) now runs through the engine execution-group
authority. The call shape and success value (`null`) are unchanged.

- Joined calls: a call made while a walk is running joins it and sends no new
  requests. Every joined call settles exactly once from that walk. Each
  caller's deadline is its own 30 s bound, capped by the walk's bound (set by
  the first caller), so a late joiner still has only the walk's remaining
  time, as before.
- Timeout: kind stays `timeout`; the message changes from `Bybit market
  refresh timed out` to `Execution deadline expired`. The outcome is unset
  (read). Each joined caller gets this once.
- Capacity: one walk serves at most 64 callers. A 65th concurrent call fails
  at once with message `Engine request capacity reached`, kind `internal`,
  outcome `notSent`. The running walk and its 64 callers are unaffected, and a
  call after the walk ends starts a new one.
- Founder departure: the walk is not owned by the first caller. If that
  caller's observer is detached, the walk continues for the remaining callers.
  Bybit itself has no caller-side cancellation for this call today, so this is
  an engine guarantee rather than an SDK-visible action.

Engine capacity refusal text: every engine request-context capacity refusal
(execution-group joins above, Binance command entry, Hyperliquid candle reads
and any other `Capacity` refusal) now reads `Engine request capacity reached`
instead of `Too many requests in flight`. Kind `internal` and each site's
outcome are unchanged: a refusal keeps whatever outcome it already carried,
for example none on a Read command-admission refusal (engine control
`unissued_read_command_is_rejected_once_without_a_numeric_fallback`). The old text matched the `rate_limit` health
pattern (`/too.*many.*request/i`), so NX showed a rate-limit toast and a false
rate-limit health signal for a local engine bound. The new text matches no
`errorPatterns.ts` category.
- Unchanged: shared-feed refusal, `Bybit market catalogue is not loaded`
  (`invalidInput`) and the discovery rate-limit refusal (`rateLimited` with
  `retryAfterMs`); request URLs, query and the `X-Referer` broker header; the
  atomic catalogue commit.

No tealstreet-next caller matches on the old timeout text (the review grepped
the tealstreet-sdk TS packages; `rust.market-refresh.test.ts` checks neither
text nor kind). Callers branching on `kind === 'timeout'` need no change.


## Profile-specific STP and income capabilities

AsterDEX advertises `stpControls` only for API V3. V1 consumers must hide both
account STP controls (fetch/change mode) and the per-order `stpMode` option. V3
keeps all three surfaces. The native version checks and SDK's V3 checks still
reject direct V1 STP attempts before I/O.

Bybit Spot and HyroTrader advertise `incomeHistory: false`; Bybit linear, inverse
and demo keep it true. Coinbase advertises `incomeHistory: false` for Spot, INTX
and CFM because none of these adapters implements an income-ledger command.
CFM's accepted native position PnL is separate from an income ledger.

The existing SDK capability adoption installs branded empty income methods for
these unsupported profiles, matching their TS fallback methods. Callers must gate
income polling and ledger coverage on `capabilities.incomeHistory` (and
`incomeRange`); an empty fallback is unavailable coverage, not a fetched complete
ledger. Credentialed direct native income commands retain their existing
pre-I/O refusals; legacy keyless shortcuts retain their existing behavior.

## Polymarket unsupported history capability claims

Polymarket reports `fillHistory`, `orderHistory` and `incomeHistory` as false
for every signer mode and both public-only and credentialed construction.
These handlers already reject native history commands as unsupported; account
identity, live orders/positions, order-book queries and trading are unaffected.
The TypeScript Polymarket exchange already inherits the branded base history
fallbacks. The Rust SDK now installs those same empty fallbacks for historical
fills, historical orders, income and ranged income instead of dispatching the
unsupported native commands.

Callers must gate history polling and coverage on the capability record. An
empty branded fallback means unavailable history, not a fetched complete
ledger; live cached rows are separate. A future Polymarket history implementation
must re-enable each axis in the same PR that supplies its valid-state dispatch.


### TxFlow indexed history shares accepted-page completion

Fills, historical orders and indexed income stages now use the engine
CursorHistory/Completion owner. HTTP/native, transport, decode and malformed-row
failures reject with kind `internal` when no eligible output was accepted. A
later actual failure returns only validated prior pages/stages with one command-
scoped Warn `history.incomplete`. The failed page's streamed prefix is excluded;
a filtered-only page cannot authorize recovery. Earlier fill-derived income and
funding remain available if the later ledger stage fails. This replaces the
previous whole-read rejection after later page/stage failures. No write outcome
is added; transport and decode retain the shared Internal fallback.

Empty/repeated no-progress continuation and the native offset cap remain strict
refusals even after accepted output. Terminal offset 200000 can complete; a
required offset 202000 rejects. The budget resets per native income stage while
accepted output and identities persist. Losing account identity before a native
leg also remains strict, now with Internal classification. Initial testnet/
identity, range/limit, symbol, unsupported public-trade/lookup and selected-empty
ledger admission contracts retain their existing behavior.

Native offset steps remain 2000, and pagination continues before applying the
caller's final output limit. Original time sorting, numeric fill-ID tie sorting,
generic income/order tie order, exact IDs, delisted/archived symbol mapping,
financial signs, timestamp units and result shapes are preserved. A validated
empty result remains complete `[]`. No history Store publication, SDK argument,
signing change or managed Context migration is added. Callers must ingest
command-scoped incomplete warnings, gate coverage accordingly and treat typed
rejection as unavailable history. This SDK contract does not establish NX
runtime adoption. [Scope](RUST_OWNED_HISTORY.md).


### Orderly history uses the shared accepted-page owner

Fills, regular/algo orders, settled funding and public trades now complete
through the engine CursorHistory/Completion owner. An HTTP, transport, decode,
missing-list or row-validation failure before any eligible accepted output
rejects with kind `internal`. A later failure returns only prior validated
eligible rows plus one command-scoped Warn `history.incomplete`; streamed rows
from the failed page are excluded. Regular orders remain available if the later
algo read fails. A prior page containing only filtered rows cannot authorize
partial recovery. This replaces the former whole-read rejection after a failed
continuation or algo read.

Native metadata inconsistencies, impossible/no-progress continuation and the
fifty-page budget remain strict refusals, even after accepted output. The budget
spans both order endpoints; a terminal fiftieth page completes. Native signing
setup/refusal also stays strict and now has the shared `internal` classification
(previously unclassified), with no write outcome added. Initial unsupported,
credential, time, limit and market admission checks retain their existing errors.
Transport and decode errors share the Internal fallback because they reach the
same terminal callback.

Exact IDs and regular/algo identity spaces, nested protective order mapping,
settled-only funding, public trade occurrence keys, query/signing bytes,
account-wide funding start spacing, per-stage limits, final sort/truncation and
row shapes are preserved. A valid empty result remains complete `[]`. There is
no history Store publication or managed Context adoption; this adapter retains
explicit legacy completion authority. Callers must ingest command-scoped
incomplete warnings, gate coverage accordingly and treat typed rejection as
unavailable history. SDK adapter work does not imply NX runtime adoption.
[Scope](RUST_OWNED_HISTORY.md).


### MEXC Futures history owns accepted pages across native windows

Fills, order history, funding-only income and public trades complete through
the shared engine CursorHistory/Completion owner. HTTP/native, transport, decode,
malformed-row and funding-page failures reject with kind `internal` when no
eligible output page was accepted. After accepted output, an actual failure
returns only those accepted pages plus one command-scoped Warn
`history.incomplete`; failed-page streamed prefixes are excluded. This replaces
the previous whole-read rejection after a later page or older-window failure.
A validated page with every row filtered out cannot authorize partial recovery.
Malformed funding-page metadata likewise excludes that page's entire prefix.
Transport and decode share the existing failure callback and use the Internal
fallback, without adding write outcomes.

The existing 1000-page cap remains a strict refusal, even after earlier accepted
rows; a terminal page 1000 succeeds. Each native 90-day window retains its own
cap, while the logical owner, accepted output, identities and global result limit
survive window transitions. Output retains native encounter order. Exact
newest-first window bounds and request sequence, signing/header bytes, page-size
selection, funding metadata rules, contract units and timestamps are preserved.
Order-specific fills and public trades stay single-request reads. Existing
unsupported-ledger/client-ID, credential, market, time, window-count and limit
admission errors stay unchanged; valid empty output remains complete `[]`.

No history Store publication, SDK argument, row-shape or managed Context
migration is added. Callers must ingest command-scoped incomplete-query warnings,
gate coverage accordingly and treat typed rejection as unavailable history.
This SDK change does not establish NX runtime adoption.
[Scope](RUST_OWNED_HISTORY.md).

## Phemex bounded fills, closed orders and public trades

Fills, closed-order history and public recent trades use the shared HistoryRows
owner. Eligible rows remain staged until the entire single response validates.
HTTP, transport, malformed/oversized body and request-preparation failures reject
with kind `Internal` where these errors were previously unclassified. Existing
messages are retained, including the shared wrapper's HTTP diagnostic; these
read failures gain no write outcome. The failure callback combines transport and
decode paths, so `Internal` is a fallback rather than a network-only claim.
Callers must handle typed rejection as unavailable history instead of assuming
an empty result.

These remain bounded one-response endpoints, with no continuation, earlier
accepted page or incomplete marker. Private fills/orders keep offset zero and
the requested limit clamped to 1..200; public trades keep the native recent
snapshot and 1000-row defensive cap. Valid empty responses succeed as `[]`.
Stable descending native-time sorting, final limit, output shape, filters,
dedupe and public equal-tuple occurrence IDs are retained. Client-ID lookup,
income, pre-request admission, signing/credential selection and rate-pause
behavior retain their existing contracts. This owner uses legacy numeric
completion; it does not establish managed request or NX runtime adoption.
[Scope and controls](RUST_OWNED_HISTORY.md).

## WOO shared history owner and native continuations

WOO fills (v3 and legacy alternatives), closed orders, income and public trades
share one engine history owner. A terminal HTTP/transport/decode or malformed-page
failure rejects with `Internal` if no eligible output was previously admitted.
A later failure returns only admitted rows, preceded by one Warn
`history.incomplete` event whose `fields.command` identifies the read. Failed
response prefixes and rows removed by query filters do not authorize recovery.
Read failures gain no write outcome. The shared callback combines transport and
decode failures, so `Internal` is the fallback classification rather than a
network-only claim.

A validated mixed page keeps its immediate per-page `row.skipped` Warn with the
existing message and timing. It also records missing coverage in the history
owner; terminal delivery emits one command-scoped incomplete marker, preserving
the first gap even if subsequent pages succeed. Entirely malformed pages are
failures, never accepted empty pages. Public trades remain one response: they
have no earlier page to recover, although a mixed snapshot can carry coverage.
Callers must correlate the coverage marker before consuming the result, treat
its rows as incomplete, and handle typed rejection as unavailable history.

Native census/metadata, no-progress, complete-income row-budget, range truncation
and global 1000-attempt refusals remain strict, with no result or coverage marker.
A terminal response at attempt 1000 succeeds; a required attempt 1001 is refused
before sending. The one budget spans symbol alternatives, failed/empty attempts,
legacy pages and v3 windows. Ledger continuations keep the shared 6000-ms spacing;
resuming a timer does not consume another attempt.

Unsupported client-ID lookup, pre-request admission and consuming signed-request
setup refusal keep their legacy behavior, including unclassified setup errors.
Native endpoints, query/header/signature bytes, symbol alternatives and fallback
order, defaults/filters, identity and money precision, row shapes, descending
time/id sort and final limits are preserved. History never publishes live Store
rows or returns the live cache as complete history. This adoption retains legacy
numeric completion and does not establish managed requests or NX runtime
integration. [Scope and controls](RUST_OWNED_HISTORY.md).

## Non-pageable history coverage: Kraken Spot, BreakoutProp and Hyperliquid Spot

Fills, order-history and income queries on these adapters still return `[]`
without a native history read. The shared history owner now emits one Warn
`history.incomplete` event before that result, with exactly `fields.command`
identifying the query. The marker survives Error-level filtering. An empty
Store, missing credentials, public-only mode or live cached orders cannot prove
history absence; these results are explicitly incomplete, never authoritative
empty history.

Callers must correlate the coverage marker with the command and gate any
completeness-dependent use of its result. Gate unsupported polling using venue
capabilities and deduplicate user-facing alerts rather than displaying a fresh
alert for every poll; retain each command's coverage fact even when UI alerts are
suppressed. The engine does not suppress or rate-limit that coverage warning.

Public-trade queries keep their existing refusal text but now report
`kind=unsupported`, `outcome=notSent`, matching the pre-I/O refusal convention:
no request was sent. They emit no history result or incomplete marker. Client-ID
lookup misses keep `{"found":false,"definitive":false}` and no history marker;
Hyperliquid's existing earlier cache-hit and identity/client-ID shortcuts remain.
Live Store/cache access stays separate from history. No REST capability, result
wrapper, managed-request migration or NX runtime adoption is added.
[Scope and controls](RUST_OWNED_HISTORY.md).


### Paradex managed independent book snapshots

Rust `fetchOrderBook` uses the engine managed preparation/binding/final claim
path for the existing public Paradex GET snapshot. Custom worker/hosted transports
must implement preparation; legacy `request()` alone cannot service this query.
Typed preparation refusals retain their engine-provided kind/outcome and reject
the existing snapshot promise. One original20,000ms raw upper bound covers route
preparation and response handling; a shorter host timeout may shorten it. Late,
duplicate or retired replies cannot finish a reused command ID.

Production/testnet URL selection, encoded raw symbol, default20 and explicit
unclamped depth, side order/duplicates/zero sizes, quote-product totals and the
current native non-auth/rate error metadata remain unchanged. No query retry is
added. Snapshot completion never publishes live SBE/JSON depth or changes JWT,
private account state or trading's separate optional capped-order quote path.
The previously audited caller scope remains: no default direct NX/CLI/MCP
snapshot producer was established. This transport migration does not claim
listener/chart migration, NX deployment or live private connectivity.

## Lighter candle command-entry refusal

Lighter and Robinhood command candle reads borrow their genuine engine callback's
initial Read owner through shared `admit_http_command`, before validation or
outbound work. When capacity is exhausted, callers receive exactly one correlated
`CommandFailed` with `Engine request capacity reached`, Internal kind and no financial
outcome. Invalid clock/deadline admission instead returns `Invalid request
context`, InvalidInput kind and no outcome. These replace the adapter's generic
lifecycle-unavailable/Internal refusal. No caller retry or special transport path
is required. Existing successful candle results and native parameter errors keep
their shape; custom transports still perform managed Prepare/Bind/final claim.
Repeated live command IDs retain the engine duplicate diagnostic without a
second result or replacement request. The original candle owner can still finish.
Subscription fallback polling retains its background owner and generation cleanup.

## KuCoin Spot candle Read authority

FetchCandles admits its original Public Read owner before interval preflight and
uses prepared unsigned HTTP with the declared ordered public headers. Its
20-second budget includes preparation. Exact physical delivery is checked before
row staging and result publication; stale, unclaimed and expired responses cannot
complete it. Unsupported interval preflight gains Unsupported/NotSent metadata.
Initial admission refusal has the shared typed reason and no Read outcome;
immediate HTTP-queue refusal retains NotSent.

Preparation, transport and expiry failures retain their typed errors and the
existing correlated error companion before rejection. Decoded malformed candle
rows and unsupported preflight keep their companion-free failure timeline. The
native query order, flooring/u64 casts, default500/cap1500, unknown-symbol HTTP,
candle codec, duplicate timestamps, oversized results and ascending sorting are
preserved. Private credentials, successful setting no-ops and capability policy
retain their existing behavior.

## MEXC initial plan-list credential refusal

An initial Plan snapshot with an existing parsed native credential refusal now
rejects startup with AuthInvalid, AuthExpired or AuthPermission, matching the
initial position-mode read. Native venueCode and venueMessage are preserved,
including complete error envelopes without a data list. A primary-key AuthInvalid rejection
still deactivates; secondary-key refusal retains the existing non-deactivation
policy. Startup publishes one terminal failure and cannot initialize orders or
resume account polling, snapshot pages or asset refreshes through late callbacks.

Transient failures, bare HTTP401/403 without a parsed native refusal, successful
envelopes missing their list and refusals after initial orders loaded retain
their existing retry behavior. No caller option or credential migration is
required; callers awaiting startup receive the existing typed rejection.

## Phemex unsigned candle command transport

Phemex FetchCandles uses prepared managed HTTP when neither its constructed
primary nor secondary credential container exists. Custom transports prepare,
bind and synchronously claim that public request; no legacy send fallback is
used in this branch. The original issued20-second Read deadline covers route
waiting and released HTTP. Initial admission rejection is typed, exactly once
and has no write outcome; released Network/Timeout failures declare the public
read NotSent. Late or duplicate callbacks cannot finish a reused command ID,
change account credentials or pause a later read.

Credentialed public candles retain their existing native signed request path,
including primary/secondary GET selection even with public_data_only. This
partial change does not migrate signing or require credential transfer. Native
host selection, symbol mapping/encoding, interval and limit/from/to rules,
data.rows mapping, sorted results, errors and current rate-limit pause remain.

## MEXC standalone book command authority

MEXC FetchOrderBook uses one public Read Context through prepared route binding,
final synchronous release and completion. Custom transports must prepare, bind
and claim; legacy request dispatch is not a fallback. The issued20-second bound
includes route waiting and released HTTP. Initial engine rejection publishes
one typed terminal without a write outcome; issued public failures retain their
NotSent declaration and typed setup/Network/Timeout causes. Late/duplicate
replies cannot complete a reused owner or extend the depth/account cooldown.

The unsigned native depth URL, captured contract size and limit1..1000/default100,
sorted base amounts and totals, zero quantities and exact u64 version string
remain. Numeric overflow in rows beyond the requested output limit still rejects the
whole snapshot. Native status/code510 classification, code/message and bounded retry
hint remain; live rate limits pause depth and account reads without resending.
Results never change subscribed books, account state or credential health.

## Backpack and Paradex order snapshot freshness

Backpack and Paradex publish order snapshots through the engine's shared
freshness token. Reads invalidated by native writes cannot replace Store orders
or their native signing, terminal or trigger metadata. Existing per-order venue
timestamps, terminal retention and Paradex amend-evidence checks still protect
against lagging server data. Backpack keeps its newer-read-wins behavior; repair
requests start only after an accepted snapshot.

Backpack retains pending trigger relabeling when a stale or failed read is
discarded, so a retained take-profit receives its correct kind when the ticker
seed arrives. Fully validated native Pair acknowledgements also fence older
reads, including when no row is stored locally; malformed response identities
keep their existing rejection behavior. Command IDs, receipts, completion shapes,
fill quantities and signing payloads retain their native contracts.

If snapshot authority is permanently exhausted, these families emit an Error
snapshot-discard notice and issue no order read. Account cycles continue through
their ordinary next stage and poll schedule; accepted native writes and stream
updates remain usable. Existing SDK callers need no new option or state migration.

### Lighter cached leverage and account-tier Read ownership

`fetchLeverage` and the `FetchAccountTier` extension now admit the engine's genuine
current Read command before consulting their synchronous caches. Finite admitted
calls retain leverage JSON, native missing-setting errors and tier string/null;
tier fetch remains cache-only and does not trigger accountLimits refresh.

At shared owner capacity or with a non-finite clock, these calls now produce one
typed engine Read refusal (no financial outcome) rather than bypassing admission.
They allocate no HTTP, preparation, signer, socket or timer and need no new
transport hook or external caller signature. Duplicate IDs cannot replace a live
owner, and immediate completion releases capacity for the next call. SetAccountTier,
account refresh cadence, signing and all other commands keep their native paths.

## Script adapter batch failures retain SDK-reported IDs

`createCoreAdapter.placeOrders` rejects with the exported `ScriptBatchOrderError`
when its SDK failure contains a nonempty string array of `partialOrderIds`. Its
message is the original SDK message, and `cause` retains the original failure.
The error owns a frozen copy of those IDs and preserves a valid `outcome`
(`notSent`, `rejected` or `unknown`) exactly; a missing outcome stays absent. Valid
nonnegative integer `partialOrderCount` and `totalOrderCount` remain available.
Other allowlisted SDK fields and reporting state remain on the rejected error
through the canonical `serializeCommandError` validator; arbitrary cause fields
are not promoted. Ordinary failures and local validation errors retain their
original identity.

The failed host write event stays `ok: false` with its existing error string,
and carries the same partial IDs, outcome and valid counts. It does not expose
them as successful `orderIds`. The successful batch result remains unchanged.
Web, CLI and mobile host bindings share this adapter contract.

Scripts catching the rejected batch and hosts consuming `HostWriteEvent` can
reconcile the reported IDs separately. A partial failure is never whole-batch
success or permission to retry the entire batch. An `unknown` outcome remains
uncertain evidence; callers must not upgrade it to confirmed placement. An
uncaught `runScript` failure still has a string-only `RunScriptResult.error`, so
Agent/automation hosts must retain the failed write event for these details.

## WOO family WASM venue throughput

Standalone WOO family builds compile the WOO venue crate at opt-level3. This
applies to public and private WOO execution in browser/Worker and Node family
artifacts. Callers download a larger module within the existing fixed size cap;
SDK construction, endpoints, signing, transports, row shapes and errors retain
their contracts. Combined/native builds continue using the package size profile;
this supersedes the earlier package-profile description for the WOO family only.
[Profile and controls](RUST_WOO_PUBLIC_CPU.md).

## SDK master fee subjects and Lighter placement identities

Hyperliquid fee reads use the nonempty vault, then nonempty applicationId trading
account, never the API-agent key. Empty identity strings are absent in the same
existing account selector used for other account reads; an empty vault no longer
masks a configured master. Agent discovery finishes before fee/private bootstrap.
Public-only and unresolved startup retain default taker0.00045/maker0.00015 with
no userFees request. Configured private accounts retain their native fee response
and fallback behavior.

Robinhood Lighter defaults to integrator47296 on both deployments, with maker0 /
taker250 attribution. An explicit integrator override wins; ordinary Lighter's
mainnet158385 and testnet no-default behavior is unchanged. Existing tier and
builder-approval preflights run before nonce allocation/submission. A parsed
missing, expired or insufficient approval blocks, as do existing405/429 handling;
malformed responses and other lookup/transport failures retain the adapter's
existing best-effort continuation. This parity change does not tighten that
failure policy. Callers must approve the Robinhood partner identity for attributed
placements to be accepted by the venue.
Every prepared lot, bracket leg, single and batch reserves a positive48-bit
client-order index from the existing instance namespace. Current time/entropy is
reconsidered for every reservation; a backward clock or failed/unknown send never
releases its prior index. Exhaustion rejects before submission rather than
wrapping to a reused identity. Transaction nonces and signer authority are separate.

## Robinhood master-parity fixture provenance

The real-WASM boundary fixture now supplies an approved test integrator matching
the alias or explicit override, and asserts the signed Robinhood47296 attributes.
Production approval refusal, signer indices, slippage, settings and tier behavior
remain unchanged. The two private replay tape identities reflect the authorized
master/harness integrator change; their eleven existing diagnostics each and all
public declarations are unchanged. Raw replay reports remain FAIL with known
failures; this content-hash refresh does not establish complete venue parity.

## Managed native-plan admission and preflight

Rust embedders enable the engine's `http-plans` Cargo feature to use Preflight
and Plan APIs. Bybit and BingX enable it through their venue dependencies. Other
standalone families omit Plan state, scheduling and callbacks until their own
adoption enables the feature; ordinary managed Context behavior is unchanged.
A venue overriding `on_http_plan_ready` or `on_http_owner_retired` must enable
`http-plans` in its own engine dependency; workspace feature unification does
not establish that its standalone family configuration has opted in.

Managed commands using Preflight retain one command generation while prerequisite
reads run. Each read keeps its declared deadline and retry policy. Whole-plan
admission starts only after the final native steps are known; it reserves the
complete declared work, including pacing, retry attempts and replacement branches,
before the first write. The checked native envelope can exceed 120 seconds; there
is no fixed product command cap. Repeated external mode changes may restart
preflight reads without a new whole-preflight limit.

Initial admission belongs to the genuine command callback. An unissued read
refusal emits one typed failure without a write outcome; an unissued write
refusal reports NotSent. NX must consume that failure once. A cached terminal,
issued owner or rejected entry cannot be reminted in the same callback.

Resource refusal before a write Plan is admitted leaves the preflight owner able
to report its typed failure and proves no Plan write was sent. Once work is
released, a timeout or later queue/preparation refusal retains all prior accepted
facts and unresolved effects in the command journal. NX must retain the structured
kind/outcome, reconcile Unknown effects through reads, and avoid optimistic
success or automatic write resend. A classified, effects-free resend uses the
engine's exact-attempt grant within the original declared call and Plan bounds.

Native codecs validate the exact current Plan delivery before changing cache or
Store state. Structural slot completion proves no financial effect by itself;
only validated write facts grant those effects. Delayed route preparation,
binding, signing and dispatch keep the captured physical deadline. Readiness
callbacks expose only their own declared eligible work and never borrow another
HTTP response or command's entry authority.

A cancel deferred behind a Plan is processed when the placement owner seals,
including root expiry and readiness preparation refusal. It does not require an
unrelated later event to resume or receive its typed terminal result. Existing
uncertainty fences still decide whether that cancel can run.

A Read Plan may return its native current-settings fallback after the first
validated response without issuing a remaining declared read. The engine checks
the current DATA call and retires only unissued work; this cannot complete a
write or abandon another selected call. The original deadline and native cache
publication rules stay in force. NX receives the native result once and must not
treat successful local fallback as proof that cache or Store state was refreshed.


### Proven subset with unresolved write remainder

| Native proof | Command outcome | NX handling |
| --- | --- | --- |
| A validated subset took effect; other targets remain unresolved (`PartialUnknown`) | `Unknown`, with bounded proven IDs and batch position metadata when available | Keep the proven subset, reconcile the remainder through reads, and never resend the whole write. |
| A complete mixed acceptance/refusal is proven (`KnownPartial`) | Partial failure with the resolved facts | Keep accepted results and report refused targets. |
| Count-mismatched, empty-ID or wholly unproven ACK (`Failed`) | `Unknown`; retained IDs are evidence only | Reconcile through reads; infer no Store effect or optimistic success from those IDs. |

A satisfied native cancel may prove a subset without creating IDs. That proof
still leaves any unresolved targets Unknown; a successful subset never cleans up
the remainder or extends its deadline.

## Shared ticker symbol lookup

Ticker symbol reads and updates use the shared Store index. Ticker row order,
last-duplicate lookup winner, replacement/delisting and append-only recovery
indices retain their contracts. Full snapshots and dirty-field deltas keep the
same row identities and signed-zero behavior. Venue code and cached row-index
consumers continue using the same Store API.

Each independently loaded engine/account Store owns its ticker index. The measured
native 965-symbol example adds 55,743 bytes (about 54.4 KiB) retained index allocation
per Store; replacement peak adds 111,486 bytes (about 108.9 KiB) per Store while old
and new maps coexist. Ten similarly sized independent Stores therefore add about
544 KiB retained, with about 1.06 MiB additional peak if all replace concurrently.
The cost occurs per Store, not per message, and grows with symbol count/string
length and hash-table capacity steps. These are native index-only measurements;
actual process/browser/WASM costs include allocator and memory-page effects.

## Engine socket lifecycle: OKX

OKX authentication state, socket epochs, reconnect permission, subscription
replay and retirement use the shared Engine lifecycle. Native venue codecs
provide login/subscription payloads and Store invalidation through the existing
socket actions and events. Retired or expired frames and timers cannot restore
authentication, replay interests, advance private health or publish a parked
signing result. Permanent OKX private/business credential refusal revokes both
retry authorities before callbacks; reconnectable loss retains native recovery.

Current desired book/candle interests survive reconnect. Retirement clears only
the retiring feed's owned public depth before retry; account/history/settings
freshness fences remain unchanged. Physical IDs retain the shared host-safe
integer boundary and signing/remote-signing wire contracts remain unchanged.

Bitunix and all other venue families retain their existing socket adapters.
Bitunix keeps its native pre-ACK reads, first-pong idle activation and shard
pacing. The temporary managed-sockets feature remains until every legacy family
has adopted the shared lifecycle; its inventory includes Bitunix explicitly.

Initial HTTP command admission precedes any managed socket side effect. Socket
registration, retirement, closure and queued work spend a fresh command-entry
loan even when no new transport identity is allocated.


## Declared mutation terminal API (#1894)

An opted-in engine Plan may declare a native conditional terminal before any
release. `done_mutation_plan_after_data` consumes only its exact current
released DATA anchor and retires unissued remainder; it does not manufacture
Accepted facts or renew the native root. A valid terminal whose earlier writes
are uncertain still emits the common journal's failure projection with known
IDs retained. An undeclared anchor, Read Plan, foreign/old/expired delivery or
another issued call refuses without consuming current authority.

The shared seal prepares projection/report storage before commit and keeps one
Cx terminal funnel for the forthcoming execution-observer integration. Native
codecs still prove when their conditional native work is finished. ApeX #1891
is a separate consumer obligation; this engine API alone does not migrate its
writes, provider presentation, remote zkLink, startup or session paths. No TS
caller shape or native result bytes change until an adapter adopts it.


## Context-owned history completion and admission

For adapters using `ReadCompletion` and the `Read*History` types, the history
owner retains the original engine-issued command Read root across pages and
composite stages. Legacy numeric owners keep their existing lifecycle; each
venue's adoption carries its own caller contract. Bind the matching command/root once; a page
leg, foreign issuer, stale generation, background owner or mutation owner cannot
substitute for that root. `ReadCompletion::from_context` validates that binding
without reserving exclusive completion; only the first valid context terminal
can complete the owner. Read ownership does not become numeric completion
authority when a lifecycle check refuses it. Plan and joined-observer ownership
use their own explicit APIs.

Native adapters use the borrowed admission guard for row staging, identities,
accepted output and coverage. The guard checks the exact current HTTP leg and
physical attempt; native response validation and publication freshness are
separate requirements.
Scope the guard around admission, drop it before mutable Cx diagnostics or Store
publication, and re-admit if more staging follows. Native cache/Store effects
outside the history owner require the same synchronous root/leg check.
Read cursor identities stay page-local until admission, so a discarded failed
prefix cannot suppress those rows in a later retry. Discard the failed staged
page before retry admission; a different physical attempt is refused until that
binding and its unvalidated rows and identities are cleared.

Migrate a venue's owner to the Read type as a whole. Handle or deliberately
ignore a Context ownership error; never unwrap it or retry through numeric
done/fail. Stale or mixed-mutation ownership cannot run a captured finalizer,
publish a history.incomplete companion, or consume the proper command owner's
terminal authority. Preparation/policy refusals reject strictly; actual later
page failures can return previously accepted rows with one command-scoped
incomplete Warn. Native strict endpoint-policy refusals remain strict.

SDK/NX callers keep the incomplete fact correlated to the command and treat
its returned rows as partial coverage. An absent or refused managed result does
not establish complete history. Existing native row shapes and result shaping
remain venue-specific. See [the history bridge contract](RUST_HISTORY_CONTEXT_BRIDGE.md)
and [owned history](RUST_OWNED_HISTORY.md).


## Managed order reads reconcile from their earliest leg

A managed order read clears an uncertain-write fence only if its earliest
released leg (page, settle coin or product) was released after the fence.
A read that straddles a fence leaves it set until the next full read, which the
engine already requests. Caller outcomes and frames are unchanged; an order row
may stay one extra read cycle after an unknown write.

Legacy `cx.request` order reads still carry the per-attempt overclaim: a paged
legacy read can clear a fence on rows read before it. This stays true for each
venue until that venue moves its order reads onto managed contexts.

### Lighter public order-book seed uses managed Background Read

The independent `orderBookOrders?market_id={id}&limit=100` snapshot now waits for
prepare/bind/final claim in custom worker/hosted transports. It remains unsigned
GET with empty headers/body. SDK subscriptions, snapshot decoding, level units,
book/nonce validation, WS revision/generation precedence and native2s recovery
remain unchanged. This does not establish WS nonce continuity from REST.

Preparation shares the original20s bound with response handling. Expired or
unreleased work cannot publish; failures release only the matching subscription's
in-flight latch and use existing background recovery, never a user's command
terminal. Ticker/catalog/private/trading paths stay native. No caller signature
or financial-outcome contract changes; transports need managed preparation for
this additional background read.

## KuCoin futures local history/funding terminals

Synchronous history/funding preflight errors and zero-limit empty results now
borrow the engine's genuine initial Read authority and complete it in the same
callback. Native validation order, error messages, empty arrays and companion-free
timelines remain. Unsupported client lookup gains Unsupported/NotSent metadata;
local input/absent credential failures gain InvalidInput/NotSent. There is no
transport or private remote read on these branches, so their policy declares no
credential. Valid signed paging continues through its existing separate path.

Initial shared admission refusal emits exactly one engine-owned failure and no
adapter fallback or result. Capacity refusal uses the shared Internal error;
invalid raw clock admission uses InvalidInput, both without a Read financial
outcome. Repeated commands can recover when unrelated roots retire. Callers need
no new endpoint, retry or history marker handling for this local-only slice.
## Bitunix synchronous history Read completion

Local history exits retain their original command Read authority through shared
entry admission and Context completion. Unsupported client lookup/public trades
retain their native text and now carry Unsupported/NotSent; nonfinite time keeps
its text and carries InvalidInput/NotSent. These local refusals still have no
error companion. Genuine admission refusal uses the engine's typed reason once,
including invalid clocks and capacity, without issuing opaque work.

Keyless private queries still return `[]` before time validation. Missing or
unknown fill symbols still return `[]` before finite-time validation. Unsupported
query checks still precede keyless handling. No range-order guard, timer or
transport is added. Valid private history dispatch, signatures, paging and
incompleteness behavior retain their existing contracts and remain legacy-owned.

## Bitrue synchronous history refusal authority

Unsupported complete order/income history and local fill preflight refusals now
consume genuine initial Read authority and finish in the same callback. Their
exact native messages, authentication-before-range-before-symbol precedence,
companion-free timeline and absence of I/O/Store mutation remain. Unsupported
kinds gain Unsupported/NotSent; local input or missing credential errors gain
InvalidInput/NotSent. Shared admission refusal emits its one engine-owned typed
failure, with no adapter fallback or Read financial outcome. Valid private fills,
lookup and WS FIFO delegates keep their existing separate transport contracts.

## Bitrue local leverage preflight authority

FetchLeverage and FetchMaxLeverage retain unknown-market-before-credential
validation and their exact native messages. Those four local refusals now use
shared initial Read authority and Context completion, adding InvalidInput/NotSent
metadata without a companion, transport or Store mutation. Admission refusal
emits exactly one shared typed failure with no adapter fallback or Read financial
outcome. Valid bracket requests and leverage-query joining/refresh/fan-out keep
their existing separate contracts and remain held for shared signer/group work.

## Phemex local history admission

Synchronous Phemex history/income results use genuine engine command-entry
admission before publishing their local terminal. Invalid engine time and
exhausted managed-owner capacity produce one typed engine rejection. Finite
admitted errors retain their native text, kind=None and outcome=None; unknown
client-ID markets still return {"found":false,"definitive":false}.
Native validation order, sixteen-query limits and duplicate-ID behavior remain.
A local refusal preserves outstanding native history/income owners. Successful
HTTP, signing, pagination and financial history completion retain their existing
paths. Affected SDK callers are fetchHistoricalFills, fetchHistoricalOrders,
fetchIncomeHistory/fetchIncomeHistoryRange, fetchPublicTrades and
findOrderByClientOrderId through the Rust command bridge.

## MEXC synchronous history and leverage command custody

The five local history admission errors and three local leverage errors now use
one genuine engine Read command-entry Context before publishing their native
refusal. At the existing 256-owner bound, the engine publishes one typed initial
failure instead; a nonfinite local command clock fails with InvalidInput.
Initial Read admission failures carry no write outcome (outcome=None). An
existing owner sharing the numeric command ID remains protected by the engine's
duplicate-command fence. Normal finite errors retain their exact message and
absent kind/outcome/venue metadata, with no HTTP, signer, timer, AccountAuth or
Store publication. These local errors use no credential or remote transport.

Affected callers are fetchHistoricalFills, fetchHistoricalOrders,
fetchIncomeHistory/fetchIncomeHistoryRange, client-order-ID lookup,
fetchPublicTrades, fetchLeverageForSymbol and updateSymbolMetadata. The public
SDK method fetchLeverageForSymbol retains its existing warning/null catch;
updateSymbolMetadata continues to await the command directly. SDK arguments,
result shapes, history validation precedence and leverage pause/market/primary
credential precedence stay the same. Valid private/public history and leverage
reads retain their existing signed/unsigned dispatch, secondary GET selection,
rows, accepted-page recovery, bounds, revisions and callback behavior.


## Signer non-HMAC prerequisites (#1839, e4c07bbd615)

Backfilled after landing (auditor). Additive schema for families that sign
with more than one key, starting with ApeX's zkLink seed. No family's runtime
behaviour changes until its admission PR declares the new policy.

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| Rust `signing::Identity` gains `extra: BTreeMap<ExtraKey, KeyRef>` (serde: omitted when empty) | native hosts or tests that build `Identity` with a struct literal | `Identity { credential, signing, passphrase, stable_id }` | add `extra: Default::default()` to every literal; JSON without `extra` still deserializes | compile only (Rust literals) |
| `ExtraKey` vocabulary (`zklinkSeed`) and `Algorithm::ZkLinkContract` (36-byte payload, 64-byte result) | none today | n/a | extra refs are role-bound: usable only by their own algorithm, never presented, interpolated or used as an ordinary MAC key | none |
| TS setup policy: `secretEncoding` gains `base64Text`, plus an `extra { key, field, encoding }` signing-ref policy (`requiredForPrivate`) | `LocalSignerSetup` callers and the construction-rules `signerSetup` entries | `utf8`, `base64`, `hex`, `pem` encodings; no extra refs | `base64Text` installs the ASCII of padded base64 of the secret as the key; extras are declared per family and validated at trusted setup | add (admitted families only) |
| `host.d.mts` `SignAlgorithm` adds `'zkLinkContract'` | custom JS host signers that switch on algorithm | previous union | handle or refuse the new member; an unhandled algorithm must refuse, never sign | add |
| Crypto `zklink` feature (enabled by the ApeX venue only) | WASM family builds | n/a | other family builds are unchanged; a provider without it refuses `ZkLinkContract` as `Unsupported` | none

### Paradex unsigned public-trades Context history consumer

`fetchPublicTrades` keeps the native unsigned GET query, limit1..500/default100,
row schema, encounter order, first-ID dedupe and truncation. The endpoint remains
one physical page even when a valid continuation cursor is returned. A missing
market returns admitted []. Private history, JWT and Stark signing are unchanged.

The command now requires managed prepare/bind/claim support in custom transports.
Preparation and the full response share the original20s Read bound; stale or
reused owners cannot emit a result or history.incomplete marker. Malformed or
failed prefixes reject strictly without returning staged rows. Typed route
failures retain their engine metadata; released transport loss is Network and
expiry Timeout, while native malformed/status errors retain Internal. Capacity
admission precedes missing-market completion. Callers treat a rejected read as
unavailable history, not an authoritative empty snapshot; no SDK signature changes.

## Hyperliquid local order lookup admission

Perps and spot local `OrderByClientId` results require genuine command admission.
A cached symbol/client-ID match still returns the complete native Order, including
optional fields, before validating the client ID or account identity. An invalid
client ID or absent trading identity still returns exactly
`{"found":false,"definitive":false}`. These synchronous reads emit no request,
signing, timer, history marker or Store update.

Invalid entry clocks and full shared owner capacity now return one correlated
Read failure (`InvalidInput` / `Internal`, respectively), with no financial
outcome. Callers must handle that failure instead of treating a local lookup as
unconditionally successful. Reusing the command ID after completion remains valid;
an active duplicate cannot complete or replace its existing owner. The valid-ID
uncached spot miss, public-only perps refusal and private perps REST lookup retain
their native behavior. The exchange-server order reconciliation caller consumes
the same complete cached result and must retain non-definitive miss handling.

## KuCoin synchronous local Read terminals

Spot query credential refusal still precedes kind, cache and parameter handling.
Missing credentials retain their message with InvalidInput/NotSent; unsupported
Spot public trades and futures leverage retain their messages with
Unsupported/NotSent. Keyed missing-symbol fills, order history and income still
return[]; cached lookup preserves exact symbol/client/native-ID matching and full
order JSON. The constant Spot leverage projection retains leverage/buy/sell1 and
its existing unknown-symbol behavior, preserving unrelated margin settings.

Shared initial Read admission now precedes these local results. Capacity/invalid
clock refusal emits only the engine terminal; denied leverage reads leave Store
unchanged and denied cache hits publish nothing. No credentials or route are
borrowed for these no-I/O branches. Private fills, user-info, cache-miss paging,
setters and successful margin/position no-ops retain native dispatch and policy.

### Lighter public ticker seed uses managed Background Read

The public `/api/v1/exchangeStats` GET now requires prepare/bind/claim support in
custom transports, with its original20s bound covering preparation and streamed
response together. Empty headers/body, all six native list-envelope aliases,
field conversion, usable partial rows, per-symbol WebSocket precedence and poll/
startup Backoff behavior remain the same. Invalid envelopes discard staged rows;
queued, expired or late responses cannot publish ticker changes. Admission failure
keeps recovery on the existing startup Backoff and completes no user command.
Successful data and SDK method signatures are unchanged for both Lighter aliases
and networks; private credentials, trading and catalog fallback stay native.



## BloFin PublicTrades managed history

PublicTrades uses prepared public HTTP and the original 20-second command Read
owner. A response must match the released, claimed physical attempt before trade
rows or identities enter the shared history owner. Expired, unreleased or stale
responses cannot publish results. Preparation refusals retain their typed kind
and notSent outcome; released transport failures retain network and expiry
retains timeout. Failed first pages publish neither a staged prefix nor an
incompleteness warning.

The native unsigned GET, ordered query parameters, empty headers, default and
clamped wire limit, millisecond timestamps and trade result fields are preserved.
The wire limit does not truncate returned rows. Known cross-market rows, native
row order, unknown-market identity consumption, numeric defaults, contract-size
quantities and side conversion retain their existing behavior. Private history
and signing paths keep their existing lifecycle.

## WOO synchronous history refusal custody

WOO local history refusals require genuine initial Read admission before the
native terminal. Unsupported low-level remote client-ID lookup, invalid symbol/
explicit range/limit and invalid derived range keep their existing validation
order and native error messages at finite clocks. Invalid engine entry time now
returns typed InvalidInput, and shared owner capacity returns Internal, each
without a financial outcome or new HTTP/signing/timer/Store effects. Rejected
entry is already completed by the engine; revoked entry cannot complete another
owner. A settled local command ID remains reusable.

The Rust exchange callers for executions, historical orders, income and public
trades keep their existing rejection handling. Historical positions through
`wooJournal` still obtain historical orders through the same API. This low-level
unsupported lookup is separate from SDK `findOrderByClientOrderId`: with WOO
clientOrderLookup disabled, that caller retains its existing non-definitive
open-orders lookup. Valid public/private history keeps native unsigned/signed
dispatch, credential selection, rows, page recovery, range budgets and pacing.
No managed HTTP-history or signer adoption is implied.

### Paradex local history results require genuine Read admission

Public-only Fills, OrderHistory and Income still return exact `[]`; public-only
OrderByClientId still returns `{"found":false,"definitive":false}`, including
invalid private query inputs. Private-enabled invalid client IDs and explicit
invalid history bounds retain their exact native messages and unclassified
Read errors. Fills without a symbol still returns `[]` after bounds validation,
before authentication. These local results now require genuine engine command
Read admission: capacity and invalid clock policy produce one correlated typed
engine refusal without a financial outcome. Successful admission preserves the
native decision order and introduces no request, signer, timer, Store mutation
or incomplete marker. PublicTrades retains its existing managed public path;
other private history, signing, settings and REST fallthrough remain unchanged.

## Exact background read delivery

Adapters publishing a one-leg Background Read use
`validate_background_read_http_delivery` synchronously in its matching managed
HTTP callback, before retiring the Context or changing native/cache/Store state.
Logical liveness alone does not prove that a different live Context belongs to
that response. Command, Execution and Session owners cannot use this Background-only gate.

Native response validity and publication freshness remain separate checks. A
failure callback can have valid delivery identity without response data to
publish. A refused gate publishes no replacement response and grants no command
completion, retry or send authority. SDK/NX order-error handling and native result
shapes are unchanged; adopting venue adapters retain their own caller contracts.
## AsterDEX dispatch signing admission

AsterDEX now signs only through the injected provider at dispatch
([RUST_ASTER_SIGNER_ADMISSION.md](RUST_ASTER_SIGNER_ADMISSION.md), phases 2-3).
Caller-visible changes:

- **Setup carries the material.** `key`, `secret`, `auth.privateKey`,
  `secondary_key` and `secondary_secret` reach the provider through the one-use
  setup channel (raw WASM constructor argument 3, or native legacy setup). Generic
  options and `exchange.options` keep only the public `auth.userAddress` /
  `auth.signerAddress`. Native callers passing `asterdex.privateKey`,
  `asterdex.readKey`, `asterdex.readSecret` or top-level key/secret beside opaque
  `signing` refs are refused as invalid input.
- **Setup resolves `apiVersion`.** TS and native setup resolve V1/V3 from the
  material's shape and send it explicitly; the venue refuses a missing or
  contradictory version instead of guessing. Keyless V3 callers should set
  `extra.asterdexApiVersion`, since nothing inferred admits a keyless account.
- **Encodings.** V3 wallet keys are trimmed hex, with an optional `0x`/`0X`;
  V1 HMAC secrets stay byte-exact UTF-8. A whitespace-only key is absent.
- **Read agent.** With an `auth.privateKey` wallet, the read agent presents its
  own key and signs with the primary wallet's provider ref (one copy, one
  revocation point), as before. Otherwise it signs with its own secret.
- **Main-wallet hint.** The V1 "API Wallet keys require the Main Wallet Address"
  refusal now happens at setup, with the same message.
- **Read agent errors fail closed.** An empty read secret, or a V3 read secret
  that is not hex, refuses setup instead of failing only the signed reads.
- **`fetchAccount().userId`.** Without an `accountAlias`, V3 falls back to the
  public signer address, as before. V1 previously fell back to the API key; it
  now uses the trusted primary `localSigner.primary.stableId` (WEEX precedent)
  and otherwise fails typed (`unsupported`). It is never the key and never
  empty. claude-6mg5na confirmed tealstreet-next neither reads nor keys on the
  Aster `getAccount` userId (DB, dedupe, cache, telemetry), so NX needs no
  caller migration.


## Managed subscription handoff before native reads

OKX private reconnect account refreshes begin on a subsequent engine ingress
following the handoff of their required subscription frames. The shared
`set_socket_interests_then` API binds native Pending work to the current epoch
and exact required interests. It waits for every paced batch and outstanding
socket signature before arming an ordinary engine timer after final fencing.
This establishes host handoff order; it promises neither peer acknowledgement
nor a later JavaScript event-loop turn. Custom hosts must process the returned
actions in order and deliver timer events through ordinary engine ingress.

A committed removal or wire change cancels the dependent work. Unrelated
additions preserve it behind the new revision; a refused replacement preserves
still-needed work on the unchanged committed revision. Retirement, expiry,
stale timers and duplicate delivery cannot issue its read. The callback has no
borrowed HTTP delivery, command-entry, order-response or Plan activation token;
reads retain their existing admission, signing, timeout and completion rules.

The JS host removes inline-fired timer handles and ignores callbacks belonging
to a cleared or superseded timer arm. Bitunix keeps its existing legacy socket
boundary and its remove-before-master obligation.


## WEEX ordinary trading: complete managed Plan

The ordinary placement/cancel/amend/leverage/margin/mode pipeline now enters
through genuine Preflight authority, admits its entire native serial sequence,
and releases only through the existing primary dispatch recipe and shared
Prepare/Bind/Claim path. Each native call retains 20s and one attempt; eight
serial placement lots can complete over 152s inside their 160s Plan. No write
retries or caller-side key selection were introduced. History/account/public
callers and their temporary generic sender remain separate migrations.

Accepted native IDs survive subsequent provider, refusal, transport, malformed
ACK or expiry failures through the engine journal. Native code 40015 is a system
error and remains Unknown, including when an old fixture's message says
insufficient balance. Unlisted codes do not prove refusal. HTTP 408/5xx use the
engine's typed Unknown reason, with native decoding/publication fenced. Two
existing classification assertions were updated under overseer approval;
payload fixtures are unchanged.

CancelSymbolOrders still attempts its separate ordinary and algorithm families
after an ordinary provider refusal or transport failure: exact failed-call
settlement advances the Plan, and its journal retains the earlier NotSent or
Unknown fact. It never retries either failed call. HTTP 408/5xx still terminate
through the core before ordinary callbacks; no local continuation bypass exists.

Late metadata contains bounded native client/cancel identities. Late replies
remain nonpublishing and cannot produce a second terminal; native Store
publication stays with authoritative active account/reconciliation reads.
An immutable late reducer cannot also update WEEX native tombstone/freshness
state, so freshness-aware late publication remains explicit follow-up debt.

The inventory shrinks exactly 51 ->46 occurrences (42 -> 38 entries), with
trading.rs 5 ->0: two numeric completions plus three conservative macro matches.
No other venue inventory, shared checker, allowance or signer source changes.


## Bitunix public REST Read authority

Markets, tickers and FetchCandles use shared managed unsigned HTTP with a20s
lifecycle bound covering preparation and queue time. Public route and exact
production/testnet headers are preserved; WebSocket behavior stays legacy.
Unclaimed, stale or expired replies publish no rows or successful completion;
engine expiry may emit the original command's one typed Timeout failure.
Candles retain their native interval/query/limit precedence, seconds timestamps,
quote volume, permissive successful code coercions and row order without local
truncation. Admission, preparation and timeout failures are typed; unsent work
can carry NotSent. Shared read classification owns failed-response kinds/retry.
Discovery keeps native rate floors (429 Retry-After15s versus business10005/6
60s); auth/refused failures log a diagnostic, leave the seed unready and schedule
no retry. Recovery requires fresh caller-owned initialization. Public auth
failure spends no private auth budget after actual1923. No private P0#33 closure,
signer readiness or managed socket claim. Exact inventory18->10; only8own sites.

### WEEX synchronous local history Read terminals

WEEX Futures keyless Fills, OrderHistory and Income retain exact `[]` before
private input validation. Client-order lookup remains unsupported. Invalid
family bounds and income/list budgets retain the existing messages, ordering
and unclassified Read errors. These seven local terminals now require genuine
engine command Read admission: shared capacity and invalid clock policy emit
one correlated typed refusal without a financial outcome. Admitted terminals
create no HTTP, signer, timer, Store mutation or history marker. Remote history,
its credential selection and paging retain their existing paths and wire bytes.

### WEEX FetchAccount initial Read authority

WEEX account identity validation now requires genuine engine Read admission for
both its local refusals and its signed accountConfig GET. Shared capacity or an
invalid request clock produces one typed correlated Read failure without the
old numeric CommandError fallback or a financial outcome. Admitted native
refusals keep their validation order, exact messages, metadata and checked
CommandError-before-CommandFailed delivery, including the existing missing
stableId Unsupported/NotSent result. Valid reads keep the selected primary or
secondary GET signer and exact native wire; the returned userId remains the
trusted primary stableId. Account state, polling and reconciliation stay intact.


## BingX synchronous history terminals acquire Read authority

Keyless private history and credentialed Fills without a symbol still return
exact []; keyless precedence, unsupported client lookup and invalid normalized
range messages remain native. These four local exits now require genuine Read
admission without a transport credential. Invalid entry clocks and full Context
capacity emit one typed Read refusal (no financial outcome), with no provider,
HTTP, timer or Store effect. A refused entry never emits a second numeric reply
or retires an unrelated pending owner. Completed local command IDs remain reusable.
Remote history selection, page geometry, signing, streaming and native finalizers
are unchanged; this slice does not claim managed remote history adoption.

## Backpack transitional dual custody (remove before master)

The Backpack registry arm installs a provider copy of the configured API key
and Ed25519 seed, and passes its identity to the venue for managed trading.
Engine options still hold the raw key and seed, but only for the unadopted
account-read and private-WS paths ([RUST_BACKPACK_SIGNER_ADMISSION.md](RUST_BACKPACK_SIGNER_ADMISSION.md),
option B, approved by claude-ddi2il).

- A malformed or non-32-byte base64 seed is now refused at construction, typed
  invalid input ("Invalid Backpack base64 seed" or "Backpack Ed25519 seed must
  be 32 bytes"). Such an account now gets no engine at all, public data
  included. Previously it posted CONFIG_INVALID at account start and kept
  streaming public data. This is deliberate (R3).
- TS construction is unchanged: `dispatchSigning` is off for backpack.
- External signers and injected refs are still refused for backpack.
- This must be removed before master. It ends when account reads, private WS
  auth and repair adopt the provider (codex-hcmq3f). The raw fields are then
  dropped from venue options.

## Backpack managed foreground write results

Foreground place/cancel/amend/leverage uses shared Plan custody, send-time
provider signing and one journal terminal. A failed explicit cancel no longer
returns success; optional child failure returns a failure with all accepted IDs.
A malformed or partially failed Pair retains correlated IDs as partial evidence;
missing acknowledgement rows do not invent total or failed-index counts.
Unlisted native errors and undocumented HTTP statuses cannot prove refusal.

A delivered 408/5xx currently stops the batch as Unknown, preserves native
code/message and prior IDs, and retires unsent legs: reconcile before resending.
Restoring native independent optional-child continuation remains OPEN until the
shared independent_calls policy is available and adopted. Each attempt now has
an explicit 20-second managed bound; the root is derived from declared steps.

Callers preserve partial IDs and reconcile uncertainty before retrying. A typed
pre-send refusal restores only unchanged optimistic cancellation state. Late
replies have bounded correlation and no Store or new-terminal authority. Native
cancel/leverage empty-object acknowledgements remain accepted; malformed JSON
cannot prove a successful managed acknowledgement. Legacy internal repair keeps
its original response behavior.

The native out-of-band Ed25519 cancel proof uses a fixed independent signature
and remote-only key sentinel, with no HTTP before Signed. SDK/browser signing,
registry admission, account/startup/session presentation and internal repair
signing remain OPEN. Raw legacy credentials alone do not admit a managed write;
a trusted identity is required and missing admission fails before write I/O.

Managed Backpack write admission and window selection use only the trusted
identity and Options. The delegated cancel proof uses a venue with no raw key or
secret. Account/startup remains a separate legacy scope. Automatic trigger repair
still uses that legacy account signer; identity-only mode gates it before job
creation/I/O and reports that managed account signing is pending. An owned RED
pins the repair dispatch obligation for the next account/background adoption.

## Backpack transitional SDK custody (owner ruling, 2026-10-24)

Authorized admission preserves current account/private-WS behavior while trusted
setup supplies a provider and opaque identity for managed foreground writes. Raw
material remains only in the unadopted legacy account/WS scope, temporarily beside
the provider copy. Managed writes have no raw/eager fallback. dispatchSigning
remains off and TS setup is unchanged; SDK/browser remote readiness stays open.

This is a remove-before-master obligation owned by codex-hcmq3f. Account/WS and
legacy account-repair adoption must remove retained raw material, end dual custody
and complete ref-only admission. The admission implementation is separate work
owned by claude-m26ps4; this ruling alone claims no SDK availability or merge.

## Binance identity local refusal custody

Binance USD-M and portfolio-margin `fetchAccount` preflight refusals use genuine
engine Read command admission before publishing a result. Missing, disabled or
incomplete credentials retain their existing message and one correlated
diagnostic followed by one terminal when admission succeeds. If the engine
context registry is full, the engine instead emits one Internal failure,
`Engine request capacity reached`, with no extra diagnostic or transport.
These Read failures carry no mutation outcome.

Local identity refusals release their Read ownership immediately, including
repeated calls with the same command number. They do not issue HTTP/signatures
or mutate account snapshots. Valid identity reads retain the selected signer,
request bytes, routes and `userId` shape. Spot and COIN-M identity remain
capability-refused before venue dispatch.


## Hyperliquid whole-family managed socket lifecycle

Hyperliquid perps, standalone spot and merged spot accounts use one managed
socket for their public and private topics. Connect10s, idle45s, ping15s and
native retry/JSON behavior remain unchanged. Duplicate opens and retired or
idle-expired ingress cannot replay subscriptions or publish financial/market
data, even when a watchdog callback arrives late. Shared/Disabled public-feed
policy still permits native private traffic while suppressing direct public
subscriptions and publication.

For an actual private subscription obligation, the complete private-then-public
frame batch is handed to the host before a subsequent ordinary timer ingress
starts the native account/spot refresh. This moves refresh HTTP after the public
frames compared with the earlier private/HTTP/public opening trace; it promises
host handoff ordering, neither peer acknowledgement nor a later JavaScript turn.
Custom hosts must dispatch returned actions in order and deliver timers through
normal engine ingress. Mode resolution still synchronously clears its own timer.
Shared subscription capacity refusal reconnects through the native backoff;
mode-completion intent survives refusal and retirement until a fresh handoff
transfers its spot refresh to the native read owner. Stale or duplicate wakeups
cannot consume or repeat that work. Final book unsubscriptions retain native
l2Book-before-bbo order and the five ordered spot depth variants.
Refreshes with no private frames (not open, unresolved mode or no private user)
retain their native behavior instead of waiting for nonexistent convergence.

Book/candle refcounts, spot depth variants, account snapshot/revision guards,
HTTP request and signer formats, history and cached lookup/candle results retain
their native semantics. A stale handoff cannot start a refresh on a retired feed.
Bitunix retains the separately tracked legacy boundary under Sam's decision.

## Phemex unsigned public-trades request custody

Phemex fetchPublicTrades uses a managed public Read when both constructed
credential containers are absent. Its original 20 s command deadline includes
route preparation and the existing rate-limit pause. A pause wake after that
deadline fails typed Timeout without sending; it releases the native active
history slot. Host setup and network failures retain their typed kind and
venue metadata without credential-auth evidence. Initial Read admission has
outcome None; pre-release route refusal retains the shared NotSent outcome,
while released network failures retain outcome None.

The one-response snapshot keeps the native URL, unsigned headers, tuple codec,
occurrence-based duplicate identities, stable descending sort and output limit
(default100, clamp1..200). More than1000 tuples or any malformed row rejects the
whole page; no accepted prefix or incomplete marker is published. Direct and
streamed responses validate exact delivery custody before staging, completion
or the common429 pause update. Expired/duplicate input cannot finish a reused
command or pause its replacement. Rare history does not mutate live account data.

Credentialed public trades retain their existing signed route and wire contract,
including public_data_only and secondary-only configurations. Private reads,
identity, income, trading, signing and socket behavior remain on their current
paths. Those consumers still require the inventoried shared legacy funnel.

## Selected managed signed Read helper

The engine offers one selected-identity Read composition for genuine command
entry and Background reads. Selection, opaque identity, slot, egress and account
evidence stay coupled across the original Context generation and any permitted
physical retry; method does not choose credentials. Timeout, pacing and retry
remain explicit native declarations. Entry rejection is already terminal; queue
or retry refusal preserves its exact error and original pending/wrapper. Missing
command credentials use terminal-only local Read admission, with optional native
diagnostic before the sole typed failure and no I/O. Caller codecs still own
publication, completion, native error tables and body interpretation.

This additive engine seam does not migrate BingX/WEEX issuers in this PR; their
original native capacity assertions remain RED on the separately held consumer
branch. It introduces no new NX/Agent caller timeout or cancellation behavior.
Pre-issued external Contexts, Plan/Slot admission, writes and WS remain outside
this API. G2 owns the separate trusted subject/config-generation retirement gap.

## OKX dispatch signer adoption (draft; CI evidence pending)

This proposed family change covers `okx`, `okxeu`, `okxdemo` and `okxeudemo`.
Native HTTP and private/business WS stage canonical work through the shared
engine signer. Trusted setup consumes primary/secondary credentials before
venue construction; generic options carry opaque identities, not secrets or
passphrases. Native ordinary private GET uses secondary when configured;
ordered writes, client-ID lookup and both WS login channels use primary.
The primary passphrase is required for private accounts. An absent secondary
passphrase inherits the primary provider ref; explicit empty is rejected.

The #1714/#1728 worker/bootstrap requirements apply: provision the provider in
the engine-owning realm, including rotation/reload/disposal, rather than copying
sanitized `exchange.options`. Public/private readiness must use construction
metadata. No new persisted account-ID mapping is introduced. Live/demo/EEA roots
and proxy/CORS requirements remain as documented in the OKX integration guide.
Typed signer refusal before the first write is `notSent`; earlier accepted
bracket effects retain the existing aggregate outcome. Provider refusal cannot
mean lookup absence or an empty completed financial snapshot.

The draft now consumes actual #1757 and #1763. Browser `signHttpRequest` and
`signWsAuth` use the shared owned provider without constructing a TS OKX venue;
only the standalone request serializer survives deletion of TS venue classes.
Keyless `remoteSigner` is declared for all four aliases. Its existing external
wire is unchanged: each alias keeps its own label, Base64 MACs and WS
`extraData.timestamp` as integer seconds derived from prepared work. The
primary-only external wire cannot select a secondary identity: a native read
requiring that identity refuses before callback/I/O, never silently uses primary.
Hosted capture itself signs with primary even when secondary is configured.

Private `PayloadSigner` remains typed `unsupported` / `notSent` at setup,
including when a local secret is also supplied. The TS OKX interceptor never
invokes that callback, so this does not remove a working combination. RemoteSigner
wins when both callbacks are supplied, with no local or payload fallback.
Passphrase material is separately provisioned into trusted Present-only provider
custody; neither generic options, the external wire nor callback receiver retains
it. Browser/worker providers must be provisioned in their executing realm.

Native and SDK controls are passing; actual-WASM hosted/native-login tests and
full CI/replay proof are pending. This section describes the proposed contract,
not a landed or live-capture claim. [Family plan](RUST_OKX_DISPATCH_SIGNER.md),
[landed shared presentation](RUST_HOSTED_SIGNER_PRESENTATION.md).

### OKX caller adoption: leverage, config read and trade entry (slice 1)

This covers all four OKX aliases. [Plan](RUST_OKX_CALLER_ADOPTION.md).

- **`fetchLeverage`** is admitted as one command Read before any refusal. It
  then sends one managed signed GET using the read key when configured, else
  the primary key. An unanswered read fails once with kind `timeout` at the
  shared 20 s deadline. A late reply publishes no leverage and adds no second
  terminal. More than 256 outstanding managed reads give one typed capacity
  failure before any HTTP; an answered read frees capacity. Provider refusal
  stays one `notSent` failure.
- **The startup config read** is a Background managed Read with no retry. An
  unanswered read ends at the shared 20 s deadline and enters OKX's existing
  startup backoff, so it is re-sent one jittered backoff step later. It no
  longer stays in flight forever. 50113 counting, documented key rejections
  and the 30 s configured poll are unchanged.
- **Refusals are unchanged.** `unknown market`, `OKX leverage requires
  credentials`, `Only cross mode is supported`, `OKX account is not ready` and
  planning `invalidInput` refusals keep their text, kind and outcome. The
  trade refusals stay `notSent` through a terminal-only Mutation owner and
  leave no order lock behind. A duplicate active command ID is refused by the
  engine before OKX sees it, so OKX's own "command is already running" text is
  gone.
- Both reads now emit `ManagedHttp` preparation, as Binance, BingX and WEEX
  already do. The demo `x-simulated-trading: 1` header comes from the
  recipe's unsigned-header declaration. Writes, history, candles and socket
  login are unchanged.

### OKX caller adoption: fetchCandles (slice 2)

This covers all four OKX aliases. [Plan](RUST_OKX_CALLER_CANDLES.md).

- **Bounded by one deadline.** Candle reads stay public and unsigned, with
  the same URLs, limits, history fallback and row shape. They are now managed
  public reads. The recent page and the history fallback share one 20 s
  budget per command, so the fallback gets only what is left of it.
- **Soft-empty on any post-send failure.** A venue error, HTTP error, bad row,
  transport failure or timeout still gives one error event and then an empty
  `[]` result. An unanswered read ends with the error event `HTTP request
  timed out`. A late reply after the result is ignored. If the history
  fallback cannot be admitted (capacity), the soft result applies, because
  the recent page was already sent. More than 256 outstanding managed reads
  are refused with one typed capacity failure before any HTTP.
- **Pre-send deadline is hard.** If the host never finishes route
  preparation, nothing is sent and the deadline gives one typed failure
  (`HTTP request timed out`, Timeout, NotSent) with no `[]` result.
- **Refusals are unchanged.** Unknown market still answers `[]`. Invalid
  bounds and unsupported bars keep their text, with no kind or outcome. Live
  candle subscriptions are unchanged.

## Public derivations through the signer (P4)

The engine signer gains two non-secret operations, `PublicAddress` and
`PublicKey` (`RUST_SIGNER_PREREQUISITES.md`, "Public derivations (P4)").
No venue uses them yet; the Hyperliquid, TxFlow, GRVT and Polymarket
conversions adopt them. Caller-visible changes:

- **`IdentitySetupPolicy.signing_algorithm`.** A new optional field declares a
  slot's signing ref an asymmetric key (`ed25519`, `es256`, `secp256k1Digest`
  or `starkDigest`). Rust literal constructors must set it, or use
  `..Default::default()`. The TS `IdentitySetupPolicy` type and
  `construction-rules.json` gain the optional `signingAlgorithm` key. A MAC
  algorithm is refused, and so is an aliased secondary that declares a
  different algorithm from its primary.
- **Only a declared ref answers.** A provider answers a public operation only
  for a ref that setup policy declared for exactly that algorithm. An HMAC,
  undeclared, credential or passphrase ref is refused, and an extra-role ref
  answers only through its role's algorithm (P6b), so no secret is
  fingerprinted. `LocalSigner::insert` drops a replaced ordinary ref's
  declaration. A role ref keeps its role, which is its declaration.
- **`Operation` gains two variants.** Exhaustive matches on
  `safe_cex_engine::signing::Operation` outside this repo must add them.
  Injected providers may refuse them (`InvalidRequest`/`Unsupported`); result
  widths are exact (20-byte address, 32-byte Ed25519 key or Stark felt).
- **`safe_cex_venues::fetch_public`.** Trusted setup asks a local provider
  inline for public values: at most 4 public-only operations, validated first.
  A host or remote provider gets `Unavailable` and nothing is scheduled. Every
  answer is checked as a canonical curve value (Stark x, Ed25519 point), and a malformed one
  is `InvalidResult`. Those accounts
  supply the value as bound account metadata instead.
- **`safe_cex_crypto::ethereum::signed_by`.** A shared pre-release check that
  a low-s host/remote signature recovers to the bound actual signer. Families call
  it before release; it never compares with expected venue config.

## Explicit request egress (#1833) and Plan/preflight issuance

#1833 captures explicit egress (`Primary`, `PublicCors`, `Direct`) when a
context is issued through `issue_with_egress` / `Cx::http_context_with_egress`.
Legs, replacements and retries inherit it. Plan and preflight contexts from
#1873 are minted directly with `Context::new`, so they have no explicit-egress
admission. Without one, they derive routing from the credential: no credential
routes Public, the primary slot Primary, and the secondary slot Secondary.
Callers must not treat #1873 together with #1833 as Plan-egress support, for
example for WOO or TxFlow entry. The additive seam (explicit egress on
Plan/preflight issuance) is owned by claude-m26ps4, plan first after #1833
lands, with codex-uufub4 reviewing API fit.

## Caller command wait in the engine (#1945)

The WASM host now adds the effective caller wait to every reply command's
native envelope as `timeoutMs`. The effective wait is the per-call
`options.timeoutMs` if given, else the configured `rustCommandTimeoutMs`, and
it is the same validated number that arms the host's own timer. No-reply
controls send no field.

- **Receipt stamp.** The engine stamps one absolute deadline per external
  receipt. Native hosts and envelopes without the field get the SDK default of
  120000 ms, stamped at receipt. Held and deferred commands keep the receipt
  they arrived with.
- **Refusals.** A present but invalid `timeoutMs` (null, non-integer, zero,
  negative or over 2147483647) is refused at decode. A non-finite clock is
  refused by `handle_command`, with a `JsError` and no dispatch.
- **Venue behaviour.** Nothing changes until a venue adopts
  `Cx::command_wait`.

Two separate events, which callers must not conflate:

- **Caller abandonment.** The host timer rejects the promise with
  `COMMAND_TIMEOUT`. The engine keeps any in-flight transport, and a late
  native reply is dropped by the host.
- **Engine-root expiry.** An adopting venue uses the `Remaining` wait as its
  original Read root and stops issuing work at that same absolute deadline. A
  Fresh entry that is already expired ends through
  `reject_expired_command`: one typed Timeout terminal with zero HTTP, timer or
  Store effects, `NotSent` for a mutation and no outcome for a Read. After
  admission, expiry ends through the existing Context or Plan authority and
  never claims `NotSent`.

Raising a caller's timeout lengthens an adopting venue's root but never
renews it. Plan consumers are not capped by the caller wait until the
separate receipt-cap seam (#1949) lands.

## Opt-in Read Plans retain the caller receipt deadline

`begin_preflight_with_command_wait` opts a Read command into the original caller
receipt cap before JWT/preflight work. The eventual Plan ends no later than
that absolute deadline, and still ends earlier when its declared native work
has a shorter bound. Native page, retry, fallback and per-call geometry are
unchanged. Later callbacks, stages and retries cannot renew the receipt cap.
Existing native-only preflight/Plan callers keep their current lifetime.

An absent host timeout receives the SDK default once at external receipt. A
missing or failed receipt never manufactures another default: genuine Fresh
admission uses one typed engine Read refusal with no financial outcome. Borrowed
or spent custody emits nothing. An already-expired own receipt leaves Fresh
custody for `reject_expired_command(Read)`, the sole initial Timeout emitter.
Expiry at Plan admission leaves valid preflight failure authority and returns
TimedOut; adapters use the canonical Read error projection when settling it.

Host listener abandonment and engine Plan expiry remain distinct. Raising the
configured/per-call wait can lengthen the caller cap, but cannot enlarge a
shorter native declaration. No transport cancellation, original Read-root cast
or accepted-prefix recovery permission follows from observing this cap.
Mutation and execution-owned capped Plans are outside this first slice; native
mutation/provider errors and their outcome projections remain unchanged.

## BingX FetchAccount selected Read custody

FetchAccount admits its original Read before native eligibility diagnostics or
signed HTTP. Shared capacity/invalid-clock denial emits one typed failure without
a native diagnostic or financial outcome. Eligible requests retain the selected
opaque identity and original 20s/Never Context through managed preparation and
release; Secondary serves reads only with an eligible Primary, and a saved
invalid Secondary keeps its native refusal even in public-only configurations.
The exact GET/query/header bytes and userId codec are preserved. Rare identity
reads leave live account state unchanged. Network/Timeout metadata now survives
with the native correlated diagnostic before the single terminal. No history,
account-poll, trading or socket caller is migrated by this slice.


## Shared two-run CPU certification

Every registered replay suite uses the same CPU certification policy. A complete
public first measurement with only CPU misses below 5% of its existing limits
requests one separate physical runner. All three CPU objectives must pass the
arithmetic mean of the two runner-relative normalized ratios. The policy never
selects the better run or averages memory, correctness or size failures.
A strict first pass and an existing declared diagnostic result keep their
ordinary manifest verdict and request no second measurement. Original strict
reports, diagnostic lists and raw outputs remain unchanged, including FAILs.

The existing `Rust / replay gate (<suite>)` checks certify the decision after
initial `Rust / replay measurement (<suite>)` jobs. The global planner, shared certifier and final
checks run even after upstream failure. Each final check requires the successful
same-run, same-checkout complete output of that shared certifier. The second-run matrix is skipped when
there is no eligible miss; otherwise it contains exactly the selected suites.
Consumers of check-rollup metadata must allow that conditional job to be
skipped, while requiring every final replay check to succeed. Missing, failed,
retried or ambiguous second evidence fails certification; there is no third
measurement. A workflow retry cannot add another candidate to the same cohort.
A separately requested fresh workflow owns independent evidence.

Both measurements use the original sealed source, Node, dependencies, tape,
compiled JS/WASM and initial loader cache. Runtime or canonical output drift
fails certification. Private replay workloads and existing diagnostic handling
keep their strict policy. Build arguments, CPU thresholds and fixtures retain
their existing contracts. [Policy and controls](RUST_BITGET_CPU_OPTION_B_FOLLOWUP.md).

An immutable artifact or completed-log download can recover one TypeError caused
by UND_ERR_SOCKET. Immutable artifact downloads can also recover one empty HTTP
200; both causes share exactly two retrieval attempts. Metadata/log empties,
HTTP refusals, oversized responses and invalid evidence still fail closed.
Only registered runtime archives use the existing 4 GiB ZIP response bound;
metadata, logs and ordinary archives retain the 512 MiB response bound. Exact
size, digest, producer, source, ZIP and restored-runtime checks remain required.
This retrieval recovery adds no physical measurement and cannot repeat a failed
second run.

## Engine-owned expiry of caller-wait Read roots (#1945 follow-up)

A venue that admits its original Read root with `Cx::admit_caller_read_root`
gets a root bounded by the caller's exact receipt deadline. The engine ends
that root itself at the deadline:

- **Terminal.** The engine emits exactly one strict `CommandFailed` with
  kind Timeout and no financial outcome.
- **Nothing published.** Accepted history rows are not published, and no
  INCOMPLETE marker or native finalizer runs, even when earlier pages were
  accepted.
- **Venue.** Outstanding requests are cancelled. Their continuations reach
  `Venue::read_root_retired`, which runs with no `Cx` and only drops queued
  work.
- **Inert afterwards.** Late replies, binds and venue completions through the
  expired root are inert.

A page timeout while the root is still live keeps the venue's native
accepted-only partial recovery.

Missing receipts. A genuine command with no receipt of its own (an exhausted
receipt sequence, a duplicate of a live legacy id, or an unstampable clock)
is refused once:
- a Read gets no outcome;
- a Mutation gets `NotSent`.

No default deadline is ever minted. Callers see a typed failure, not a
120 s wait.

Adopting venues must complete only through their root and must never use
numeric `done` or `fail`. A numeric completion after the owner is removed is
a separate gap that predates this change and is not closed here. Callers can
distinguish the following:

| Event | Meaning |
| --- | --- |
| Host `COMMAND_TIMEOUT` | Caller abandonment. Engine work may continue. |
| Engine Timeout terminal | The root expired. No further work is issued. |

## Selected caller-wait roots and physical history pages

Signed plain command reads can retain one selected identity, slot, route and
account-evidence scope across physical pages. The original root captures the
caller's exact receipt deadline. Each eligible page or native re-send gets a
fresh Read/Never child capped by that root; a preceding page's shorter deadline
does not bound the next page. Selection never changes midway through a query.

Root expiry uses the shared strict Timeout terminal without accepted-row
publication or an incomplete marker. A physical page timeout while the root is
live retains the shared history caller's accepted-prefix recovery. Queued native
state retires by exact owner through the shared expiry hook. Consumers complete
only through their original root and keep their existing retry eligibility,
paging, body validation and finalization policy. This helper changes no host
wait, numeric completion contract or native venue issuer by itself.

## TxFlow whole managed socket family

TxFlow uses shared managed feeds for both api-ws and node ws. API is Public;
node is Private unless public_data_only. Shared/Disabled public-feed policy
suppresses public sockets/topics while retaining private node financial traffic.
Both feeds retain receive-before-decode AnyText liveness, native JSON/ping bytes,
10s connect,30s idle,10s ping and1s-to16s retry bounds. Duplicate/retired/expired
socket ingress is inert before native publication. Only node retirement clears
published books and invalidates the account generation/settings/read owners.

Subscriptions represent the final desired set, reconciled against frames already
returned to the host, with150ms pacing. Transient subscribe/unsubscribe flips may
coalesce before release. They do not create an unbounded FIFO or a4096-flip close;
actual retained-capacity refusal remains bounded by the shared core. Unsubscribe
after an already-returned subscribe still emits the native unsubscribe frame at
the existing cadence. The final set converges after a burst of flips.

Private opening/reconnect refresh follows the complete desired node batch and a
subsequent shared released-interest timer ingress. This deliberately replaces
native HTTP-before-paced-private-frames ordering; it is host-action handoff, not
peer acknowledgement. The callback selects current native read/revision state.
Refresh intent survives refusal/retirement and is consumed only by native read
ownership. Startup and poll reads remain independent; startup consumption prevents
a later wake from adding a refresh the native early-open path never admitted.
Public-only mode has no private refresh. A missing private address still fails
configuration before I/O; the defensive no-address callback remains a guarded
native no-op. HTTP builders, PublicCors/static headers, financial reducers,
history, signing and trading retain their existing contracts.


### Replay protocol diagnostics at final capture

Replay checkpoints capture comparison state without finalizing protocol
coverage. Both TS and Rust replay receivers aggregate protocol errors once at
the logical end of the replay, before correctness-mode memory collection. A
checkpoint taken before an authentication acknowledgement or the first
heartbeat therefore cannot permanently introduce an incomplete-lifecycle
diagnostic into an otherwise complete final result.

Actual event violations remain recorded and appear in the final report. Missing
final authentication, heartbeat or required subscriptions still fail; an audit
exception still aborts replay. This changes harness reporting, not native order,
fill, transport or signing behavior. The tape and declared-failure policy retain
their existing meaning; a clean intermediate capture does not certify private
parity.

## Hyperliquid perps history caller-root custody

Perps fills, orders, income, public trades and uncached REST client-ID lookup now
use one Read root at the exact caller receipt deadline (configured/per-call
rustCommandTimeoutMs; absent input is stamped120000ms at engine receipt). Each physical
page gets a20s child at eligible dequeue in the native shared Account FIFO,
capped by that unchanged root. Missing receipts refuse typed with no default,
I/O or outcome. These unsigned JSON POSTs retain the same user/vault/application
subject, Content-Type and transport-preparation obligation. Public Auth* failures
retain typed metadata without AccountAuth evidence.

At root expiry the engine emits one strict Timeout with no financial outcome:
accepted rows, INCOMPLETE markers and native finalizers are discarded; late data
and old queued continuations are inert. A failed page while the root remains
live keeps native accepted-only recovery, or strict failure if no eligible rows
were accepted. fetchHistoricalPositions / PositionHistory::FillsSymbol therefore
reject at root expiry instead of folding a prefix. Lookup keeps native unknownOid
and symbol-mismatch non-definitive results and untyped codec errors.

Native geometry is unchanged:200 pages per fills/funding stage, up to400 for
combined income, inclusive timestamp/ID progress, retention notices, sorting and
limits. Streamed pages reserve120 weight then settle20+ceil(rows/20) in the shared
1000/min budget (full fills120, full funding45); no whole-walk duration is promised.
Queued children do not spend their20s before dequeue, but the caller root keeps
running. Existing cached/invalid/anonymous local lookup and standalone spot
completion behavior remain unchanged. Uncached public-only identity refusals now
obey caller-root capacity before their native message; admitted local refusals
immediately clear their root timer and publish exactly one terminal.

## Bitget native dispatch signer checkpoint (draft; hosted prerequisite pending)

This proposed change covers `bitget`, `bitgetusdc` and `bitgetinverse` native
execution through the existing engine signer/provider. Production venue options
contain only opaque primary/secondary identities. Trusted #1738 setup consumes
material once: private primary passphrase required, missing secondary passphrase
inherits the primary reference, and explicit empty rejects before startup.
Ordinary private GETs (identity, metadata, resolve-amend and history included)
select secondary when configured; degraded balance recovery retains its existing
primary identity/egress exception. Writes and Classic/UTA private WS use primary.
Public requests stay unsigned; product, inverse sizing and UTA admission remain.

#1714/#1728 provider bootstrap requirements extend to these aliases: provision
and rotate/dispose the provider in the engine-owning worker/host, rather than
copying sanitized exchange options. Use construction/readiness metadata instead
of `options.key`. No new persisted account-ID mapping is introduced. Missing
passphrases now fail trusted setup synchronously rather than a later startup
callback. Native provider refusal does not prove Classic account type or native
auth rejection. Strict identity/history reads cannot report successful absence;
a wholly unsent write remains notSent, while existing accepted/resent/rollback
financial evidence is preserved.

Hosted and keyless admission now builds on the landed shared presentation
extension; see the next subsection. Passphrase custody stays with the trusted
provider; no SDK receiver secret, new required external wire field or
venue-local crypto fallback is authorized.
[Family plan and proof gates](RUST_BITGET_DISPATCH_SIGNER.md).

### Bitget hosted and keyless remote signing (draft; actual-WASM CI pending)

Browser `signHttpRequest`/`signWsAuth` for `bitget`, `bitgetusdc` and
`bitgetinverse` use the shared owned provider without constructing a TS Bitget
venue. Hosted HTTP formatting lives in the standalone `bitget/bitget.signing.ts`
codec, which the TS interceptor also uses, so it survives deletion of the TS
venue classes. Hosted capture signs with primary custody even when a secondary
is configured; without a complete primary the request returns unsigned, as the
legacy interceptor did. MACs are Base64; the provider presents
`ACCESS-PASSPHRASE` and the codec sets `ACCESS-KEY`, `ACCESS-SIGN`,
`ACCESS-TIMESTAMP`, `ACCESS-PASSPHRASE` over the request's broker headers.

Keyless `remoteSigner` is declared for all three aliases on the existing
external wire: WS login requests keep the hard-coded `exchangeName: 'bitget'`
label, `outputFormat: 'base64'` and integer-seconds `extraData.timestamp`
derived from prepared work; signed HTTP preimages keep the alias label. Native
login JSON keeps its string timestamp. The primary-only wire refuses work
needing the secondary identity before callback/I/O, never silently using
primary. Private `PayloadSigner` stays typed `unsupported`/`notSent`: the TS
Bitget interceptor never invokes it. RemoteSigner wins with no local or payload
fallback.

The WS-seconds codec is now construction-declared and shared with OKX:
`wire.extraData: "secondsTimestamp"` requires `wire.secondsSuffix`
(`{method, path}`; OKX `GET` + `/users/self/verify`, Bitget `GET` +
`/user/verify`). Native login work is three typed byte parts (seconds, method,
path) from the engine `seconds_suffix_payload` helper; the remote bridge decodes
it only against its own family's declaration. OKX keeps its exact concatenation,
MAC, receipt and external wire. Per-alias actual-WASM hosted and native-login
tests run in CI only; no live capture is claimed.

## OKX history caller-root custody

All four OKX aliases admit history, income, public trades and client-ID lookup
through a Read root at the exact caller receipt deadline. The absent host wait
is stamped120000ms at engine receipt; configured/per-call waits keep their raw
absolute deadline, and a missing receipt gets one typed Read refusal with no
outcome or fallback. Native local[]/lookup validation still follows admission.

Each physical page has a20s/Never child minted at its actual native send/dequeue
boundary, capped by that root. Native100ms orders/income pacing, immediate fills
archive fallback and EEA product transitions stay unchanged. Ordinary signed
reads retain Secondary else Primary, client-ID lookup Primary, and public trades
stay unsigned. Demo headers, exact paths/query bytes and dispatch recipes are
unchanged. Hosts must perform managed route preparation and final release;
managed Primary intent uses the host's default primary egress (physical routeNone).

Whole-root expiry is strict: one Timeout with no financial outcome, rows,
INCOMPLETE marker or soft[]; parked continuations and late DATA cannot resume or
finish a reused command. Ordinary released page transport/HTTP/parse failure or
a child-only timeout while the root is live still emits the native command_error
then soft[] for queries, discarding every prior page. Lookup rejects instead of
claiming absence. Provider/preparation refusals before release are strict typed NotSent failures.
Synchronous child/submission refusals are also strict, with the shared Read
projection (outcome None). Native
lookup codec errors retain their untyped kind. Serialization errors finalized
through ReadCompletion use the shared Internal classification.

fetchHistoricalFills, fetchHistoricalOrders, fetchIncomeHistory and its range
variant, fetchPublicTrades and findOrderByClientOrderId use this bound. Native
PAGE100, orders cap50, income cap200 per product, EEA SWAP/FUTURES composition,
decimal ID cursors, sorting, dedup, fee/PnL decomposition and contract conversion
are unchanged. Plain page-cap/stall results retain their own incomplete notice;
EEA composite cap/stall still rejects. Only successful native51603 is definitive
lookup absence. FetchClosedPositions/okxJournal remains its separate unchanged
source. Discovery still uses the retained legacy list/sign funnel.

## Crypto.com candle and local-history read ownership

fetchCandles uses unsigned Public managed HTTP with one physical attempt and a
20-second engine deadline spanning preparation and response. Custom transports
must drive prepare, bind and the final engine claim before sending; this candle
path supplies no legacy-request fallback. Legacy unsigned
HTTP relied on the host timeout after dispatch; preparation now consumes the
same immutable command bound. Expiry/transport failures preserve typed Timeout/
Network with public-read NotSent. Initial invalid-time/capacity refusals retain
typed InvalidInput/Internal and no outcome. Native URL/timeframe mapping, raw
bounds and candle timestamps/order, ignored limit and numeric-string parsing
remain unchanged. Native HTTP/envelope/row/missing-list errors retain full string
error fields and auth-health isolation; no public401/403/40101 auth retry is added.
Undecodable text remains the native stream-list string at200 and HTTP string at500.

Only five synchronous history terminals borrow shared entry and finalize a
Context. Finite native errors/[] and validation order stay exact; invalid engine
time or owner-capacity exhaustion gives one typed engine refusal instead.
fetchHistoricalFills/fetchHistoricalOrders, fetchIncomeHistory/fetchIncomeHistoryRange,
fetchPublicTrades and findOrderByClientOrderId consumers retain all successful
native history signing/pagination/coverage paths. Explicitly bounded income
with caller limit0 still dispatches native HTTP. This is not managed network
history or signer adoption.

### Crypto.com current shared failure contract

Managed delivered text without the expected list at HTTP200 retains the native
message and now carries the shared engine Internal kind, with the established
public-read NotSent outcome. HTTP500 retains its native HTTP-error string.
This follows the existing typed undecodable-response seam; the adapter does not
demote the kind to mimic legacy string-only callbacks. The20s Context lifetime
is explicitly a new admission-through-preparation/response whole-command bound,
distinct from the old host's physical HTTP timeout fallback. Raw candle t/order
and malformed numeric-string-to-zero coercion remain unchanged.

### Bitget caller adoption: candles, identity, account reads and trade entry (slice 1)

This covers `bitget`, `bitgetusdc` and `bitgetinverse`.
[Plan](RUST_BITGET_CALLER_ADOPTION.md).

- **`fetchCandles`** is admitted as one command Read before the interval
  check. It sends one unsigned managed GET with only `X-CHANNEL-API-CODE:
  Tealstreet`, prepared with the Public routing intent. The URL, limits and
  rows are unchanged. Unsupported intervals and post-send failures keep their
  text with kind and outcome absent and no diagnostic.
- **`fetchAccount`** is admitted as one command Read. Public-only and invalid
  read-key refusals happen before any HTTP with the same text and diagnostic.
  It sends one managed signed GET with the read key when configured, else the
  primary key. Failure texts and the equal diagnostic are unchanged.
- An unanswered candle or identity read fails once with kind `timeout` at the
  shared 20 s deadline; a late reply adds nothing. More than 256 outstanding
  managed reads, Background account reads included, give one typed capacity
  failure before any HTTP.
- **Account reads** (settings probe, positions, balance and order pages) are
  Background managed Reads with no retry. The degraded balance read keeps the
  primary key and prepares with the Primary routing intent. An unanswered
  probe ends at 20 s and takes the existing transport route into Classic
  detection, unless UTA is confirmed. A probe that was never released
  (signing or host preparation did not finish) backs off instead and is not
  Classic evidence. An unanswered balance, position or order
  read ends at 20 s and the next poll reads again. It no longer latches the
  poll forever. Classic/UTA routes, backoff, 40009 and rate-limit pauses are
  unchanged.
- **Trade entry refusals** keep their text, no kind and `notSent` through a
  terminal-only Mutation owner, with no lock or queued job left behind.
- History, writes, discovery and the private socket are unchanged.

### OKX account-cycle reads under Background Context

This covers `okx`, `okxeu`, `okxdemo` and `okxeudemo`. [Plan](RUST_OKX_ACCOUNT_CYCLE.md).

- The balance (`account-position-risk`), positions (each product) and open-order
  pages (`orders-pending` and `orders-algo-pending`, each product, kind and cursor)
  are now one managed Background read per page. Each page signs with the read key
  when configured, otherwise the primary key.
- An unanswered page ends at a 20 s deadline. The account cycle then reads again
  on its native retry instead of staying stuck. A long order walk is never cut
  short, because every page has its own 20 s.
- URLs, the demo header, row decoding, the balance and positions join, order
  paging, product order and rate-limit backoff are unchanged.

### OKX discovery under Background Context

The instruments catalogue and ticker seed walks (SWAP then FUTURES) are now one
managed Background read per product: 20 s, no retry. Public reads stay unsigned
and keep the demo header. The EEA account catalogue is signed with the read key
when one is configured, otherwise the primary key. An unanswered read ends at
20 s and startup discovery retries on its native backoff. URLs, row decoding,
partial-catalogue rules, the seeded ticker count and the 429 pause are unchanged.

## Lighter synchronous history Read custody

The seven native local history replies (unsupported client ID/income category,
invalid bounds/native index/market, empty keyless history and unavailable private
identity) retain native precedence, JSON and messages. They now borrow genuine
shared Read command admission before publishing, so invalid clocks, full owner
capacity and reused ownership produce one engine refusal without a financial
outcome. Both Lighter and Robinhood, on mainnet/testnet, keep the same contract.
Remote history requests, pagination, authentication and byte-level query/codec
behavior are unchanged; this slice issues no remote-fallthrough Context.

## GRVT synchronous history Read custody

Seven native local history refusals retain their original validation order and
error messages: unsupported income, missing session, unknown market, invalid
bounds, range/empty order ID, client-ID syntax and missing subaccount. Publication
now requires a genuine shared Read root. Clock/capacity rejection produces one
typed terminal without a financial outcome. Revoked entry and live duplicate IDs
preserve the existing owner without a second terminal. Successful local admission
keeps the original untyped native error. Both networks preserve the same behavior.
Remote history queries, session setup, headers, paging, completion and codecs
remain unchanged. This slice issues no Context on remote fallthrough.

## KuCoin synchronous command refusal admission

Eight local futures/Spot refusal sites now acquire their genuine shared command
owner before publishing the native error. Capacity or invalid-entry refusal can
therefore supersede that error; duplicates cannot replace or complete another
owner. Read admission failures carry no write outcome, while Mutation admission
failures are NotSent. Free-capacity native error text, kind, outcome and validation
order remain unchanged, including Unsupported/NotSent on Spot's unsupported reads.

No local refusal sends HTTP or uses the signing provider. Spot placement building
still consumes native UUID entropy before some credential/build errors; supplied
IDs and later batch errors keep their original order. Valid private/public wire
paths stay legacy, and KCS steady public events gain no admission work. Successful
keyless fixed settings remain held for the shared local-setting success seam.

## Bitrue initial local trading refusal admission

Three synchronous trading refusals now acquire their genuine shared Mutation
owner before returning the existing error. Shared entry refusal can supersede
the native error, producing one NotSent terminal; duplicate callbacks retain the
original owner. Free-capacity messages, kinds and credential/public-only ordering
are unchanged. Refusal creates no HTTP, signing-provider work or Store effect.

The queued trade_next error, native builder/entropy behavior, valid private wire
and successful SetPositionMode(false) stay unchanged. This does not migrate the
signer or admit keyless successful settings; that success awaits the shared seam.


## Polymarket public reads and local completion (2026-10-24)

Owner codex-hcmq3f; plan RUST_POLYMARKET_PUBLIC_READ_CONTEXT.md. Public `/book`
and `/prices-history` now use unsigned managed transport, requiring host
Prepare/Bind/Claim. Their explicit new attempt bound is 20000 ms from selection,
before preparation; command legs are additionally capped by the original caller
receipt through landed #1976. Missing/expired receipts use shared typed refusal,
without a new fallback cap. No retry or pacing is added. Background candle polls
keep the native five-second schedule and require exact current delivery before
staging or publishing. Released read failures keep typed kind without a money
outcome; genuine preparation refusal retains its shared local NotSent fact.

Local read results/refusals and unsupported STP/symbol-amend mutation refusals
use genuine command entry and Context completion, preserving native JSON/text.
Keyless mutation refusals now explicitly prove NotSent. Fixed-setting no-op
SUCCESS is still legacy and remains inventoried pending #1989; no fake write
fact, credential or Read cast is used. The exact prune is 15 fingerprints /
20 occurrences. Discovery aggregates (G1), private account/import (P6b), private
writes/admission/P8 and their legacy inventory remain open.

## Kraken public discovery and local custody (2026-10-24)

Owner codex-hcmq3f; RUST_KRAKEN_PUBLIC_LOCAL_CONTEXT.md. Futures instruments and
tickers plus Spot AssetPairs/Ticker now require managed Prepare/Bind/Claim, with
exact current Background Read delivery before row staging, body decoding or Store
publication. Spot keeps its existing 20000 ms timeout; Futures adds an explicit
20000 ms admission-to-attempt-expiry bound, including time held in preparation.
No retry is added inside an attempt. Existing native discovery backoff schedules
fresh independent attempts after exact success/failure retirement, and failure
clears the same attempt's busy state. Stale replies cannot publish or affect a
new attempt. Background work emits no numeric command terminal or auth evidence.

Selected local Read results/refusals and keyless Mutation refusals use genuine
command entry and Context completion with native JSON/text/metadata. None or
expired caller receipts use the cause-aware shared refusal. Fixed-setting
SetPositionMode(false) SUCCESS stays legacy and inventoried pending #1989.
Valid signed account/trading/history, separate Spot/Futures nonce domains,
private sessions, paged Futures candle reads and existing Spot OHLC remain open
or unchanged. This prune is 17 fingerprints / 19 occurrences; no new signing or
whole-family custody claim is made.

## GRVT replay protocol coverage at final capture

GRVT replay reports evaluate private-stream coverage at the logical end of the
lifecycle. An interim observation before authentication acknowledgement does not
leave missing-stream diagnostics after the valid subscriptions complete. Genuine
unsubscribed private data, wrong subaccount selectors and missing coverage on an
active replacement generation remain final failures. Execution identity and
order/caller-identity differences remain strict account diagnostics.

### Crypto.com replay protocol declaration accuracy

Crypto.com's current private replay no longer declares missing subscription
errors introduced by observing the audit before the authentication ACK. TS
requires its four private channels; Rust additionally requires `user.trade`.
Invalid authentication, subscriptions sent before the ACK and unanswered final
heartbeats still appear as protocol failures. The remaining execution, hedge
mode and caller-identity differences retain strict FAIL; this reporting change
establishes no native financial parity.

### Binance Spot replay acknowledgement diagnostics

Replay checkpoints do not finalize Binance Spot's acknowledgement coverage.
A signed subscription can therefore be observed before its ACK without creating
false final missing-subscription or missing-request errors after the actual
lifecycle completes. Genuine signature, typed-ID correlation and pre-ACK event
violations remain failures, as do incomplete live replacement generations and
unanswered final requests. The remaining fill-price, time-in-force, client-ID
and order-lifecycle differences retain strict FAIL; financial parity is not
established by protocol reporting.

## MEXC local write refusal custody

Low-level write commands whose native step construction fails retain the same
CommandError diagnostic followed by the same NotSent failure. Validation still
precedes missing-key handling. A valid step result without credentials retains
'MEXC trading needs an API key', AuthInvalid/NotSent, with no diagnostic. These
two local branches now borrow genuine Mutation entry and settle its Context.
Successful native queue/sign/async paths remain unchanged.

Invalid clock or full owner capacity produces one canonical typed admission
refusal (InvalidInput/Internal, NotSent) without the local diagnostic, I/O, timer,
Store mutation or peer completion. Synchronous settlement releases its own
capacity/order-lock custody; duplicate live IDs keep the existing shared
diagnostic and owner. A local missing-key error carries no credential evidence,
so it no longer increments auth_failures through the legacy untracked-command
path. This consumes the existing shared custody rule, with no venue counter.
SDK placeOrder(s), cancellation/amend and setting callers keep existing validation
and dispatch choices; this entry describes low-level refusal behavior and adds
no new SDK calls or successful write capability.

## KuCoin Spot fixed-setting local success

SetLeverage still ignores the requested value and returns `[]` after storing
leverage/buy/sell 1 through the original Store::update_setting (including its
unknown-symbol default insertion). SetMarginType and SetPositionMode remain
successful `[]` no-ops, before credentials, including public-only/keyless setup.
The shared LocalSetting seam now refuses expired/sub-ms waits or exhausted
command-owner capacity before projection and success; there is no numeric
fallback or duplicate terminal. Duplicate live ids retain their original owner
and diagnostic. No transport, signer, timer, Context or journal is added.

## Bitrue local one-way setting completion

SetPositionMode(false) still returns `[]` and stores hedged=false before
credentials, including public-only/keyless setup and missing or unknown symbols.
The shared LocalSetting seam now refuses exhausted command-owner capacity and
expired/sub-ms waits before that projection or success. It adds no Context,
HTTP, signer, timer or journal. Duplicate live ids retain the original owner
and diagnostic; SetPositionMode(true) retains its Unsupported/NotSent refusal.

## WEEX account-cycle reads use Background authority

WEEX's recurring balance, positions, normal/algo orders and account/symbol
configuration reads now use the shared managed preparation and final claim
with one Background Read Context per physical request. Credential selection,
sorted query bytes, streamed row shapes, snapshot joins and native revision
fences are preserved. A paused continuation admits its fresh 20-second Context
only at eligible dequeue, with no transport retry or re-sign.

Provider/preparation refusal retires the physical Context and follows the
native failed-read path without auth evidence or deactivation. A physical
Timeout or transport failure retains last-good snapshots and permits a later
poll; native settings backoff still independently holds the configuration reads.
Exact Background delivery is checked before row decode and publication, and
stale cycle/seed deliveries have no snapshot or credential-auth side effects;
429 still preserves the native global pause for other reads.
No command completion, metadata group, history or trading policy is changed.
The remaining legacy shared signing helper and STAGING entry stay for their
other callers; this semantic caller conversion has zero lexical inventory delta.

### Bybit cancel by symbol-qualified refs

| Change | Command outcome | NX handling |
| --- | --- | --- |
| `cancelOrdersWithSymbols` on any Bybit variant | Typed `NotSent` `unsupported` before any request (was accepted and never finished) | Use `cancelOrders` with order ids. |

## Flat-position realized PnL evidence

Replay comparison includes flat-position realized PnL only when the producer
reports a finite value. Native unavailable NaN and TS absent/null values produce
no flat-PnL evidence row. Actual zero, losses and profits remain comparable,
including the existing numeric-string conversion; losing or changing actual PnL
still fails comparison. This corrects shared replay evidence without changing
venue account state or the native frame's unavailable-value encoding.

## Paradex public Read refusals and Lighter catalog custody

Paradex candle requests keep unsupported-interval-before-market validation and
native messages. Their two immediate refusals now require a genuine Read root.
The existing managed public book read uses shared command entry, so clock/capacity
rejection yields one typed failure with no financial outcome. Revoked entry/live
duplicates preserve the existing owner without a second terminal. Remote candle
klines/TradingView fallback, polling, book GET bytes and book schema stay native.

Lighter/Robinhood public startup catalogs now await managed Prepare/Bind/Claim
and validate their exact Background Read callback before decoding or publishing.
Their20s budget includes host preparation/claim delay; a delayed custom transport
can expire before outbound. Each native orderBookDetails/orderBooks attempt has
its own budget. Native endpoint fallback and shared startup backoff are retained,
with the old attempt retired before follow-up. No command result is introduced.
All four deployments keep their headers/body, route, catalog codec and metadata.

## WOO and Crypto.com joined order-snapshot freshness

Rust WOO and Crypto.com retain the last accepted order rows when a regular/algo
or regular/advanced REST census crosses an accepted native order mutation.
The shared Store token rejects the entire joined publication before changing
native algo classification, advanced-list membership or startup fill baselines.
Accepted ACKs invalidate older reads even when their IDs have no stored copy;
malformed/refused order responses and leverage/mode operations keep their
existing acceptance and outcome rules. Successful members of a batch retain
their fence if a later member fails. Bounded Crypto.com retries keep the original
read's token and collected regular half, so retrying after an ACK cannot renew
old authority.

Per-ID WS version/timestamp/terminal fences remain in force: newer live rows
survive while healthy snapshot peers progress, and snapshot publication does
not invalidate sibling read tokens. A discarded WOO join waits its ordinary
account cadence rather than publishing caches or issuing an immediate repair;
Crypto.com releases its reconciliation cycle and schedules its ordinary timer.
When capture for a new census is exhausted, no new order census starts; stored
rows and readiness are preserved. Existing in-flight retries can finish but cannot
publish exhausted authority. Positions, balances and normal polling continue. Public wire
payloads, signing, commands/results, IDs and frame shapes keep their contracts.
No wrapper, SDK, CLI, MCP or server caller needs a new parameter or outcome type.

## BloFin local FetchLeverage Read refusals

Unknown market still precedes margin/mode-not-loaded before credentials, with
exact native messages and absent kind/outcome fields. Each of these two local
refusals now acquires its genuine shared Read owner; capacity refusal supersedes
the native error with one typed Read failure and no write Outcome. No numeric
fallback or extra terminal occurs. Duplicate live ids preserve their diagnostic
and original owner. No Store change, HTTP, provider, timer or private credential
is introduced. Valid signed leverage queries retain their original issuance,
path, request fields, revision checks, response handling and native retries.

## Polymarket and Kraken fixed settings (2026-10-24)

Polymarket SetLeverage/SetMarginType/SetPositionMode and Kraken Futures
SetPositionMode(false) adopt actual #1989 done_local_setting. They remain
keyless Store no-ops with exact `[]` and `null` results, respectively, and no
I/O. Success is confined to genuine Setting entry and now honors its receipt;
an invalid clock produces typed InvalidInput/NotSent instead of numeric
success. No Context, credential, physical attempt or signer readiness is
created. Kraken true-mode refusal is unchanged. Inventory shrinks by two
occurrences. See RUST_FIXED_SETTING_ADOPTION.md for the native compatibility
and NEW-CONTRACT refusal controls.

## OKX reported closed-position caller-root custody

FetchClosedPositions now owns the immutable original caller receipt deadline
(default120000ms, or the supplied rustCommandTimeoutMs). Each signed page has a
20s/Never child minted only at its actual native100ms dequeue, capped by that
root. Keyless and unknown-symbol empty results require genuine Read admission;
missing/invalid receipts or full capacity can therefore produce one typed Read
refusal instead of bypassing custody. No local timeout default is fabricated.

Root expiry strictly returns one typed Timeout with no rows or INCOMPLETE, and
local child admission/submission refusal strictly rejects the root. Physical
HTTP/transport/live-child-timeout and provider/preparation failure preserve the
native policy: fail without accepted output, otherwise return only accepted
rows with owned INCOMPLETE. Shared typed failure metadata is retained on strict
failure; released Read timeouts have no money outcome. Exact-owner retirement
prevents late data and native timers from affecting a reused command ID.

Raw OKX rows, Secondary-else-Primary selection behind the primary-required gate,
wire parameters, PAGE100/MAX100, cursor/cycle/cap semantics and mapper stay
unchanged. SDK fetchClosedPnl/fetchReportedClosedPnl still catches and reports
failure then returns[]; fetchHistoricalPositions/okxJournal with report:false
still propagates it. Recovered results keep copyHistoryCompleteness. This
retires the Closed funnel consumer with0 direct lexical inventory removals;
account/discovery/writes retain their shared list/sign sites until conversion.

## Retained command readiness

With the engine's `http-plans` feature, native FIFO commands can park their own
Preflight during genuine entry and activate it once after a predecessor physically
retires. They keep their original command generation; waiting creates no request,
timer or artificial deadline. Final native Plan admission establishes its bound.
A predecessor's HTTP DATA cannot donate command-entry or response authority to
the next command. Cleanup returns only an already-parked handle; actual activation
runs after cleanup, callback exit and the execution sweep.

A ready callback must finish, submit a bounded prerequisite read or admit its Plan.
An unhandled callback or a selected read with no submitted HTTP fails once with
Internal. ReadOnly has no write outcome; a truly unissued mutation is NotSent.
NX consumes that one terminal and keeps any prior uncertain effects for ordinary
reconciliation. Ready custody cannot submit or retire another command's work.
Root expiry and preparation refusal wake the successor in the same engine event;
no unrelated feed event is needed. No host wire or signer capability is added.

While either retained-Preflight readiness or Plan readiness owns the callback,
Session creation, derived legs, retirement and waiter changes refuse with State
before reserving IDs or consuming timers/continuations. Session-ready/failed
drains return empty; session_wait returns its pending unchanged. Read-only
session_live and the active Plan's own APIs remain available. Ordinary Session,
socket, timer and startup callbacks retain their existing Session authority.

### Lighter Schnorr signer contract (Lighter P3 part 1)

Plan: [RUST_LIGHTER_SCHNORR_RECIPE.md](RUST_LIGHTER_SCHNORR_RECIPE.md). This
defines the reserved `Algorithm::SchnorrDigest` and adds its provider arm. The
Lighter venue still signs eagerly until its admission PR.

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| `SchnorrDigest` result width goes from 64 to **80 bytes** (`s \|\| e`) | none; the variant was reserved and had no callers | reserved, 64-byte group | Lighter ECgFp5 Schnorr over Goldilocks Poseidon2. Payload is 32 entropy bytes then little-endian u64 words; a 40-byte LE scalar key | none |
| Keys are refused, not reduced | none until Lighter admission | legacy `Signer` reduces a key at or above the group order (Go semantics) | the provider arm refuses zero or non-canonical keys with `InvalidRequest` | behaviour change at admission: an out-of-range key fails typed instead of signing |
| Crypto `schnorr` feature | WASM family builds | n/a | only a family that signs through the provider enables it; without it `SchnorrDigest` refuses as `Unsupported` | none |
| `host.d.mts` `'schnorrDigest'` (already in the union) | custom JS host signers | refuse | a host that implements it must use the payload's first 32 bytes as nonce entropy and never its own randomness | none until P8 |
## L1 wallet role (P6b part 1)

`ExtraKey::L1Wallet` (`l1Wallet`) is the Polymarket L1 wallet: the raw 32-byte
secp256k1 scalar, declared by family policy (`ExtraKeyPolicy`). See
RUST_SIGNER_REMOTE_WIRE_AND_IMPORT.md, "P6b part 1". No family declares it yet;
the Polymarket conversion (#1882) adopts it. Caller-visible changes:

- **Every role names one algorithm** (`ExtraKey::algorithm`): `zklinkSeed` is
  `zkLinkContract`, `l1Wallet` is `secp256k1Digest`. A role ref signs and
  answers public operations only through that algorithm. It is never
  presented, interpolated as a credential, or used for a MAC. Exhaustive
  matches on `ExtraKey` outside this repo must add the variant.
- **Primary only.** A policy declaring `l1Wallet` for the secondary slot, or a
  secondary identity carrying one, refuses setup.
- **Address.** The wallet address comes only from `fetch_public` with
  `PublicAddress` on `identity.extra[l1Wallet]`. A wallet-only primary
  identity (no L2 key or secret yet) is valid pre-import setup.
- **Types.** The TS `ExtraKeyPolicy.key` union and `host.d.mts`
  `SigningIdentity.extra` gain `l1Wallet`.


## Socket login and native snapshot completion

`register_socket_feed_with_snapshot` declares a private authenticated feed whose
configured authentication timeout runs from physical connect until the native
snapshot is accepted. Login acceptance permits the subscription needed to obtain
that snapshot; it does not remove or renew the snapshot bound. Account ping
callbacks and queued private one-shot frames wait for accepted login; the login
frame itself remains permitted. Private one-shots use the shared
`send_after_socket_interests` custody rather than an unconditional opening send.
Pongs retain their separate receive-liveness meaning. Existing `register_socket_feed` semantics stay
the same.

The venue prepares and validates its native snapshot once, then calls
`complete_socket_snapshot()` before publishing affected Store state. The engine
requires the exact current managed text callback, physical epoch and receipt
time; callers supply no feed, socket, generation or clock. Duplicate, retired,
wrong-scope and borrowed callbacks cannot complete the barrier. Capacity refusal
retains the pending proof and deadline, so no snapshot commit is authorized by a
failed completion. A caller-modified timestamp cannot discharge the barrier.

The existing shared Supervisor owns expiry, timers and reconnect. Equality at
the immutable deadline fences ingress before native decoding/publication, even
if the host delivers its watchdog late. Hosts use the existing socket actions
and ordinary timer ingress; no new host message or wire format is required.
Snapshot completion grants no command, HTTP, signing or new send authority.


## Phemex whole-family managed socket lifecycle

Phemex public and private sockets use shared feed identity, admission, interest
release, liveness and reconnect supervision. Public receive-before-decode liveness
and native subscription/refcount/depth-seed behavior remain intact. Shared and
Disabled public-feed policies suppress the public transport while allowing a
configured private socket and its financial publication. Retired, duplicate,
late-open or idle-expired ingress cannot decode or publish through either feed.

Private login acceptance and accepted native orders snapshot are independent.
The private15s connect,12.5s PongOnly idle,5s ping and1s-to16s backoff policy is
shared. A20s snapshot deadline starts at physical connect and remains live after
login/pongs until the existing validated orders snapshot completes it before
Store publication. Positions/accounts-only and malformed snapshots do not
complete that barrier. Accepted revision-conflicted orders snapshots still do,
while preserving newer financial state as the native reducer requires.

Opening emits only user.auth999. After accepted login, the shared interest batch
emits aop_p.subscribe1202 before the retained server.ping1201. This deliberately
replaces the earlier auth,ping,ACK,AOP order; individual native JSON/signature/
expiry and RPC identities remain exact. Subsequent periodic pings require
accepted login and do not grant snapshot authority. No venue-owned socket timer,
epoch map, retry queue or financial predecoder is introduced. REST account polls,
reconciliation, HTTP/candle/history callbacks, signing and trading reducers keep
their existing owners. Bitunix retains its separately recorded legacy boundary.

## Phemex native dispatch signer custody

Phemex native REST and private user.auth signing use the shared provider at
actual dispatch. Trusted SDK/native setup consumes both credential slots; generic
venue options retain only opaque identities. Ordinary GET keeps Secondary else
Primary selection; client lookup and active-order repair keep Primary. Exact
query/body/header bytes, optional authentication on public reads, and rounded
60s protocol expiry are preserved. Hosted/delegated signing remains disabled.
Provider refusal before the first write retains its typed NotSent failure;
a later refused leg keeps accepted native IDs and cannot report the whole job
NotSent. Unknown-write repair waits for the maximum engine-observed uncapped raw
signature expiry across released legs plus5000ms. Private socket order follows
the approved shared accepted-login contract: auth999 -> accepted ACK -> AOP1202
-> opening ping1201, including a parked provider. SDK callers must provision
providers through trusted setup rather than copying generic exchange options.
### Session-scoped private egress (#1897)

Unsigned requests that carry a login or session token now take the primary
account egress. Examples are the GRVT API-key login, the BreakoutProp login and
the KuCoin keyless bullet. They never take the public/CORS route. Host callers
see a `Prepare` with intent `primary` and no explicit route, as for a primary
signed request.

This needs no host or tealstreet-next change. A host without a primary egress
fails preparation typed; it never falls back to public.

**Why `SessionEgress` carries no explicit subject (owner ruling, 2026-10-23).**
- **Grant.** The engine mints the egress only from a live session. The grant
  is an opaque `PrivateEgress`, which only a primary `SessionAuth` credential
  selection yields.
- **Session.** A session leg cannot move to another session (`issue_session`),
  and a child cannot swap its parent's egress (`issue_with_egress`).
- **Engine.** Another engine's session handle or egress-bearing context fails
  issuer validation.
- **Slot.** It is Primary only, so a slot cannot be swapped.
- **Retirement.** `validate_send` refuses a retired generation at issuance and
  at every send step.

Together these controls make the session generation the subject. Delivery,
streams and write facts of attempts sent before retirement remain recordable.
[Design and controls](RUST_BACKGROUND_SESSION.md).

## MEXC unsigned discovery and subscription snapshot custody

Markets, ticker discovery and subscription depth snapshots use unsigned managed
Background Reads with an immutable 20-second bound per operation. Public route
preparation and final release precede HTTP. At deadline equality, including before
host timer ingress, payload rows and stream completion cannot load markets or
tickers, mutate volume-unit metadata or replace a subscription book. Expired
responses are discarded before headers reach the venue, so they supply no new
Retry-After evidence. A still-live operation from an old subscription generation
retains native 429 pause evidence without publishing stale book data.

Failed preparation, submission, transport and timeout retire their own Context
and retain native startup backoff or depth resync. The first depth resync remains
exactly 500ms; discovery keeps its nominal1/2/4-second jittered backoff. The native
private startup after markets, malformed catalogue recovery, book generation and
version fences, contract-size scaling and buffer limits remain. Public reads
carry no credential and publish neither command terminals nor account-auth
evidence. Existing safe-cex startup/subscribed-order-book consumers require no
new input; host dispatch uses the existing managed prepare/bind/claim protocol.


### Hyperliquid spot candles share managed Read custody

Standalone and merged spot FetchCandles now use the existing perps public Read
Context: unsigned,20s from command admission, Never retry. Admission precedes
native interval/market/range validation. Preparation and host claim time consume
that original deadline; expired or unreleased replies cannot publish candles.
Native POST/info body, @coin mapping, header order, rows and local error strings
remain. Existing perps metadata is retained: an unreleased preparation failure or
timeout carries NotSent; a released Read timeout has no financial outcome.
No signer, Session, Plan or socket authority is introduced. Six real legacy
terminal occurrences are removed (Hyperliquid37->31, lib9->3).

## BingX history pages retain the original caller Read root

Remote fills, orders, income and unsigned public trades now require managed
Prepare/Bind/Claim. One original engine caller-wait root spans all native
windows, RangeWalk chunks, 200 ms page delays and the single signature resend.
Each physical child keeps the native 20 s bound, including preparation, capped
by that root. The caller's configured wait is retained exactly; absence of a
host override defaults at external receipt, while invalid/missing receipt
provenance refuses through shared admission without a new venue timeout.

Every native page error still rejects the whole query, even after accepted
rows: no partial rows or INCOMPLETE marker. Engine root expiry also rejects
strictly. Shared physical Network/Timeout and admission failures retain typed
Read errors with no write outcome; unclassified native errors receive the
bridge's Internal kind with unchanged text. Provider/preparation refusals keep
their typed NotSent fields and create no account-auth evidence or deactivation.
The SDK's listener timeout remains separate from engine cancellation/expiry.

GET selection remains Secondary when configured, otherwise Primary; selection
and opaque identity stay fixed through every page and resend. Native query/body
bytes, filters, dedup, sorting, limits, page caps and strict truncation remain.
The existing 100001/signature-verification refusal re-signs once per page using
a fresh root-bound child, with no new transport retry. Rejected-attempt staged
rows/errors are discarded before that resend; the old wrapper retained them,
though normal signature-refusal envelopes have no rows. Exact delivery gates
and generation cleanup make late responses inert, including after ID reuse.
Local history replies, account cycles, metadata groups, trading and the shared
legacy signer funnel remain with their existing owners.


### Backpack managed command candle stream

Supported `FetchCandles` uses genuine engine Read admission and managed unsigned HTTP. The exact X-Broker-Id header and native /api/v1/klines query survive preparation/bind/re-preparation; streaming starts only after final claim. One original twenty-second root owns the list, with zero automatic retries. Exact delivery validation runs before row parsing, malformed diagnostics and response/status handling, so expired or reused owners cannot publish rows or complete another caller.

Bounds remain floor-seconds in startTime/endTime, while native candle response timestamps remain milliseconds. Limit is ignored and no local bound filtering is added. Server order, duplicate timestamps, accepted finite negative/zero values, empty-list success and whole-list failure after a malformed row are preserved, together with the native malformed-row diagnostic. Unsupported interval, startup, WS candles, subscribed books, private sessions and history retain their existing behavior.

Initial Read admission rejection emits one typed engine failure without an outcome. Preparation refusal and parked expiry preserve the engine's local pre-release NotSent fact and typed/native metadata; this is an attempt fact, not proof about a whole command's money effects. Released transport/deadline/integrity failures retain their engine kind without a financial outcome; malformed streamed envelopes receive the shared typed Internal failure. Native row/non-2xx error messages retain their existing fields, with no partial successful list. The separate public depth-query NotSent compatibility contract stays in place. SDK worker/hosted error receivers continue to consume the existing structured fields; no upstream deployment is claimed.


## Crypto.com immediate trading command custody

Seven immediate trading refusals now require genuine credential-free Mutation
admission before publishing the native NotSent error. Invalid clocks, missing
receipts or full owner capacity instead receive the shared typed refusal once;
duplicate live ownership publishes no replacement native terminal. No native
warning or command_error is emitted for denied admission. Finite business
messages, error kinds, validation/key precedence and unchanged Store are retained.
Local missing-key refusal carries no remote credential-health evidence.

The fixed SetPositionMode acknowledgement uses shared LocalSetting admission
without a Store projection: both initial hedged values remain unchanged. Its
warning is emitted after successful completion, rather than before completion;
failed/revoked admission emits no warning. Callers receive the same [] payload
for admitted success. No arguments, signed wire bytes or remote write flow change.
Empty order/cancel jobs and the asynchronous legacy trade terminals remain for
the later managed-write conversion. Exactly eight request inventory occurrences
are removed (Crypto.com 21 to 13); credential-selection inventory is unchanged.

## WEEX public order-book query Read custody

`FetchOrderBook` retains the original caller receipt across the native shared
rate pause and one unsigned managed HTTP call. It needs no credentials, market
or subscription; native URL/limit, row order, aliases, cumulative base amounts
and exact version output remain unchanged and never publish subscription depth.
The physical call retains 20 s, capped by the original caller wait.

A query parked behind a pause now ends at that receipt deadline instead of
waiting without a venue whole-query bound. Root expiry rejects once with typed
Timeout and no write outcome; an expired/revoked 429 cannot extend the shared
pause, whereas live 429 classification and delay are preserved. A local host
listener timeout alone still does not cancel the engine command. Native
HTTP/codec errors retain their original fields, including explicit NotSent.
Released Network failure, engine physical Timeout and engine integrity failure
keep their actual typed Read cause and outcome None (legacy wrapped host or
non-UTF8 delivery failures as Network/NotSent).
Pre-release managed preparation refusals keep typed NotSent and cannot infer
credential failure from this public route. Stale generations cannot complete,
pause or otherwise affect a reused command ID or an unrelated live query.


## Backpack fixed-setting refusals

SetMarginType and SetPositionMode keep their native keyless Unsupported/NotSent
refusals ("Backpack only supports cross margin" and "Backpack position mode is
fixed") for either requested value. The genuine shared Mutation entry checks
the caller's receipt, clock and capacity before reporting the native refusal.
An invalid or unavailable receipt reports the shared typed refusal; an expired
receipt times out without renewing its bound. Duplicate commands retain their
existing owner's rights and eventual reply. No credential, signing, HTTP, socket,
timer or Store update is created by these refusals, and the command ID is
reusable after completion. No fixed-setting success or new venue capability is
added.


## BingX local candle refusals use command Read authority

FetchCandles keeps unknown-market, unsupported-interval and invalid-range
validation in that order, before credentials, with its exact native untyped
error on an admissible entry. Only these three local branches acquire an
immediate credential-free Read Context and finish it through fail_context;
there is no physical request, signer, timer or auth evidence. Valid signed and
keyless remote candle issuance and all selection sites are unchanged.

Capacity now supersedes the selected local error with the canonical Read
capacity refusal. A missing receipt from an invalid/nonadvancing clock rejects
as Revoked: "Request context was revoked", InvalidInput, no outcome. An expired
own receipt uses the existing Read Timeout rejection. No default receipt or
numeric fallback is fabricated. Duplicate live IDs preserve their diagnostic
and original owner, and local completion releases authority for immediate ID
reuse. Native absent kind/outcome remain absent; these refusals do not use the
ReadCompletion string-to-Internal projection. No TypeScript, host, CLI or AWS
production caller signature changes are required.


## Crypto.com public discovery Background custody

Instrument and ticker discovery now use credential-free managed Background
Reads with the existing 20 s physical bound starting before route preparation.
Expired rows, including a live streamed prefix ending at the deadline, cannot
publish markets/tickers, index metadata, warnings derived from failed DATA or
startup side effects. Shared preparation/capacity/timeout failure returns to
the existing jittered discovery backoff once; it supplies no command terminal
or account-auth evidence. Native retry cadence and response/status policy stay
unchanged, with no new arguments or private signing behavior.

Accepted catalogue publication retires its HTTP Context before issuing the
next ticker read, then private startup runs while tickers are pending. Ticker
success publishes quotes before retiring and opening public subscriptions.
Usable partial catalogues, refresh/index retention, malformed-exposure coverage,
independent ticker recovery and cached quotes keep their native behavior. One
legacy discovery sender is removed; key-selection sites remain unchanged.

Public ticker preparation and private legacy HTTP use independent dispatch
paths. Logical order remains ticker enqueue then private startup; if the public
route is parked, private HTTP may leave before the ticker's physical release.
The old eager path emitted the ticker HTTP first. Private startup remains
independent of ticker readiness/success; no venue defer queue restores physical
ordering. A raw-action parked-route control pins this consequence explicitly;
the immediate-route fixture proves its own ordering only.


### Orderly initial history refusals

Only the initial unsupported client lookup, unsupported income kind and shared
argument-validation refusals in history.rs now use genuine Read command entry
and fail_context. Accepted history requests, paging, signing/selection, nonce,
callbacks and queues retain their existing ownership. A finite valid entry
preserves the exact native failure text and Read outcome None; an unstamped or
expired genuine entry is rejected through the shared initial-entry helper.
Capacity refusal produces one shared failure, with no numeric fallback or
physical action. The local helper must never be called from a delayed funding
callback, accepted page delivery or queued trading work. Inventory: Orderly
28F/29O to 25F/26O, solely the three documented initial failure fingerprints.

### BreakoutProp login transport (session core): OPEN obligations

The DXtrade password login now runs as an engine session write on the primary
private egress. Login 401/403 now reaches waiting commands as
`AuthInvalid`/`AuthPermission` (NotSent) with `AUTH_REJECTED`. It no longer
arrives as untyped text.

**OPEN, required before any BreakoutProp signer-migrated or remote-ready
claim:**
- **Provider presentation:** the password is still serialized into the body
  at dispatch by the venue (claude-19pt8s).
- **Remote readiness.**

This slice claims transport and session ownership only.
[Plan](RUST_BACKGROUND_SESSION.md).

### Kraken Futures managed candle pages

FetchCandles keeps one genuine caller-wait Read root across its native ten-page
walk. Every eligible page uses a fresh unsigned Read/Never physical child, capped
at that exact original receipt deadline. Subscription pages use independent
Background Reads; generation checks still guard both publication and in-flight
cleanup, with native five-second polling and no logical20s poll-walk cap.

The explicit20s admission-to-expiry page bound is NEW: the old chart packet had
no request_timeout declaration. It includes route preparation and unclaimed Bind
waiting, as well as transport. Root expiry is strict engine-owned Timeout with
no prefix, rows or continuation. Local preparation refusal/timeout preserves
shared typed fields including localNotSent; released Read failure retains the
shared projection. No financial effect or new retry is inferred. Existing
unsigned wire, codec, sorting/duplicate winner, paging budget and validation
messages remain native. SDK host/outer wait contracts are unchanged.

A walk needs two registry Context slots: root and current physical page. Initial
root admission reserves only its own slot; if the first child cannot fit, one
strict typed Capacity/localNotSent terminal retires the root without HTTP.


## Backpack unsupported command candle intervals

Unsupported FetchCandles intervals keep the exact native refusal message and
absent error kind on a valid keyless Read entry. The shared receipt, clock and
capacity checks run first; invalid clocks and unavailable capacity report their
existing typed engine errors without a write outcome. Missing or expired
receipts refuse without renewing the caller's bound. Duplicate IDs retain the
prior owner's authority and eventual reply. Refusal creates no HTTP, managed
preparation, signing, socket, timer or Store update. Supported candle intervals,
query/stream behavior and unsupported WS subscription diagnostics are unchanged.


## CPU runtime environment mismatch evidence

Shared CPU runtime restoration reports every differing identity field, including
nested Node component versions, with the sealed expected and runner actual
values in deterministic dotted-field order. Identity rejection still precedes
all restore writes. The existing image, architecture, Node and component
bindings remain enforced; provisioner versions are outside that identity.


## WEEX history pages retain the caller Read root (#2003)

WEEX fills, normal/algo order history, public trades and income now retain the
original caller receipt deadline across native windows, pauses and physical
pages. Each physical call keeps 20 s and no retry; signed pages retain their
selected identity. GET lists remain secondary-else-primary after the existing
primary-presence gate; POST income remains primary-only.

The engine now ends the query once with typed Read Timeout at root expiry,
without rows or an INCOMPLETE marker. Before expiry, a failed page recovers only
nonempty accepted output and its owned marker; an empty accepted prefix or a
provider/preparation refusal rejects. The default caller wait is 120 s; callers
requiring longer native walks must raise rustCommandTimeoutMs. A host listener
timeout does not cancel transport or grant engine detach authority. NX remains
on its existing TS path until Rust is selected; no executed NX adoption is
claimed by this venue change.


## Crypto.com unsigned PublicTrades caller-root ownership

PublicTrades is an unsigned one-response Read with one immutable caller receipt
root across native 10ms queue eligibility, route preparation and retries. Each
eligible dequeue mints a fresh child capped at 20s and at that original root;
late or reused-owner DATA cannot parse/stage/complete. Root expiry is strict
Timeout with no prefix, INCOMPLETE marker or renewed work. Preparation/local
submission refusal is strict; a released physical-child Timeout under a live
root keeps the native retry path. Shared Context pacing additionally preserves
the native 10ms physical send gap when independent preparations are delayed.

Production/testnet URL, escaping, empty headers/body, default count25, wire
count clamp150, caller truncation, duplicate row order and native financial
fields remain unchanged. A failed streamed page is reset before retry; no
unaccepted prefix is published. Missing-array streams keep native transport
retry, whereas malformed rows in a valid array reject the whole page.

Native retry counts remain exact: three released transport retries with 1000ms
waits; the legacy 40101 hint caps at one retry, including code40001/message40101
and transport-message40101 collisions. Native code/message fields are carried
through the shared classifier. The minimum 40101 compatibility predicate uses
the original error projection: decoded error fields for a valid envelope, raw
body text for a missing-list envelope, and the transport message for a host
failure. It is a known follow-up requiring a separate caller-policy ruling
before removal. Strict route preparation keeps the shared pre-release NotSent
attempt fact; released read failures and root expiry have no financial outcome.
It grants no account-auth evidence or mutation rights. Public 40101 errors
retain typed/native metadata without changing auth health. Private history,
signing, account/trading reducers and credential selection are unchanged.

## Breakout immediate local command custody

Local candle validation, leverage results, unsupported commands, fixed-account
setting refusals and initial trade-planning failures consume their genuine
command entry through shared Read or keyless Mutation admission. Missing or
expired receipts, invalid clocks and exhausted capacity use the shared typed
refusal without renewing the caller's bound. No local result stages HTTP,
signing or timers, and duplicate IDs retain the earlier owner's delivery.

Native messages, JSON, error kinds and validation order stay intact. The local
unsupported SetStpMode Mutation now reports NotSent, an explicit added outcome
field; its native message and absent kind remain. Read failures carry no money
outcome. Physical login, valid candle/trade scheduling, queues, credentials and
financial reducers retain their existing behavior. These immediate results
introduce no successful keyless Mutation or LocalSetting path.


### Orderly unsupported order controls

Only the first trading.rs field-presence rejection for PlaceOrders containing
Bybit BBO or TickSize controls now consumes genuine Mutation entry and finishes
through fail_context. Finite valid entry preserves the exact native Unsupported
message and NotSent outcome with no Store/queue/IO effects; unstamped/expired
entry uses shared rejection, and capacity failure has no numeric fallback.
Retained PlaceOrders passed this predicate unchanged, and push_front only adds
CancelSymbolOrders, so this branch cannot borrow a dequeued job's entry. Later
account, queue-full, planning, supersession and job-terminal paths are outside
this slice. Signing/selection/nonce and accepted write/reconciliation behavior
remain unchanged. Inventory shrinks1F/1O: base28F29O to27F28O, or after the
separate #2051 history landing,25F26O to24F25O.

## Paradex config and initial local command custody

Paradex's unsigned `/system/config` GET now waits for managed Prepare/Bind/Claim
under a 20s Background Read; the bound includes preparation delay. Exact live
Background delivery precedes config decode, credential installation, private
socket opening and auth/account I/O. Failure/expiry retires that root and keeps
the native 2s retry; late/duplicate/unreleased responses cannot initialize it.

Initial unsupported and missing-account branches plus isolated-margin/hedged-mode
refusals now use genuine command admission before their one native terminal.
Native strings, validation order, Unsupported kind and existing NotSent metadata
(including native Read refusals) remain. New clock/capacity admission failures
are typed once by the engine, with Read outcome None or Mutation NotSent.
Fixed cross-margin and one-way `[]` successes use LocalSetting entry and honor
the original caller receipt, without credentials, I/O, Store changes or a write
journal. Remote signing/JWT/trading/history behavior is untouched. Exact legacy
inventory: config 1 plus initial terminals 6 removed; Paradex 48 -> 41.


## Lighter unsigned FetchAccount identity

FetchAccount now admits one caller-bound unsigned Read before selector validation.
Its native index/address candidates retain their order and each has its own20s
physical window capped by the original caller receipt. Shared capacity, clock,
duplicate and expiry refusals precede native diagnostics. Released transport
failure or physical timeout can advance to a different native candidate only
while that exact original root remains live; preparation refusal cannot. Existing
native status/row/error/referrer semantics remain unchanged. The single checked
companion diagnostic and command terminal use that root. No new caller parameter,
credential, Session, signer or private lifecycle activity is needed.


## Hyperliquid local settings and schedule validation

Standalone spot1x/cross and merged-spot leverage/margin no-ops still complete []
without credentials or writes. One-way mode still projects hedged=false; shared
LocalSetting admission now precedes that Store change and the single terminal.
These Context-free successes consume only the local command owner transaction,
not an HTTP Context or write journal. A full command-owner registry, invalid
clock or caller receipt now gives one shared typed refusal before projection.

Hedged mode, unsupported standalone spot leverage/margin, spot scheduleCancel and
invalid schedule time retain native precedence, exact messages, kinds and NotSent.
Each refusal first admits its genuine Mutation owner; shared admission rejection
supersedes the native diagnostic with its typed NotSent terminal. Schedule time
<now+5000 refuses; equality and None disarm still reach the unchanged signed path.
No caller parameter, signing/selection change, valid write, retry or timer is added.

## HTTP pacing sets and signing eligibility

A venue declares primitive floors through `http_pacing()` and optional immutable
flat sets through `http_pacing_sets()`. `PacingSet { group, constraints }` gives a
set its own declaration ID; member references name primitive IDs only. Sets share
primitive release clocks and cannot nest, duplicate members or collide with
primitive IDs. Engine construction validates and resolves bounded member indices.

A Context's existing `Pacing { group, minimum_gap_ms }` selects one primitive or
one set. Its outer minimum extends each member floor. For WOO, a set containing
global100/algo500 with outer0 charges both on algo POST; ordinary writes select
global100, PUT global200. Ordinary interleaving does not reset the algo clock.
All bounds pass before one admission; only Released charges all members. Failed
admission, stale/expired requests and waits consume none. Raw deadlines remain
immutable through signing waits and route reprepare.

Signed HTTP now retains a prepared resource without invoking its recipe/provider
while any selected primitive is blocked. Its existing timer wakes at the earliest
eligibility, route-expiry or physical-deadline boundary. Final Claim rechecks all
bounds, including releases made by other requests after signing. Hosts continue
to prepare, bind and Claim through the existing protocol; no wire change.
Admission paths requiring no pacing must use `SendPacing::validate_zero`, which
expands registered sets and checks every effective floor, including outer0 sets.
G2b's owner composes its zero-pacing helper onto this resolver before IDs; this
section does not claim that unlanded G2b adoption has occurred.

### Kraken Futures unsigned PublicTrades cursor ownership

PublicTrades admits one genuine caller-receipt Read root, retained by shared
ReadCursorHistory through its native 50-page cursor walk. Every physical page
uses a fresh Read/Never child from that original root and the existing unsigned
wire. Exact root/child/Attempt DATA admission precedes codec/progress/staging;
the physical child retires before a successor. Only the root completes.

The 20s per-child admission-to-expiry bound is NEW (the old raw packet declared
no timeout). It covers route/Prepare/Bind and transport, capped by the exact
original caller receipt. It adds neither a logical walk timeout nor a retry.
Typed preparation/queue failures are strict and retain shared fields/local
NotSent even after accepted pages. Root expiry is strict engine-owned Timeout,
with no rows or INCOMPLETE marker. Released Network/physical Timeout under a
live root and native later HTTP503/malformed DATA preserve accepted-only recovery
and the owned marker; failed-page staging never appears. First released failure
retains its typed cause (Network/Timeout), plus the native message prefix.

Unsigned native egress, sorted form, limits, exact IDs/millisecond timestamps,
row economics, descending sort, deduplication and cursor/stall rules remain.
No signing, private history, key selection, Spot or candle behavior changes.
Root plus current child require two Context slots; a child Capacity refusal
strictly retires the admitted root without I/O or a leaked reservation.
