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
this lifecycle. Accepted Hyperliquid gates follow below; Binance final gates remain pending.

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
plus strict Clippy and formatting. Binance final gates, combined-host checks,
PR correctness review and green-CI landing remain pending.
