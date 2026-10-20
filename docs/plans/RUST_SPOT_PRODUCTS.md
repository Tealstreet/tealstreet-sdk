# Hyperliquid and Kraken spot products

Branch: `feat/rust-spot-products`, based on `origin/feat/rust-rewrite`.
This extends the existing venue crates; TypeScript venues and registry defaults
remain in place. Explicit Rust selection must preserve saved spot accounts.

## Plan and verification

1. Public data: distinct spot constructors, native spot identities, typed
   streamed markets/tickers, full books, candles and socket recovery. Register
   WASM aliases and SDK selection; capture public tapes for each spot product.
2. Private data: native spot balances, currency/coin details, flat position
   settings, open orders, executions, polling/reconnect and stale-read fences.
3. Trading: whole-batch preflight, supported order shapes, client-ID contracts,
   native amend or safe replacement, cancellation and fixed spot settings.
4. Queries and consumers: match implemented versus inherited TS capabilities;
   exercise both aliases through actual compiled hosts and worker consumers.
5. Harness gates: documented private tapes, live public fixtures, both touched
   perpetual products and Binance regression gates, repository checks and CI.
6. Landing: merge current landing branch, PR into `feat/rust-rewrite`, independent
   code-correctness reviews posted to the PR, fixes/reviews until passing, merge
   only on green CI, then cut a fresh follow-up branch.

Every implementation phase receives an independent sub-agent review and a
stable commit with progress below. No stash, TS deletion, master changes,
rebases or force pushes. Gates retain their existing correctness/CPU/memory
thresholds. Credentialed private coverage uses documented scripted inputs and
must not be described as live-key verification.

## Product and authentication decisions

Hyperliquid spot shares the L1 action signer, sequenced writes and common
public feed transport with perps. It joins spot universe entries to token
indices, uses `10000 + universe.index` for native action assets, and spot
price precision. Wallet/application/vault identity and network choice must
remain consistent across REST, subscriptions and signing. Perpetual account,
builder, fee and HIP-3 discovery requests must not leak into the spot product.
Native spot balances need raw currency details plus USD valuation; failed or
older REST snapshots cannot overwrite newer socket state. Spot-only capability
decisions follow the actual TS spot methods and documented upstream support.

Kraken spot is a distinct state machine inside the existing Kraken crate:
`KrakenSpot::new(Options)`. Futures signing and WS challenge auth cannot be
reused as a spot protocol. Spot signs the exact form body including monotonic
nonce with `HMAC-SHA512(path || SHA256(nonce || body))`, decoded API secret,
and `API-Key` / `API-Sign`. Every private WS connection obtains its own REST
token before subscriptions; old tokens/responses cannot authenticate a new
generation. AssetPairs REST keys, altnames and WS symbols require explicit
identity indexing. Native `txid` stays canonical across atomic amend; generated
`cl_ord_id` does not imply support for caller IDs. Trailing placement uses the
documented positive relative distance; ambiguous edits fail before writes.

## Progress

- Planning: current brief/spec and TS spot contracts inspected; independent
  implementation lanes assigned to the two existing crates, with a separate
  harness lane. Host/consumer integration and gate orchestration stay with the
  parent. Constructor and auth plans audited before implementation.
- Public checkpoint review found and authors fixed Kraken WS-v2 BTC/XBT
  translation, subscribed-depth truncation, and Hyperliquid metadata read
  retries. Initial Hyperliquid suite: 77 tests including 19 spot tests; strict
  crate Clippy/format passed. SDK caller-ID/fallback tests: 18 passed. TS spot
  metadata/ACK/public protocol regression subset: 34 passed.
- Follow-up private/trading/query review found and verified the fix for a
  Hyperliquid fill-before-ACK resurrection race, including partial/duplicate
  fills. Kraken's current typed triggers object and externally placed iceberg
  read compatibility were also repaired and independently checked. Spot uses
  open-only positions and fixed settings, as the global specification requires.
