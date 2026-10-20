# Phase 9 product semantics

Base: `a7caec9314` from `origin/feat/rust-rewrite`. Branch:
`feat/rust-product-semantics`. This work retains every TS venue and keeps
Rust opt-in. Follow `VENUE_PORT_BRIEF.md`; full-auto execution is authorized.

## Committable phases

1. Carry typed instrument classification through an optional indexed frame
   section. Preserve absent versus empty native labels, market region and
   underlying ticker. Reconstruct only documented semantic fields. Verify real
   Rust discovery, transferred frames and `prepareCliOrder` TradFi refusal,
   including stock, ETF, commodity, FX, crypto, innovation and unknown labels.
2. Implement Bybit combined linear/inverse/options categories inside its
   existing crate. Forward saved flags, preserve category-specific feeds,
   account reconciliation, routing, quantities, option terms and Greeks. Test
   each enabled combination and existing standalone profiles.
3. Extend OKX's existing crate to native inverse swaps and EU xperp futures,
   preserving account/host/demo routing, native quantities, quote valuation,
   subscriptions, commands and histories. Test product and consumer sizing.
4. Run independent adversarial phase reviews and repair findings. Run bans,
   format, strict Clippy, native/host/frame/CLI tests, root typecheck/lint/tests,
   fresh public/private replay gates for touched venues and Binance profiles.
   Record correctness, request coverage, CPU, retained memory and artifact size.
5. Update phase-9 readiness rows; push a PR into `feat/rust-rewrite`, publish
   correctness reviews, fix and repeat. Merge only after green CI on the final
   head, then cut a fresh branch from the landing branch.

## Audit decisions

- Optional indexed semantic sections keep ordinary market columns unchanged;
  replacing markets invalidates metadata and a metadata change emits a market
  replacement. Native labels are retained rather than guessed into crypto.
- Venue agents own disjoint crates; the primary agent owns shared frame,
  wrapper and consumer integration. Artifact builds and replay gates have one
  owner to prevent stale or mismatched generated bindings.
- An inverse-only alias does not prove combined-account parity. A perpetual
  fixture does not prove dated futures/options, and synthetic private traffic
  does not establish live connectivity.

## Evidence

Implementation, phase audits and gate results are appended at stable commits.

### Implementation checkpoint

- WIP checkpoint `a3f9cee4a5` preserved the implementation before merging
  landing `0063068241` in `a7ce59126b`; no stash or rebase was used.
- Shared engine tests: 74 PASS. Existing Binance native control: 99 PASS.
  Bybit: 137 PASS (123 existing plus 14 product regressions), strict Clippy
  PASS. OKX: 77 PASS before the live
  optional-cap regression described below. The combined native matrix passes.
- Root typecheck: 44 tasks PASS; lint: 28 tasks PASS; full repository tests:
  27 tasks PASS, including 505 safe-cex files / 7,329 tests. Focused current
  wrapper/inverse consumer regressions: 20 PASS; actual CLI policy tests: 22 PASS.
- Independent phase reviewers found and closed: wrong inverse native-PnL
  labeling in the shared consumer; incomplete EU history joins; Bybit global
  history limits hiding later categories, sibling settings replacement,
  reconnect healing only one category, malformed category snapshots erasing
  state, IV bypass on non-option protective branches and cold inverse mark
  fallback. These are implementation-review findings, not all TS bugs.
- The classification integration fails against the previous artifact because
  the native semantic section is absent. With the new artifact it passes real
  Bybit discovery, worker frame application, actual buffer transfer, main frame
  application and `prepareCliOrder`: stock/ETF/commodity/FX refuse, empty and
  innovation crypto pass, absent and future labels remain unknown.
- First fresh 20-second public capture: Bybit 17,591 frames / 912 markets.
  OKX/EEA captures expose empty optional order-type caps on
  inactive preopen rows; native parsing and the independent TS reference are
  being corrected before the public gate is accepted.
- Final native/combined WASM, public/private replay, artifact-size, adversarial
  PR review and exact-head CI evidence are still required. Live private
  credentials, Hyro origins and combined options connectivity are not claimed.

### Scope reassignment

The user moved item 4 (GRVT non-perpetual products) to
`feat/rust-grvt-products`. All GRVT product changes, reference hooks, tapes,
fixtures and product tests were removed from this branch. GRVT retains only
mechanical absent-IV initializers and unsupported command arms required by the shared order boundary. Its
non-perpetual readiness blocker remains assigned to the separate lane; this PR
implements items 1–3 only and makes no GRVT retirement decision.

### Final capture and review checkpoint

- Landing advanced with the TxFlow and native server lanes; merged
  `c10f7e190a` without rebasing in `760a8a53ab`. Native server command-ID
  remapping now includes all three option queries.
- Fresh live public captures ran for 20 seconds: Bybit 17,591 WS frames,
  OKX global 96,223, OKX EU 141,371. Before any public CPU comparison, each
  was trimmed to the brief's standard first six seconds: 4,299 / 22,666 /
  37,822 retained records respectively, including HTTP/catalog anchors.
  EU books are actual `BTCUSD_UM_XPERP310404` and
  `ETHUSD_UM_XPERP310404` products. Hyro retains documented fixture evidence;
  no live endpoint has been configured.
- Actual native live catalogs exposed two TS edge cases: optional empty
  specialized order caps on a CRML preopen instrument and empty last/open
  prices on an untraded TEST002 X-Perp. Typed native guards and actual TS
  class reference regressions distinguish unavailable empty values from
  malformed nonempty values.
