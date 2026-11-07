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
  The Polymarket crate enables the engine's opt-in `prediction` feature for
  these transfer sections; other standalone venue builds omit their storage
  and serialization. Feature-enabled and combined builds retain the same layout.
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

### Gate phase checkpoint (review passed; shared gates completing)

Registered Wasm feature/default dispatcher, typed wallet/L2/funder options, SDK
wrapper and fixed leverage, CI matrix, public/private harness protocols and
fixtures. Root44 typecheck tasks,28 lint tasks,1,431 native Rust tests (61
Polymarket), workspace native clippy, wasm32 clippy, bans and fmt PASS.
255 harness tests PASS. Actual SDK/Wasm file12 tests PASS, including accepted
batch identity through host Error fields, matched partials, final cancellation
proof and floor sizing in both frame consumers.

Independent gate review found2P2s: freshly signed stale clocks could be masked,
and DELETE/order unexpected nested fields were normalized. Fixed canonical
L1/L2 seconds and order milliseconds freshness before masking; restrict
entropy normalization toPOST/order. Freshly re-signed negative fixtures and
independent counterexamples now fail as intended; review PASS. Taped Gamma
metadata independently requires both public subscriptions and private auth.

Scope: benchmark is2Gamma markets,4outcome markets,2full books. An unfiltered
live attempt traversed>43,000Gamma rows/~295MB before the recorder120s startup
timeout; it is EXCLUDED from the fixture and gate claims. Default unfiltered
engine still performs the complete keyset walk. Explicit optional marketSlugs
scopes production discovery and this reproducible fixture, using exact
repeated slug parameters. Filtervalidation/cursorcycles/failurepaths are
regression tested; no hidden cap or changed gate threshold.

The scoped genuine20s capture has191frames and2books; the trimmed fixture
keeps185records/182frames. Initial three-run public PASS: TS/Rust workerCPU
25.0/19.5ms, steady8.6/3.6ms, retained21.2/0.8MB; selectedWasm795,169bytes
against1,258,291budget. PrivatePASS: allstate/fills/commands/exactsignedwrites,
31records/12frames/7commands, TS/RustCPU142.7/56.5ms informational.
A later recheck under simultaneous local verification failed TS backing-buffer
settling before comparison (11samples/8transitions/1,025ms); the strict
1second deadline is unchanged. Rechecking unchanged after broad checks.
Binance public/private, combinedWasm suite and root tests remain before PR.

### Shared gates and final contract audit

The recheck passes unchanged: public TS/RustCPU49.7/39.6ms, steady23.4/16.2,
retained21.1/0.9MB; private70.7/65.5ms correctnessPASS. Combineddefault
Wasm build PASS, SDK87 actualWasm tests PASS, host/wire suite PASS. Binance
public3runs PASS CPU218.8/113.8ms, steady116.2/50.5, retained18.6/3.0MB;
privatePASS. Root `yarn test`27tasks PASS in addition to44typecheck/28lint.

Final API contract audit found global cancel-all missing: generic SDK fallback
only canceled cached orders, while TSPolymarket calls DELETE/cancel-all. Added
venue-wide endpoint even with empty local cache, typed cancellation scopes,
strict proof for known IDs while accepting additional server IDs, global
mutation exclusion, and SDK override. Native trading now22tests PASS and
all-target clippy PASS; new13threalWasm regression awaits rebuiltbinary.
Private fixture now also exercisesglobal cancellation against an unseenID.
Next: verify repair, commit, integrate currentlanding (AsterDEX+Kraken),
repeat affectedchecks, thenPR/adversarialcomments/greenCI/merge/freshbranch.

Cancellation repair verified:13/13actualWasm boundarytests and updatedprivate
gate33records/12WSframes/8commandsPASS (CPU90.7/66.9ms informational).
Independent repair review additionally found token-scope emptyACK could falsely
confirm cancellation. Token scopes now carry their capturedexpectedIDs too;
empty/subset ACK errors preserve unconfirmedorders.24trading tests and crate
all-target clippyPASS. Independent global/token repairreviewPASS.

