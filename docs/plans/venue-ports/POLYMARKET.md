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

### Public phase checkpoint

Implemented typed outcome markets with atomic streamed Gamma discovery, staged
REST midpoint seeding, lazy public socket, full sorted depth and every absolute
change, reconnect snapshot requirement, raw PING/PONG watchdog, dynamic tick
updates, and bounded sampled candles. Optional typed prediction metadata passes
through native frames and both SDK consumers; legacy numeric payloads unchanged.
Signing primitives are prepared and independently checked against official
client v1.1.0 (eight V2/1271 vectors and 72 amount-rounding cases).

Independent public phase review found four P2s: stale ticker/tick rollback,
missing snapshot arrays interpreted empty, unfiltered returned candle bounds,
and fidelity units. All repaired with focused regressions. Startup reads have
bounded retries; midpoint snapshots commit only once complete. Verification:
14 venue tests (6 signing, 8 public), 56 engine tests, 10 wire tests, 12 SDK
frame regressions, safe-cex typecheck and frame lint; crate native clippy passes.
Full host suite needs default Wasm rebuild (cached feature binary lacks Binance).

Known TS fixes to register in the gate: raw initial book arrays were ignored;
price_change updated tickers without depth and throttled away intermediate
updates; unsorted snapshot levels; stale quote/tick rollback; missing tick size
updates; fidelity60 misread as1minute. The Rust behavior follows official raw
CLOB protocol and retains all full-depth changes. Private implementation next.

### Private phase checkpoint

Implemented supplied L2 or fresh-server-time L1 credential derivation, all
private socket/auth/reconnect hooks, signed paginated orders, Data holdings and
pUSD balance reconciliation, fail-closed partial/failed snapshots, order revision
and terminal fences, holdings trade fences, maker/taker execution attribution,
settlement deduplication, and valid zero resolution prices.

Independent private review confirmed the aggregate balance PnL omission: fixed
both read completion orders and added a regression; failed positions preserve
last known PnL and an early holding response cannot mark balance loaded.
Cancellation notifications retain their final cumulative matched high watermark
for the following trading phase without emitting cumulative fills. Verification:
15 account + 8 public + 6 signing tests PASS, native crate clippy PASS. No
remaining P1/P2 findings in reviewed private scope. Trading implementation is
prepared in separate uncommitted files and is integrated after this checkpoint.

### Trading and queries checkpoints

Trading: exact V2 GTC payloads, all4 account signature types, full-batch
preflight, secure random salt, exact signed bodies, sequenced writes, reported
partial-success IDs, scoped and proven cancellations, and locked cancel-replace
jobs. Fresh terminal read must cover every known cumulative/execution lower
bound before replacing; fractional remaining shares round DOWN to avoid
exceeding caller total. No ambiguous write is retried. Financial precision
uses shared engine helpers; JavaScript binary formatting is not emulated.

Independent trading review found3P1+2P2: losing prior accepted batch IDs,
treating matched ACK as fully terminal, ignoring trade-only final-fill evidence,
rounding fractional replacement leaves up, and late live ACKs publishing zero
filled after an execution. All fixed; reviewer authored5 reproductions outside
the implementation, verified they fail before repairs and pass afterward.

Queries: price history uses interval fidelity in minutes and exact caller ms
bounds, sorted/deduplicated points and limits. Four unimplemented history
contracts reject explicitly; client-ID lookup remains nondefinitive because
caller IDs are unsupported. Account setters acknowledge without writes and
leverage reads return1 on both rails. Independent query review PASS.

Verification:57 native venue tests PASS (10 primitive/account-unit,19 account,
18 trading,8 public,2 queries), native all-target clippy PASS. Both small phases
share the stable checkpoint commit after their separate reviews passed.

### Harness preparation / live discovery evidence

Wasm feature/default dispatch and SDK wallet-only options are registered. Real
Wasm SDK8 tests cover public metadata/books in both receivers, supplied proxy
credentials vs derived EOA auth, pUSD/funder holdings, and batch ID rejection.
Actual TS public/private reference defects are repaired with regression tests;
there is no rewriting of Rust answers into the TS oracle. Independent harness
checks authenticate exact HMAC bytes and V2 economic order fields before
normalizing entropy. Generic nested-client discovery includes all3 HTTP APIs.
251 harness tests PASS.

A genuine20second public capture initially produced10 frames and2 books but
revealed Gamma silently caps legacy responses at100 despite limit500.
Offset100 returns the next100; offset10000 explicitly rejects and directs
clients to /markets/keyset. Official keyset docs confirm max100, markets array,
next_cursor -> after_cursor, and no offset parameter. The capture will be
replaced after complete keyset discovery is implemented in Rust and reference
TS; this avoids claiming a truncated market universe passed.