- Initial Hyperliquid live public capture failed with 1006 reconnect loops and
  no accepted books. This is not a fixture candidate. Check spot socket URL
  and replace unsupported nSigFigs 1/6 subscriptions with documented supported
  variants. Corrected live capture reached two books, then its replay exposed
  another real defect: HYPE is listed against USDC, USDT0, USDH and USDE, while
  TS forced every quote to USDC. Both implementations now resolve the actual
  quote token, preserving every pair and native action/book/candle identity.
  USD balance valuation must prefer a direct USDC market or convert quote units
  through a bounded price path, keeping unpriced raw holdings. Duplicate-base
  identity/conversion tests and fresh public recapture are required. No live
  public gate is claimed yet.
- Kraken frozen full crate: 122 tests (93 existing futures, 29 spot) pass;
  strict crate Clippy, formatting and diff checks pass. Public/private/trading/
  query review findings are resolved, with focused independent regressions.
- Hyperliquid frozen full crate: 84 tests (26 spot) pass; strict crate Clippy,
  formatting and diff checks pass. New quote identity and valuation changes
  received another independent review, including the cyclic conversion fix.
- Harness phase review passes: independently checked 9 protocol tests and
  4 compiled Kraken tests. Three compiled Hyperliquid tests also pass after
  fixing the deterministic rig's timer pumping. No account economics are
  rewritten in the replay gate.
- Final DOGE/cancel-sentinel review passes: 2 native DOGE regressions,
  12 TS parity regressions and 10 protocol tests independently pass. SDK subset
  62 tests, package typecheck and lint pass. Crate and SDK fixes are committed
  at `d7c1d415df`, `a2c7f7c666`, `b9f51b2bc8` and `3f0413cdb6`.
- Kraken spot private gate passes: 19 frames, two markets/tickers; exact
  requests, auth, fresh token rotation, account, trading and query results.
  TS/Rust informational CPU 85.1/47.5 ms; retained memory 14.7/1.0 MB.
  Successful cancel void normalization is operation-scoped; failures and
  economics are unchanged. Recheck on the final rebuilt artifact.
- First Kraken live capture includes 5,043 WS frames and two books, but exposed
  rejected XDG subscriptions. Pair TS/Rust XDG/XXDG-to-DOGE normalization,
  test inbound/outbound identity and balance valuation, then rebuild/recapture.
- Work-in-progress checkpoint `3972cc5f54` committed before merging the landing
  branch cleanly at `16e4fa340e`. No PR exists yet. Selected Kraken WASM build,
  private/public/parent-venue/Binance gates, final repository checks, independent
  harness review, measurements and PR remain pending.
- TxFlow landing integrated at `30a16c7a2a`; incoming replay/tape/responses,
  routing and memory snapshot changes are preserved. Readiness now counts
  49 registered names / 46 Rust aliases / nine defaults / 37 opt-in / three
  absent products. Independent merge review found two stale test expectations,
  repaired without enabling credentialed spot smoke profiles. All 562 harness
  tests and the 332-test SDK/registry/transport/readiness subset pass.
  Integrated default WASM strict Clippy, all-workspace formatting and bans pass.
- Full safe-cex suite passes: 513 files / 7,427 tests before the final three
  decimal-step regressions; the subsequent affected 43-test subset passes.

## Accepted Kraken gates

Integrated source `3fc4c20aa0` plus paired canonical-decimal fix `4a2dfa8477`:
fresh live public capture has two books, zero rejected subscriptions and
successful DOGE/USD, DOGE/USDC and DOGE/USDT ticker feeds. Committed fixture
contains 6,169 trimmed records. Four compiled host tests pass.

| Gate | Frames | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Kraken spot public, three runs | 6,166 | 282.8 / 144.4 | 222.7 / 83.4 | 16.4 / 4.3 |
| Kraken spot private, one run | 19 | 56.5 / 45.2 | 27.5 / 22.8 | 14.3 / 1.0 |
| Kraken futures public, three runs | 9,641 | 1234.0 / 243.3 | 865.5 / 142.9 | 16.7 / 2.7 |
| Kraken futures private, one run | 49 | 110.7 / 60.0 | informational | 16.4 / 1.1 |