### Landing integration checkpoint

Merged landing ecb313026f (AsterDEX, Kraken and ApeX Omni) preserving venue
registrations, HTTP deadlines, currency balances and orphan ticker identities.
Prediction frame sections13/14 remain optional with legacy columns unchanged.
Independent merge review: engine61, wire12, SDK frame4 and incoming wrappers33
checks PASS. Merged harness295/295PASS after making the Polymarket ethers
oracle lazy: selecting other venues must not load its signing dependencies.

Landing adds account/book/STP and symbol-routed cancellation commands. Native
Polymarket routes cancellations by exact nativeID without requiring cached
symbols, exposes signer account and validated complete REST books, and rejects
STP settings/orders before writes. Independent integration audit found a100level
SDK default would truncate Polymarket REST books. Fixed optional limit (Aster
keeps100);150level default/explicitlimit actualWasm regression PASS. History7,
trading26 and boundary15 tests PASS; selectedWasm848,107bytes below1,258,291.
Root44typecheck/28lint, wasmclippy and workspace native tests PASS. Remaining
combined gates and PR review will be recorded at their actual head.

Public gate at5096e3c13e PASS3runs: TS/RustCPU48.8/31.6ms,
steady23.2/9.9ms, retained21.3/0.9MB. Private33records/12WS/8commandsPASS,
CPU87.0/49.8ms informational. Workspace1,748tests androot27testtasks PASS.

### Paradex landing integration

Landing advanced to1f9e9a5206 while combined build ran. Integrated its explicit
binary transport, tape hooks and crypto cleanup without touching Polymarket's
text feeds. All318harness tests and44incomingSDKwrapper testsPASS. Independent
manifest/host/lockfile auditPASS: allincomingfeatures/versions retained, only
Polymarketcrate/dep additions. Incoming host parsers are feature gated; expose
account/book forPolymarket aswell asAster, preservingothercommandgates. Native
feature tests and fullcombinedbuild/gates running beforePR.

Incoming crypto cleanup removed dynamic typed-data production helpers; the
Polymarket independent signing test now uses alloy-dyn-abi only as a dev
 dependency, then the unchanged shared ECDSA primitive. Independent signing
reviewPASS: complete V2 schema/domain unchanged, direct production ABI encoder
not reused by the hash oracle. Lockfileaddsoneexistingdevdependencyedge only.
Combineddefault buildPASS; all91actualWasm SDK tests and55host testsPASS.
Root44typecheck/28lint/27testtasksPASS afterParadexintegration. Binancepublic
PASS3runs CPU294.3/140.1ms, steady166.2/55.6ms, retained19.2/3.3MB; bothprivate
replaysPASS. Remainingfinalnative/public/selectedbundle checks completing.


### Final local gate checkpoint (590fc8012c)

AlllocalgatesPASS:1,857nativeworkspace tests (74Polymarket), workspacealltarget
clippy, wasmclippy, fmt andbans; root44typecheck/28lint/27testtasks;
318harness,55host,91actualWasmSDK tests. Polymarket-only hostfeaturetestsPASS,
15actualWasm boundarytestsPASS. Final selectedbrowserbundle823,423bytes
versusunchanged1,258,291budget. Public3runsPASS:TS/RustworkerCPU48.7/30.4ms,
steady23.6/9.3ms, retained21.3/0.9MB. Fixture185records/182replayevents,
4outcomes/2books, explicitlyscopedgenuine20secondcapture asdisclosedabove.
SelectedprivatePASS:33records/12WSframes/8commands, exactsignedwrites/state/
fills/commands/subscriptions; CPU69.6/66.6ms informational. Combineddefault
build+bothprivategatesPASS. Binancepublic3runsPASS CPU294.3/140.1ms,
steady166.2/55.6ms, retained19.2/3.3MB. No thresholds orprotocolchecksweakened.
Next: pushPRintofeat/rust-rewrite, freshadversarialcorrectnessreviews/comments,
addressfindings, waitallCIgreenbeforemerge andfreshfollowupbranch.


