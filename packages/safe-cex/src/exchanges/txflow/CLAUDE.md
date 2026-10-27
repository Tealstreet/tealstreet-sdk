# TxFlow Exchange Notes

Read `../CLAUDE.md` first for shared exchange-integration conventions.

## Exchange information

Exchange name: `txflow`, USDC crypto perpetuals on testnet only. Both the TS
reference and Rust `sdk-rs/venues/txflow` reject non-testnet startup before I/O.
Production registries load `RustEngineExchange`; the comparison adapter lives
only in `src/__tests__/reference/txflow`.
Native order history supports symbol/time/limit; the wrapper rejects `orderId`
and `status` filters before dispatch because the engine query cannot carry them.
Mainnet is offline per the venue team. Keep `enabled: false`,
`enableInPreview: false`, and the existing development availability. This port
does not enable production, preview, Outcome, spot, TxRelay or fund transfers.

## API documentation

- [Complete official API reference](https://docs.txflow-testnet.xyz/llms-full.txt).
- [Signing](https://docs.txflow-testnet.xyz/api/signing.md).
- [WebSockets](https://docs.txflow-testnet.xyz/api/websocket.md).
- [Exchange responses](https://docs.txflow-testnet.xyz/api/exchange.md).
- [Rate limits](https://docs.txflow-testnet.xyz/api/rate-limits.md).
- Canonical testnet REST: `https://rpc.txflow-testnet.xyz`, sockets `/ws` and
  `/api-ws` on that host. Do not use the older SDK/default hosts. Mainnet's
  future constant is not an executable adapter endpoint.

## Authentication and signing

`applicationId` is the main account wallet, `secret` its already-approved
agent private key. An optional `vault` selects the queried/traded vault.
Unsigned reads/subscriptions use that account identity, never the agent's
address. Private startup validates the account and local signing key before
I/O. This connector does not approve agents, builders or fund accounts.

TxFlow has no CEX-style API keys; the venue-side `key` field is unused. TxFlow never returns an agent private key. Tealstreet must generate the agent wallet locally, send only the agent address in `approveAgent`, and have the owner wallet sign that approval. TxFlow records the approved agent address for the account; day-to-day trading is signed with the locally stored agent private key.

Private/trading support therefore requires a custom wallet + agent-key onboarding flow, not the generic API-key paste form. Do not expose a generic TxFlow API-key field unless TxFlow later ships a real venue-issued key model.

The testnet EIP-712 domain is `TxFlow-Testnet`, version `1`, chain ID `2511`,
zero verifying contract. Agent schema:
`Agent(string txflowNetwork,uint32 chainId,uint32 apiVersion,bytes32 connectionId)`.
Signing API version `1` differs from HTTP version `2026-08-16`; send the latter
as `x-api-version` on every `/info` and `/exchange` request.

Reuse Hyperliquid's MessagePack named-map preimage/hash implementation, **not**
its fixed-domain signer or action builders. Ordinary TxFlow signed bodies omit
`type`, preserve documented map order, omit absent optional fields, keep prices
and sizes as exact strings, and encode assets/order IDs as integers. Append
nonce BE8 plus vault flag/address bytes before Keccak-256. `claimBuilderRewards`
and `vaultTransfer` have special rules and are not exposed by this adapter.
Direct account-key actions also use separate schemas and are not exposed.

Rust uses `safe-cex-crypto` low-S secp256k1 signing. TS uses ethers and the shared
Hyperliquid preimage helper. Official domain/connection-ID/digest/signature
vectors are pinned independently in both test suites. Nonces are checked,
monotonic milliseconds; uncertain writes never retry. Although the venue
allows re-signing an HTTP 429 after `Retry-After`, this adapter currently
propagates write failures rather than resubmitting automatically.

Public REST reads have explicit public CORS routing and skip remote signing;
account reads retain ordinary account routing. Real venue URLs remain in the
venue code; hosts select proxy/tunnel policy.

## Public data and sockets

- Stream `perpMeta` through Rust's array splitter, cache explicit `index`
  identities and reject duplicate IDs/symbols. Accept `currencyType: usdc`, drop
  delisted markets and mark `haltTrading` markets inactive. Requests use numeric
  string IDs for `coin`, independently of display names. Prices use `priceTick`,
  amounts `basePrecision`; TS retains native metadata in `Market.info`. Rust
  retains typed instrument metadata because the packed market frame has no
  generic raw-info field.
- Seed tickers with per-market `activeAssetCtx`; publish the Rust seed only
  after every matching instrument response succeeds. Live node `allAssetCtxs`
  supplies mark/oracle/funding; API-service `ticker` supplies last/24h fields.
  Funding percent becomes an engine fractional rate. Native base OI becomes
  quote OI using mark price.
- Node `/ws` subscriptions repeat channel type at top level and inside
  `subscription`; `/api-ws` subscriptions omit the top-level type. Send fixed
  10-second `method: ping` on both even under continuous market traffic.
  Recognize node `method: PONG` and API `channel: pong`. Rust uses engine
  timers/watchdogs and role-tagged sockets. Reconnect restores subscriptions
  and invalidates stale books.
- Pace subscription frames on each socket by 150 ms, with a bounded FIFO and
  independent heartbeat. The gateway drops burst frames without identifying
  which subscription was lost. `RISK_RATE_LIMITED` restarts that socket through
  its normal recovery path and rebuilds current intents; discard old queued
  frames/timers. The live fixture confirms all 78 ticker acknowledgements.
- Keep all supplied book levels, discard older snapshots, and retain subscription
  intents made before metadata arrives. Candles use `/api-ws`; historical
  candles use `candleSnapshot` and engine-second output timestamps.

With Rust public-feed policy `Disabled` or `Shared`, native metadata and ticker
seeds still use REST. The public API socket is suppressed; the node account
socket retains private subscriptions, reconciliation, pings and rate-limit
recovery, while skipping public contexts, books and candles on every connection.
Unsolicited public payloads cannot overwrite shared prices on that account
socket. Private equity/PnL remain the venue's authoritative account fields;
provider tickers do not recompute them.

## Private data and reconciliation

REST account state, orders and executions reconcile every five seconds;
per-market settings refresh separately once per minute with paced reads.
Native pushes update clearinghouse state, orders and settings. Settings remain
separate from nonzero positions. Successful complete reads alone may replace
state; revisions/generations fence delayed replies behind pushes, accepted
writes and disconnects.

`/api-ws userFills` is a historical snapshot, not a live execution feed. Read
paged REST `userFills` from the session's startup timestamp, keep native `tid`
identities across reconnects and emit exactly one fill per native execution.
This includes executions received in the initial response after startup; older
history emits nothing. Never infer execution price from cumulative order data.
The 100,000 execution-ID cap fails explicitly rather than evicting identities
and replaying fills. Retired order metadata is bounded separately from live
orders and terminal replay fences.

## Trading and client IDs

Supported placement is plain Limit/Market (`Gtc`, `Ioc`, `Alo`, native
`FrontendMarket`/`bbo`) within documented size/precision limits. Serialize
commands, validate whole batches before any write, and recheck queued intent
against current state before dispatch.

Caller IDs are honored verbatim on one plain native order: nonempty, at most
64 UTF-8 bytes, and no `0x` prefix (reserved by TxFlow for LP accounts).
Generated orders receive distinct `ts_` IDs. Caller IDs are reserved across
queued/in-flight jobs; no caller ID is rewritten or silently dropped.

Whole-action responses have **one** status even for a batch, and never return
order IDs. Request and require echoed `confirm: executed`; consensus-only,
pending/202, malformed or lost replies are uncertain outcomes. Placements
resolve native IDs via authoritative cloid mappings from reads/pushes. Never
use Hyperliquid's per-order acknowledgement parser or manufacture an order ID.

`modify2` changes total size in place and preserves identity/fills. Omitting
`tpsl` clears protections, so amendment requires confirmed empty children and
revalidates that condition at dispatch. Leverage/margin/mode writes require
semantic execution acknowledgement; queued settings use current confirmed
state. Hedge leverage requires an explicit rail and is refused by the scalar
setter. Position mode changes require a flat account without working orders.

Reduce-only auto-cap, conditional/TP-SL/bracket behavior, partial/infinity
stops and notional-margin placement remain fail-closed pending the live
measurements required by the original venue guidance. Do not advertise those
capabilities or borrow Hyperliquid's behavior. Unknown execution controls and
unsupported custom market slippage are rejected before writes.

## Queries

Use documented offset/`hasMore` pagination for fills, ordinary limit/market
order history, funding and deposit/withdraw ledger reads. Preserve exact
milliseconds for filters, seconds for execution output, native execution IDs,
signed costs/rebates and venue-reported gross closing PnL. Unknown statuses,
failed/malformed pages, stalled continuation and exhausted offsets are explicit
errors, never successful partial arrays. Funding has no transaction ID;
financial row fingerprints keep opposite settlement legs distinct and reject
ambiguous identical rows rather than silently collapsing value.

Deposit/withdraw income requires the documented `completed` settlement status;
unverified statuses fail explicitly instead of treating pending intent as
settled cash. Account-wide transfers are excluded from symbol-scoped queries.
The native conditional/TP-SL history families have distinct formats and are not
included in the ordinary order-history method. Reported closed PnL has no
connector implementation; the inherited branded fallback remains visible.
No documented REST public-trades history query exists in the enumerated API;
use its public trades stream. Indexed client-ID lookup cannot prove an
uncertain placement absent and must not return a definitive miss.

## Validation

`docs/plans/TXFLOW_RUST_PORT.md` records the phase plan, reviews and gate evidence.
The initially empty TS scaffold was implemented as the testnet replay reference;
it must not be substituted with Hyperliquid's protocol in the harness. The
public fixture comes from a live testnet recording; private tapes use documented
payloads and the official disposable signing-vector key only.


## Rust public discovery resilience

Rust catalogue/ticker reads retry through the shared engine capped exponential
backoff. A malformed typed public row is skipped with a warning while valid
neighbours remain usable. Private account initialization starts once every paced
ticker seed read has been attempted (TS records the account execution window
after the seed), even if some reads are still retrying. Account snapshots,
mutation acknowledgements and signing validation retain their stricter rules.
See `docs/plans/RUST_STARTUP_AUDIT.md` and the native startup resilience regressions.

TxFlow retains an explicit catalogue-level refusal for duplicate or ambiguous
historical native identities/aliases: ignoring such a conflict could send a
signed order or label history with the wrong instrument. That failure is logged,
preserves the prior store, and retries with the public discovery backoff. Ticker
read failures retry independently by native asset while later seed reads and the
private account continue.