All four gates PASS. Private performance is informational; private correctness,
requests, authentication, token rotation and command/query results are exact.
Selected browser and Node WASM are both 1,051,446 bytes, below 1,258,291.
The TS book-readiness predicate waits for the startup-created socket after
ticker seeding, preserving listeners across startup; the live replay exercises
this lifecycle. Accepted Hyperliquid and Binance gates follow below.

- Final auth review: Hyperliquid spot rejects malformed or mismatched local
  API-wallet credentials before I/O; missing credentials keep public mode and
  main-wallet/vault read identity remains independent. SDK validation waits for
  native startup before accepting retained balance flags and preserves startup
  rejection/disposal errors. Native crate 87 tests (29 spot), strict Clippy and
  formatting pass; independent review reran 29 native spot tests and 47 SDK
  wrapper/validation tests, all passing with no open findings.

- Native server-adapter landing merged cleanly at `f5d60003af`. Independent
  merge review confirms all 46 aliases and incoming workflow are preserved;
  562 harness, 428 SDK and 100 server-session tests pass. Full workspace strict
  Clippy, default WASM strict Clippy, formatting, bans, SDK and root typecheck/
  lint pass. Both touched native crates pass 209 tests (87 Hyperliquid and
  122 Kraken). Secret-scan preflight identified only the two fixed public
  replay-vector declarations; exact historical fingerprints document their
  synthetic provenance, with production and future changes still scanned.


## Accepted Hyperliquid gates

The final selected artifact integrates landing commit `f5d60003af` and the
paired TS spot protocol/identity fixes. A fresh live 20-second capture contains
69 records: one public metadata response, one open and 66 websocket frames.
Both native book identities are verified against metadata: `PURR/USDC` and
`@107` (`HYPE/USDC`). No error frames or rejected subscriptions occur. All 69
records are preserved in the committed fixture. Capture metadata is
`source: live`, `private: false`, epoch `1792479120698`; the gzip SHA-256 is
`f77d2f497741baa047d2ae2cf272107f201109bf51415011c4fd9a152295c712`. Three compiled spot
host tests pass, including native cancellation identity, failed-cancel amendment
safety and unsupported-protection preflight.

| Gate | Frames | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Hyperliquid spot public, three runs | 67 | 61.2 / 46.5 | 23.9 / 17.8 | 18.6 / 6.0 |
| Hyperliquid spot private, one run | 10 | 85.0 / 52.2 | 33.3 / 38.0 | 18.0 / 5.1 |
| Hyperliquid perpetual public, three runs | 285 | 310.5 / 46.7 | 22.4 / 13.9 | 24.1 / 2.6 |
| Hyperliquid perpetual private, one run | 26 | 197.9 / 49.2 | 50.0 / 30.9 | 20.4 / 1.1 |

All four gates PASS. Private performance is informational; account economics,
requests, native fill identity, signatures, subscriptions and command/query
results compare exactly. TS preserves execution `tid` as the fill `id` and
exposes order `oid` as `orderId`, matching the global native fill contract.
Both implementations subscribe to the current four spot account feeds;
ignored feeds and perpetual-only asset subscriptions are removed from TS.
No economic output or subscription normalization hides these fixes.

Selected browser and Node WASM are both 1,050,258 bytes, below 1,258,291.
All four gates use this unchanged selected artifact (SHA-256
`b5c39c39c5bc54788d22f59b9a5d5b836b7139930a718a382905be88273a0905`).
The paired TS private protocol fixes do not change native runtime source;
private generators contain 16 spot and 53 perpetual records.
Rust linear memory is 1.2 MB for spot public, 1.6 MB for perpetual public and
0.3 MB for both private fixtures. The native author additionally verifies
88 crate tests (30 spot), including distinct executions sharing one order ID,
plus strict Clippy and formatting. Accepted Binance gates follow below;
combined-host checks, PR correctness review and green-CI landing remain pending.