- A private replay mismatch exposed inconsistent OKX balance calculation:
  position pushes used slice maintenance margin, while formatter ticks used
  full initial margin. Native cached risk now preserves all positions,
  converts coin risk at the instrument reference, and retains open-order
  reserves. Independent peer review additionally caught a reference freshness
  overreach: only the same cached raw balance object may be superseded by an
  account push; a distinct newer REST object remains authoritative.
- Focused product/CLI/reference integration: 14 PASS before the final
  malformed-risk regression. Final workspace, artifacts and replay evidence
  below supersede intermediate counts. Private tapes are documented protocol
  evidence, not live credentialed connectivity or empirical verification of
  ambiguous OKX private margin-unit documentation.

### Accepted local gates

- Current landing workspace native suite: 2,476 tests PASS, followed by the
  final engine 74 / Bybit 137 / OKX 83 regressions and strict workspace
  all-targets Clippy. WASM Clippy, workspace format and bans PASS.
- Root typecheck 44 tasks and lint 28 tasks PASS after the landing merge.
  Earlier full-root test run passed 27 tasks including 7,329 safe-cex tests;
  final combined-WASM SDK suite passes 384 tests / 12 files, host 68 tests,
  frame-wire integration one, harness 557 and product integration 16.
- Adversarial final review closed Greek identity loss through frames (old
  artifact demonstrably fails), malformed/overflowing native OKX risk erasing
  proven margin, and infinite leverage eliminating order reserve. Native and
  actual-class reference regressions cover the repaired boundaries.
- Bybit single-venue artifact: 1,108,674 bytes, below 1,258,291 budget; SHA256
  `ee22fdc06244cf29fb86c018cd34bb8e21c6eb3c23b7ce237aff2a851f288c52`.

Public medians over three runs. CPU columns are the merge gate's main-thread
and steady-state measurements; process CPU is retained in raw logs and is
informational. Private tapes require correctness; their performance is not a
public performance claim. Every accepted gate also compares output and native
request/write/signature/protocol contracts.

| Profile | Frames / markets / books | Main CPU TS / Rust ms | Steady TS / Rust ms | Retained TS / Rust MB | Public / private |
| --- | --- | --- | --- | --- | --- |
| `bybit` | 4295 / 912 / 2 | 174.2 / 138.3 | 80.9 / 47.6 | 22.0 / 5.0 | PASS / PASS |
| `bybitinverse` | 708 / 26 / 2 | 118.3 / 34.1 | 75.7 / 12.8 | 14.1 / 1.6 | PASS / PASS |
| `bybitspot` | 4339 / 527 / 2 | 144.4 / 106.3 | 79.1 / 45.0 | 16.2 / 3.1 | PASS / PASS |
| `bybitdemo` | 5833 / 912 / 2 | 222.3 / 162.5 | 112.9 / 63.6 | 21.8 / 5.0 | PASS / PASS |
| `hyrotrader` | 160 / 2 / 2 | 19.6 / 18.9 | 3.8 / 3.4 | 13.3 / 0.8 | PASS / PASS |
| `binance` | 873 / 917 / 3 | 199.0 / 91.4 | 109.8 / 36.3 | 18.0 / 4.0 | PASS / PASS |
| `binancespot` | 7045 / 3729 / 2 | 939.4 / 598.7 | 471.9 / 238.9 | 27.4 / 10.8 | PASS / PASS |
| `binanceinverse` | 202 / 20 / 2 | 238.4 / 95.1 | 166.1 / 51.6 | 13.7 / 1.9 | PASS / PASS |
| `binancepm` | 873 / 917 / 3 | 254.3 / 127.6 | 152.0 / 57.6 | 17.7 / 4.0 | PASS / PASS |

### Final matrix findings retained

- The first EU private gate failed TS startup because the generator removed
  optional `instType` from all records, leaving no exact scoped TS replies.
  Native unfiltered and TS filtered product requests remain distinct; the
  fixture is being partitioned by the raw native product, including pushed
  subscription topics. This failure is not an accepted gate.
- The first global OKX public gate matched output and memory but failed
  steady CPU: TS 830.4 ms, Rust 833.1 ms; main CPU TS 1,138.1 / Rust
  1,106.6 ms, retained 19.1 / 5.1 MB. The unchanged six-second fixture has
  3,675 index updates; each scans 514 markets and performs a hash lookup,
  about 1.89 million catalog lookups. A native index-to-market fanout cache
  is being added before remeasurement. No threshold or fixture is changed.

Binance single-venue control artifact: 928,783 bytes (budget 1,258,291),
SHA256 `c7e62e853aefc0d7220121889079180a145699429b81f91df979b59a4b65368f`.
All eight USD-M/PM/Spot/COIN-M public/private gates PASS.

### Explicit EU account and live quote closure

The final EU contract uses exact SWAP and FUTURES account requests on both
implementations. Native position/order reads join both scopes atomically under
one generation/version; failed, malformed, stale or retired later-product reads
cannot commit a partial replacement. Balance reads retain SWAP account currency
`balData`. Unfiltered scripted replies were removed, and missing/unknown filters
fail exact matching. Native tests reached 90 with strict Clippy PASS.

The index fanout diagnostic improved the public gate without changing either
public fixture or threshold. Its EU replay then caught explicit blank live
last/open quotes overwriting the real TS seed with NaN. The production TS socket
handler now preserves proven last/change for those unavailable fields; malformed
nonempty values remain visible. Actual-class blank/partial/malformed regressions
pass. A first full product-script attempt used the single-venue OKX artifact and
correctly failed the six Bybit artifact-dependent cases; it is not product or
final verification evidence. The rebuilt combined artifact is required below.

The earlier performance commit's phrase “no fixture changes” refers to the
public benchmark fixtures. The scripted private tape was deliberately repaired;
its changes and the two failed EU attempts are recorded above.
