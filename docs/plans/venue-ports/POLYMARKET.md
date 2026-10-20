# Polymarket CLOB V2 Rust port

Branch `feat/rust-polymarket`, based on `origin/feat/rust-rewrite` (373cc09864).
Follow `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`; full auto; no stashes,
rebases, force pushes, or master changes. Every phase ends with independent
sub-agent review, repairs, focused verification, and a stable commit. Final
PR targets `feat/rust-rewrite`, review findings are posted, green CI precedes
merge, and a fresh branch follows. Development-only venue enablement remains.

## Venue-specific mapping and plan audit

Read the venue CLAUDE first, plus package/exchange/store contracts. Sources:
[official V2 migration](https://docs.polymarket.com/v2-migration),
[official raw WebSocket events](https://docs.polymarket.com/market-data/realtime-data),
[official pinned client](https://github.com/Polymarket/clob-client-v2), and
Gamma/Data endpoint documents linked in the venue CLAUDE. Current docs also
present unified clients: this port preserves the explicit CLOB V2 raw protocol
used by the pinned TS integration, never legacy V1 orders.

* One Gamma outcome token is one engine market. Token uint256 identity stays
  an exact decimal string; symbols remain slug + '-' + outcome. Zip encoded
  `clobTokenIds`/`outcomes` only when valid and equally sized. Gamma offset
  pagination (500) is separate from CLOB cursors (`MA==` through `LTE=`).
  Quote pUSD, price probability in [0,1], amount shares, contract size1,
  leverage1. Retain condition/outcome/question/description/endDate/slug/tick
  and neg-risk as typed metadata, including through both frame consumers.
* Existing numeric market/order/position shapes suffice. The sole engine
  extension is optional typed prediction metadata (and holding redemption/
  merge flags). No opaque JSON info blobs, derivatives enums, leverage model,
  extra signing events, or token-as-f64 identities. Legacy frame layouts stay
  byte-compatible when optional metadata is absent.
* REST midpoints seed tickers in batches100. Lazy market socket consumes
  book snapshots and every absolute-size price_change, sorting sides and
  deleting zero sizes; initial arrays and direct objects both decode.
  The documented protocol has no numeric contiguous sequence: never invent
  one or treat hash as a sequence. Require a fresh socket snapshot after
  reconnect and withhold unsynchronized books. Tick-size changes update
  validation/signing metadata. Raw PING every10s, PONG watchdog every socket.
  Candles are sampled price-history points (OHLC equal, volume0), REST fetch
  and5s subscriptions; do not manufacture venue trade volume.
* Public/Private roles retain real distinct URLs, stable feed identities,
  reconnection with fresh state, and no embedded host routing policy.
* Account holdings come only from paginated Data /positions, keyed by funder
  (wallet for EOA). Long-only shares, cost basis, current valuation, cashPnL,
  zero liquidation. CLOB balance-allowance is pUSD base units /1e6 with
  signature_type. Reconcile every10s; malformed/failed/incomplete read never
  commits empty; stale snapshots cannot reopen terminal orders or overwrite
  newer events. Only actual trade executions emit fills, with dedup across
  MATCHED/MINED/CONFIRMED repeats and maker/taker role attribution.
* L1 auth reads /time then ClobAuthDomain/version1/chain137 with documented
  attestation and nonce0; derive API credentials via signed GET. Supplied
  complete L2 credentials work directly. Partial options fail closed. L2
  signs exact seconds+method+path+transmitted-body with decoded URLsafe
  base64 secret; all5 POLY headers. Do not include query if official signer
  excludes it. No retry of ambiguous writes or credential mutation.
* Orders sign EIP712 domain Polymarket CTF Exchange/version2/chain137;
  standard and neg-risk verifying contracts differ. Exact uint256 salt,
  token, amounts and timestamp; side/signatureType uint8; metadata/builder
  bytes32; expiration is wire-only0. Types0/1/2 and POLY1271 type3 require
  official ERC7739 wrapper, signer/funder distinction and independent oracle
  vectors. Use host cryptographic entropy for salt, injected time for ms
  timestamp; never invent deterministic salt uniqueness across engines.
* Whole-batch preflight rejects client IDs, market/conditional/trigger/
  bracket/reduce/close and alternate TIF, slippage/BBO before any signing or
  write; finite valid tick-bound price, rounded positive size >= minimum,
  active known market, supported tick. Sequence native GTC POST /order jobs;
  no write retries. Cancel responses must prove cancellation. Cancel-replace
  needs fresh terminal order details after cancel to account final fills and
  prevent replacement while old order can remain live. Derivatives settings
  retain documented1x/one-way no-write behavior; no fabricated leverage.
* Queries match implemented TS capabilities; unsupported history/fills/income/
  public-trades contracts return explicit errors, rather than claiming a
  constant endpoint implements them. Client ID lookup is unsupported. Price
  history is implemented and bounded with caller timestamp/limit validation.

## Phases and verification

1. Public: typed streaming Gamma discovery/midpoint seed, books/tickers,
   lifecycle/watchdogs, candles; optional metadata frame support; native,
   wire/SDK regression tests; phase review and commit.
2. Private: L1/L2 lifecycle, direct private stream, deduplicated executions,
   authoritative paginated holdings/orders/balance and stale fences; native
   scripted tests/failure races; phase review and commit.
3. Trading: official V2 vectors all4 signature types, batch preflight, exact
   native bodies, cancel failures/final-fill-safe replace; phase review and
   commit.
4. Queries: supported candles and explicit unsupported responses, parameter
   validation and no side effects; phase review and commit.
5. Gates: register Wasm/SDK/harness and CI; genuine20s two-book capture,
   documented synthetic private tape (no credentials/live orders), exact
   outbound auth/body/subscriptions validation, no gate weakening, known TS
   fixes explicit. Run bans/fmt/clippy/native/wasm, SDK typecheck/lint/tests,
   both Polymarket gates and Binance public/private baseline. Record gate
   CPU/memory/size and all counts, phase review, stable commit. Then push/PR,
   adversarial correctness reviews posted and repaired, merge target (resolve
   preserving landing structure), wait for green CI, merge/fresh branch.

## Progress

* Plan: source audit complete; fresh branch created; signing and narrow frame
  implementation delegated to disjoint scopes. Public implementation next.