## Accepted Binance regressions

The existing Binance public fixture is unchanged (gzip SHA-256
`cc3f426ededcee74edd3e70d4b20c564867a8009a325b43a422fbfd9f6460790`).
The documented private generator produces 30 records. Both gates use the same
selected Binance artifact, with no intervening build or threshold changes.

| Gate | Frames | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Binance public, three runs | 873 | 195.3 / 88.5 | 106.8 / 35.6 | 18.0 / 4.0 |
| Binance private, one run | 14 | 59.6 / 44.5 | 21.8 / 22.0 | 13.0 / 1.1 |

Both gates PASS. Public parity covers 917 markets, 783 tickers and three books
(`BTCUSDT`, `ETHUSDT`, `SOLUSDT`); private parity covers two markets/tickers,
account state, authentication and commands. Private performance is informational.
Selected browser and Node WASM are both 902,790 bytes, below 1,258,291;
SHA-256 is `c71f22e1086c5bbeb352938a9dcc824dd313c53aa7f5e63d5fb25c4478ccf5c7`.
Rust linear memory is 1.5 MB for public and 0.3 MB for private replay.
All touched Spot products, their parent perpetual products and Binance now pass
the public/private gates. Combined-host validation, PR correctness reviews and
green-CI landing remain pending.

## Final local gates and consumer checkpoint

Gates 1–6 PASS on the integrated branch, including the five public and five
private spot/parent/Binance replays above. The latest GRVT-only landing merge
`f95f50007c` preserves its optimization exactly; independent full harness rerun
passes 565 tests. Both TS venues remain present and selected by default.

The combined default WASM passes 396 SDK tests across 12 files, including all
46 registered Rust aliases through actual worker startup, transferred frames
and late-consumer snapshots. Spot identities are independently derived from
raw metadata. Host tests pass 68/68; compiled Hyperliquid spot contracts 3/3
and Kraken spot contracts 4/4 also pass against this combined artifact.
An initial combined run caught missing Kraken Spot request-auditor initialization
in the consumer test transport, repaired with the existing scoped Kraken setup;
no native or fixture rewrite was needed. The native author independently audited
that setup and confirmed Hyperliquid requires no counterpart change.

Root `yarn typecheck`, `yarn lint` and `yarn test` pass; safe-cex runs 7,434
tests before the final two private-protocol regressions, followed by passing
affected suites. Independent final review passes 23 SDK tests and 30 native
spot tests with no findings. The two native crates total 210 passing tests
(88 Hyperliquid and 122 Kraken); full workspace strict Clippy, default WASM
strict Clippy, formatting, bans, license and secret-scan checks pass.
Crypto helpers remain shared by the parent products; this extension replaces
no crypto implementation and leaves no newly provable unused crypto helper.

Remaining process: push and PR into `feat/rust-rewrite`, post independent
code-correctness reviews, fix/review findings, merge only after green CI, then
cut a fresh follow-up branch. Private live credentials remain unverified;
scripted private coverage must not be presented as a live account test.

## PR code-correctness review checkpoint