PR#1327 openedintofeat/rust-rewrite. Three freshadversarialreviews cover
signing/trading, public/private/query state, andengine/SDK/harness integration.
ReviewfindingswillbepostedonthePR; CI mustbegreenbeforemerge.


### Adversarial PR review and repairs

Three independent reviews were posted on PR #1327. Shared engine, frame, SDK
and harness integration passed. The trading reviewer reproduced P1 replacement
oversizing after a cumulative baseline plus a new execution, and P2 acceptance
of missing final `size_matched`. The state reviewer reproduced invalid default
candle filters and stale LIVE events restoring an authoritatively removed order.

Repairs retain distinct cumulative amount/association pairs, count observed
executions outside each pair, preserve prior lower bounds, and require explicit
final size plus coverage of all known native execution IDs. Independent re-review
found a further delayed-execution ordering: replacing a pair before receiving its
execution lost evidence. Keeping all distinct observation constraints fixes it;
the counterexample independently failed before and passed after the repair.
Final trading re-review and integration re-review pass with no further blocking
findings. New tests exercise both maker/taker paths, missing/null/wrong coverage,
fractional units, delayed executions and valid catch-up proofs.

Successful complete order snapshots now fence removed known IDs while preserving
newer stream revisions. Candle defaults use the API-defined max horizon; an
upper-only request requires `from`, which must be at least 1000ms because zero
is treated as absent. Fidelity and exact millisecond filtering remain intact.
The max horizon depends on fidelity and is not an all-history guarantee.

Verification: 87 native venue tests, 16 actual-Wasm boundary tests, nine TS
history tests (19 focused TS/frame tests), crate clippy, Wasm clippy, fmt, bans,
root typecheck and lint pass. The repaired selected bundle is 836,933 bytes.
A public rerun failed TS backing-buffer settling before comparison (22 samples,
19 transitions, 1011ms). The one-second deadline is unchanged; rerunning with
other checks finished. Initial CI had 30 successful checks and one false-positive
secret finding: the documented fake `safe-fake-secret` fixture. Its exact
historical fingerprint is recorded; local full-range secret scan passes.

Repair gates pass at a15c51c4c2: public three-run TS/Rust worker CPU47.6/34.2ms,
steady23.7/14.2ms, retained21.3/0.9MB. Private correctness passes (33records,
12WSframes, eightcommands). All87native venue tests and16actualWasm boundary
tests pass. The independent trading and integration re-reviews pass.

### Phemex landing integration

PR became conflicted after landing advanced to45d7a3fb98. Resolved four shared
registration/harness/lock conflicts, retaining the landing structure and all
Phemex text-auth, parser and host message APIs. Independent host/manifest/lock
and SDK audits pass; all340harness tests and47SDK/oracle tests pass. Prediction
sections13/14 and repaired venue state are unchanged. Incoming Phemex increases
the backing-buffer settling deadline from1s to5s and adds GC diagnostics; CPU,
retained-memory comparison criteria and1,258,291byte size budget are unchanged.
Polymarket had passed the stricter1s deadline before this merge. Combined build
and affected native/SDK/replay checks are running; merge still requires greenCI.

### Coinbase landing integration

Landing advanced to5c0e437faf. Resolved shared registration, harness, roster and
wire-test conflicts without changing reviewed Polymarket venue code. Incoming
Coinbase aliases/auth/account/currency/product/client-ID contracts are preserved.
Independent SDK/manifest audit passes (65tests); currency and prediction frame
review passes (13wire and14frame regressions). Engine and all87Polymarket native
tests plus crate clippy pass. All368harness cases pass across the full runs and
targeted rechecks: the first run encountered an unresolved SDK marker; a later
run hit the existing200ms readiness test under build load (292ms), with its
focused recheck running unchanged. No protocol or gate criteria were changed.
The combined build and affected replay/SDK checks are finishing; final proof
will be posted on the PR and merge remains conditional on greenCI.