PR [#1339](https://github.com/Tealstreet/tealstreet-sdk/pull/1339) targets
`feat/rust-rewrite`. Three independent reviews are posted as PR comments:
public/host/harness (no new findings), trading/queries, and auth/account.
Trading found a spot-only zero-disabled bracket regression: `stopLoss: 0` or
`takeProfit: 0` must omit that child, preserving TS behavior. The spot preflight
and wire filter are fixed; negative/nonfinite batch protection and the parent
perpetual contract remain intact. Native full crate passes 90 tests; reviewer
independently reran 32 spot, 11 Kraken trading and 18 TS contract tests.
Rebuilt Hyperliquid gates are in progress; the source change requires an
updated artifact hash and superseding measurements before final push.

Auth review found two Kraken P2s now being fixed: a final trade and filled
lifecycle row in one batch can double-emit, and exhausted explicit signature
rejection can warn instead of failing account validation. Both findings have
standalone repro probes; closure requires independent regression review and
rebuilt Kraken gates. No merge until all findings close and CI is green.


## PR #1339 Hyperliquid correctness revalidation (supersedes prior metrics)

The PR trading review found that zero-valued spot `stopLoss`/`takeProfit`
fields must mean disabled, as in TS. Native validation now accepts those
sentinels and omits their bracket wires, while rejecting negative/non-finite
triggers and preserving perpetual validation. Independent trading review is
clear; 90 crate tests (32 spot), strict Clippy and formatting pass.

The following results supersede the earlier Hyperliquid measurement ledger
for the rebuilt PR artifact. Fixtures, thresholds and replay economics are
unchanged; all four gates run serially on the same selected artifact.

| Gate | Frames | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Hyperliquid spot public, three runs | 67 | 39.1 / 35.5 | 10.8 / 10.1 | 18.5 / 6.0 |
| Hyperliquid spot private, one run | 10 | 50.4 / 49.9 | 33.1 / 36.0 | 17.7 / 5.1 |
| Hyperliquid perpetual public, three runs | 285 | 251.2 / 46.5 | 21.5 / 13.5 | 24.1 / 2.6 |
| Hyperliquid perpetual private, one run | 26 | 208.8 / 48.6 | 85.1 / 30.4 | 20.9 / 1.1 |

All four gates PASS; three compiled spot controls also PASS. Public spot and
perpetual replays retain two books with 302 and 341 markets/tickers respectively.
Private parity covers two spot and three perpetual markets/tickers; performance
is informational. Native linear memory remains 1.2 MB for spot public, 1.6 MB
for perpetual public and 0.3 MB for each private replay.

Rebuilt browser and Node WASM are both 1,050,441 bytes, below 1,258,291;
SHA-256 is `a3624f475129d5278defcdd062fefa914e6402f2eaa462bda0bdb8f3ef63ae47`.
The 69-record live spot fixture and its recorded provenance/hash remain unchanged.
Kraken PR correctness fixes, final combined-host revalidation and green-CI
landing remain pending.

- Kraken review fixes are frozen and independently closed: only trade rows
  emit fills, filled rows retain lifecycle closure, and a batch-local order
  cursor preserves incremental progress. The TS lifecycle-as-fill defect is
  explicit in `KNOWN_TS_FIXES`; no economic normalizer hides it. Exhausted
  explicit signature rejection now emits Error/Deactivate after one fresh-nonce
  resend; malformed nonempty native secrets fail before I/O. Missing credentials
  still select public mode. Full crate passes 128 tests (35 spot, 93 futures),
  formatting and strict Clippy. The reviewer independently passed 17 account
  tests and rebuilt both original failing probes, now 2/2 passing.
- Rebuilt selected Kraken passes five compiled host controls, including one
  native execution for a repeated final-trade/filled batch at exact last price/
  quantity. Raw wire retains the closed terminal row; the SDK's existing frame
  policy filters it. Both touched crates now total 218 passing native tests.
  Final Kraken replay gates and rebuilt combined-host checks follow before
  pushing the reviewed fixes; CI must pass at the exact final PR head.


## PR #1339 Kraken account revalidation (supersedes prior metrics)

The PR account fixes stage same-batch order progress and emit executions only
from trade rows, retaining filled lifecycle closure without a second fill.
Exhausted explicit signature rejection and malformed nonempty signer material
now surface native error/deactivation rather than silently leaving startup
pending. Independent review closes both findings; all 128 native crate tests
(35 spot, 93 futures), strict Clippy and formatting pass.

These results supersede the earlier Kraken measurement ledger for the rebuilt
PR artifact. Existing live public fixtures are unchanged; freshly regenerated
private tapes contain 36 spot and 82 futures records. All four gates run
serially on the same artifact, without threshold or economic-output changes.

| Gate | Frames | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Kraken spot public, three runs | 6,166 | 538.8 / 346.3 | 428.8 / 214.6 | 16.4 / 4.3 |
| Kraken spot private, one run | 19 | 61.5 / 52.6 | 31.8 / 30.3 | 14.8 / 1.0 |
| Kraken futures public, three runs | 9,641 | 1545.7 / 392.3 | 1147.8 / 238.2 | 16.7 / 2.7 |
| Kraken futures private, one run | 49 | 133.8 / 56.3 | 35.6 / 29.5 | 16.3 / 1.2 |

All four gates PASS; root also verifies five compiled spot controls, including
the native batched final-trade/lifecycle regression. Public parity retains
773 spot and 207 futures markets/tickers with two books for each product.
Private parity covers two markets/tickers per product; performance remains
informational. Rust linear memory is 2.5 MB for spot public, 1.0 MB for futures
public and 0.3 MB for each private replay.

Rebuilt browser and Node WASM are both 1,054,495 bytes, below 1,258,291;
SHA-256 is `2a71b1c87e99ea665fa31856c52910b0ebb499f4f34143b6f98c9b8649273385`.
Both touched venue families now pass their superseding PR-fix replay gates.
Final rebuilt combined-host validation, the final review commit and exact-head
green CI remain pending before landing.

- Latest landing sync brings Lighter broker/signing variants and the explicit
  GRVT product retirement decision. Incoming structure and protocol changes are
  preserved; both spot aliases remain. Runtime inventory is now 49 names / 47
  Rust aliases / nine defaults / 38 opt-in / two absent (`bitunix`, `kucoinspot`).
  Merge conflicts were limited to additive alias help/refusal lists and
  readiness documentation, combined without dropping either lane. All ten
  venue gates pass on reviewed artifacts; incoming WASM changes are Lighter-
  feature-gated and do not alter the selected spot/parent/Binance builds.
  Rebuild the combined host and re-audit the integration before final push.

## Final integrated acceptance for PR #1339

All implementation and review findings are closed. Final source includes
`e7f5bee863`, `177f57aa6d` and landing merge `f491ae2e7c`. The three independent
code-correctness reviews, finding repros, fixes and closure results are posted
on [PR #1339](https://github.com/Tealstreet/tealstreet-sdk/pull/1339).
No TS venue was deleted or switched to Rust by default.

| Gate | Final result |
| --- | --- |
| 1 — banned patterns | PASS |
| 2 — native formatting, strict Clippy and tests | PASS; 218 touched-crate tests, full workspace all-target strict Clippy |
| 3 — default WASM strict Clippy | PASS |
| 4 — private replay | PASS; both spot products, both parent products and Binance |
| 5 — public replay | PASS; both fresh live spot fixtures and parent/Binance regressions, three runs each, exact output and lower Rust CPU/memory |
| 6 — Binance regression | PASS public/private; both touched parent products also pass public/private |

Final combined build passes 418 SDK/WASM tests across 12 files, including
all 47 native products through actual worker startup and late-consumer frames.
Host tests pass 68/68; compiled spot controls pass 3 Hyperliquid + 5 Kraken.
Independent latest-merge review passes 571 harness and 459 SDK tests with no
findings, preserving incoming Lighter/GRVT behavior and both spot aliases.
Root typecheck (44 tasks), lint (28 tasks) and tests (27 tasks) pass; safe-cex
passes 7,482 tests across 516 files. Final post-merge bans, workspace formatting,
workspace all-target strict Clippy and default WASM strict Clippy pass again.
Selected rebuilt artifact measurements above supersede the earlier checkpoints.

The final reviewed head must receive green CI before merging into
`feat/rust-rewrite`; PR checks and the merge record are authoritative for that
landing. After merge, cut a fresh follow-up branch from the landing branch.
Private data remains documented/scripted evidence, without a funded live-account
claim. Standalone spot support does not enable combined Hyperliquid
`enableSpotMarkets` accounts or authorize TS retirement/default migration.
