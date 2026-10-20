# Binance / Bybit trading audit fixes

Branch: `feat/rust-audit-binance-bybit`, based on landing `bd94abb524`.
Scope: the requested trading findings in `RUST_AUDITS.md` (audit source
`df2d6a4a03`); TypeScript venues remain. No engine default changes.

## Phases and acceptance

1. **Binance private executions.** Reverify H1 (COIN-M triggered stop expires
   before its fill) and H4 (USD-M non-TRADE fills and missing trade-ID dedupe).
   Separate lifecycle freshness from execution identity. Test documented trade
   and lifecycle packets, duplicates, ordering, and late terminal executions.
2. **Binance trading and reconciliation.** Reverify H2 (cancel/fill race can
   replace full original size), M5 USD-M/PM stale snapshot resurrection, and
   UUID failure fallback. Defer replacement sizing until cancellation evidence;
   retain newer fills/terminal rows and fail closed on uncertain cancellation.
   Propagate client-ID RNG failure through placement planning. Test the actual command/event
   races and all changed family paths.
3. **Bybit private/trading.** Reverify M5 Demo/Hyro canceled snapshot race,
   M10 inverse amend units, and execution-ID dedupe. Test same-time REST/write
   races, native contract vs store units, repeated and distinct executions.
4. **Integrated verification and landing.** Independently review each phase,
   fix findings, run gates 1–6 for both families and all affected variants,
   SDK typecheck/lint/tests and host checks. Record actual numbers and evidence
   limits. Strike resolved audit bullets, push, PR into `feat/rust-rewrite`,
   post adversarial code-correctness reviews, fix/repeat, merge only on green
   CI at the reviewed head, then cut a fresh follow-up branch.

Every code fix receives a regression reproducing its failure scenario. Existing
code determines whether an audit item still holds; resolved items are recorded
as such rather than changed again. Reviews must inspect error paths and stale
responses, not merely test success. No account-value normalizer will be added
for the fixes. TS differences are documented in `KNOWN_TS_FIXES` and the PR.

## Verification plan

- Bans; format; strict all-target Clippy; complete Binance/Bybit native suites.
- Strict default WASM target Clippy; selected family release builds/size checks.
- Public and documented private replay gates for Binance USD-M, PM, COIN-M,
  spot; Bybit linear, inverse, spot, Demo and HyroTrader. Existing committed
  public captures remain the regression oracle because no public behavior is
  intentionally changed; Hyro's public fixture is explicitly synthetic.
- Root `corepack yarn typecheck`, `corepack yarn lint`, `corepack yarn test`;
  compiled WASM/host tests and harness checks as applicable.
- Public Rust CPU/retained memory must beat TS at unchanged thresholds.
  Private documented tapes prove scripted behavior, not live credentials or
  funded trading. The new native regressions pin adverse race behavior that
  the existing final-state replay oracle does not cover.

## Progress

- Initial inspection: clean worktree, no existing audit PR; fetched origin,
  read landing audit and brief before creating the requested fresh branch.
- Reverification underway against current implementations; H1, H2 and H4
  confirmed. Existing inverse/spot safety state does not cover USD-M/PM.

### Bybit phase review checkpoint

- M5 confirmed for Demo (removed rows lack timestamp fences) and Hyro (a REST
  reconciliation can start during a cancel, after dispatch but before its ACK).
  Order revisions now fence accepted cancels and terminal stream events, with
  regression cases at an identical engine timestamp, cancel-all, already-gone,
  partial batch rejection and full rejection controls.
- `execId` previously unused: bounded 4,096-ID per-product dedupe now survives
  reconnect and preserves distinct executions with equal economics. Invalid
  quantity/price packets do not consume identity. Legacy packets without IDs
  and duplicates older than the retained window cannot be guaranteed deduped.
- **M10 dropped after reverification.** The cited local amend assignment is
  reachable only for Demo/Hyro. Those variants cannot enable inverse products;
  standalone inverse and combined linear/inverse wait for authoritative stream
  updates. The new regression passed before production edits and preserves
  coin `remaining` plus native `remainingNative` across amend/stream updates.
- Verification: 147 native tests, strict all-target Clippy and format pass.
  Pre-fix Demo/Hyro snapshot, repeated execution and same-clock terminal failures
  were captured locally before the corresponding fixes.
- Independent Binance trading author reviewed the entire Bybit phase: no open
  correctness findings. Root additionally required malformed-first/valid-retry
  execution coverage; implemented and passing. TS differences documented beside
  `KNOWN_TS_FIXES`, with no new comparison normalization.
- Public live capture succeeded: 19,981 WS frames, 3 HTTP responses, both books;
  existing committed public and documented private variant gates are running.

### Binance phases 1–2 review checkpoint

- H1/H4 confirmed and fixed: only typed `x=TRADE` with finite positive last
  quantity/price emits derivative fills; native trade IDs dedupe in a bounded
  16,384-execution window across reconnects. Genuine delayed executions after
  stop-trigger `EXPIRED` or cancellation still emit. Distinct reordered trades
  emit independently while cumulative state remains monotonic.
- H2 confirmed and fixed: every profile defers amend replacement planning until
  cancellation evidence, subtracts maximum observed cumulative execution, and
  refuses unknown/malformed/mismatched cancellation. Regular replies require
  native identity, terminal status and quantities; USD-M algo and PM strategy
  replies use their separate native acknowledgement contracts.
- M5 USD-M/PM confirmed and fixed: acknowledged cancels and private terminal
  events fence in-flight and replica-stale snapshots. PM regular and strategy
  generations have separate native identity maps; captured replies and older
  same-family events cannot mutate a newer binding of a caller client ID. Old native generations are
  retired when a new one binds in a stream event or accepted REST snapshot.
- UUID fallback confirmed and fixed: entropy failure propagates through every
  native order formatter. Actual Engine command tests inject failures in plain,
  conditional, trailing and bracket generation before any HTTP write.
- Independent phase reviews found PM reused-client-ID and regular/strategy
  namespace hazards, a malformed missing-x test, and a late partial packet
  downgrading an already Closed inverse row after an unknown cancel. All fixed
  with scenario regressions; final account/trading reviews have no open findings.
- Binance full native suite passed 131 tests before the last Closed-row regression;
  that additional targeted regression also passes. Integrated totals follow.
- Synthetic private payloads now carry distinct native trade IDs and the
  documented complete cancel acknowledgement, rather than duplicate default
  IDs/status-only acknowledgements. No new gate normalization was introduced.
- Aster reuses the public Binance wire struct. Its legacy adapter initializes
  the two new optional fields without changing Aster behavior; 99 Aster native
  tests and strict Clippy pass. Its replay gates are included for compatibility.
- No crypto helper became unused: UUID and signing primitives remain called.

### Bybit M5 follow-up review checkpoint

- Root extended the reproducer to a REST read **started after** cancellation
  acknowledgement. It still reopened a removed Demo/Hyro order: revision-only
  fencing was insufficient against stale replicas. Captured failing test before
  adding bounded retention of the latest 4,096 terminal native IDs.
- Later open snapshot rows for retained IDs are filtered; unrelated active rows
  and terminal snapshot rows still commit. Reconnect retains the fence. Only
  explicitly final native wire statuses seed it: `Triggered` and `PendingCancel`
  cannot suppress a still-live order with the same native ID.
- Verification: 149 complete Bybit tests plus strict Clippy/format pass. A final
  independent Binance account author review found no open issue and reran all
  43 trading-variant tests successfully, including replica/reconnect/transition
  controls. This follow-up supersedes the earlier selected WASM artifact; all
  Bybit replay variants will be rerun on the final artifact.

### Focused TS reference correction review checkpoint

- Final native USD-M private replay exposed an inherited TS defect: symbol cancel
  downgraded the already filled ETH order from Closed to Canceled. The paired
  correction preserves existing Closed/Canceled rows when cancel replies arrive,
  retires all nonterminal local states, keeps accepted PM cancellations when a
  later request fails, and captures symbol-cancel targets before dispatch.
- A terminal-store opt-in was considered during review but rejected: canonical
  PM client-ID tombstones would newly prevent legitimate immediate client-ID
  reuse for 60 seconds. The final TS helper deliberately retains the existing
  status-update semantics. Native reconciliation fixes use native incarnations;
  TS still has its existing stale-snapshot limitation. No new normalizer.
- Root review required Creating/Updating/Canceling controls. Independent review
  additionally required deferred different-ID arrivals and immediate reused PM
  caller ID/new native incarnation controls. All 22 focused TS regressions pass,
  with independent rerun, TypeScript typecheck and scoped ESLint passing.
- Resolved requested audit bullets are struck in `RUST_AUDITS.md`; M10 is marked
  dropped after reverification, not claimed as a production fix.

### Landing integration and bulk-cancel review checkpoint

- Merged landing `1b8592dbd2` cleanly at `3bff1e155e`, preserving its shared
  engine feature/signing changes, Bybit decimal decoder, KuCoin Spot and all
  unrelated venue work. Merged-source root typecheck (44 tasks), lint (28),
  tests (27) and harness (586 tests) pass; strict native/WASM checks pass.
- Fresh independent merge review found a PM bulk-cancel incarnation race beyond
  the individual cancel cases: a delayed symbol-cancel acknowledgement could
  retire a newer regular/strategy order that reused the captured client ID.
  Each bulk family now captures native identities and applies success/unknown
  acknowledgement only to the matching current incarnation. Unknown native
  identities cannot create canonical client-ID tombstones.
- New regression matrix covers regular/strategy reuse, success/unknown replies,
  unrelated new arrivals and a later replica proving the new native order was
  not tombstoned. All 133 Binance tests and strict Clippy pass; independent
  reviewer reran 10 audit trading tests and closed the finding.
- Earlier selected replay gates all passed, including fresh Binance/Bybit live
  public captures. Shared-engine landing changes require final selected builds
  and complete family replay gates again; earlier timings are not final claims.

## Authorized addendum (second PR after this large patch lands)

User authorized splitting the new `Port reviews → Binance family` findings
because this first patch already exceeds 2,300 lines with regressions. Read the
new landing audit after fetch. PM amend `-2011` is already covered by this first
patch's USD-M/PM failure and racing-fill matrix. Reverify the remaining addendum
against the freshly merged base before implementing on a fresh follow-up branch:

- PM `fs:CM` rows must not create phantom positions.
- REST candles `from` + `limit`: compute start like TS and cap requests at 1,500.
- COIN-M partial batch rejection preserves placed IDs alongside the error.
- PM `close:true` respects an explicit nonzero amount.
- Unknown `CancelOrders` IDs fail instead of reporting success without I/O
  (now supplied by landing PR #1347; reverify, do not duplicate).
- History maps `EXPIRED_IN_MATCH` as canceled.
- Spot/COIN-M history reports unsupported rather than plain empty lists.
- Sign with `recvWindow` from options.
- Separate Spot delivery progress from quantified cancel acknowledgements, which
  can otherwise suppress the matching later TRADE (reverified inherited gap).
- Track the PM canonical row's observed native owner across regular/strategy
  families. A replay from a canceled strategy can still retire a newer regular
  order reusing its caller ID on the same symbol (inherited, PR review finding).

- Bybit Partial TP/SL drag flips SL/TP exactly as TS, per the explicit owner
  decision, with a regression reproducing the current in-place amendment.

Leave all other `Confirm`/owner-decision items untouched. Repeat phased independent
reviews, regression tests, all touched-family gates, PR reviews posted to the
PR, green CI and merge for the second PR; no TS venue deletion. Read-only
reverification has begun while this first patch completes its gates.

### Production-impact guidance and follow-up preparation

- Fetched and merged the new audit/guidance updates through landing `77291cd6cf`;
  these additional merges changed audit documentation only. Apply the owner's
  production-impact rule: no lifecycle matching for its own sake. Current fixes
  prevent extra exposure, lost/repeated fills, stale financial state or broken
  caller attribution. M10 is reverified and dropped, with its regression kept.
- Addendum read-only verification confirms the eight remaining requested issues.
  Partial COIN-M rejection should follow the established observable contract:
  emit/log the rejected rows and return accepted IDs, while stopping remaining
  bracket steps; malformed/count-mismatched replies still fail closed. There is
  no need to invent a new cross-host result envelope solely to mirror TS.
- Unsupported histories need native errors **and** existing SDK branded fallback
  methods, because downstream ingestion tests those brands. `recvWindow` needs
  SDK options → WASM options → native signing plus the primary-account lookup;
  the separate Spot WS default remains unchanged. Transport timeout lifecycle
  need not mimic Axios without demonstrated caller impact.
- The order-races lane (checkpoint `c2eb6aa8ca`, Aimux thread
  `thread-dmaqc0s1cvom`) owns engine order locks, pending-placement fences and
  snapshot empty-BOOK recovery. This lane additionally owns Binance/Bybit
  subscribed-book enumeration during resync, with missed-clear/fresh-recovery
  regressions. Include that public-data fix in the authorized follow-up PR and
  verify against their landed engine contract. Do not duplicate engine locks.
- Replied through the explicitly requested Aimux thread; requested the peer's
  PR/landing commit for integration. Native venues currently clear their local
  books on gaps, but filter those subscribed identities out of `books()`.

### Final native-incarnation review checkpoint

- Independent review found that a symbol-scoped native map alone cannot protect
  the globally keyed caller-ID row when a client ID moves from BTC to ETH.
  Cancellation now records the captured old native terminal evidence separately
  from updating the current row; successful/unknown and individual/bulk replies
  use the same current-symbol, native-ID and regular/strategy-family guard.
- Old PM terminal events arriving before the REST reply, strategy events,
  missing current rows and Spot late unquantified executions have regressions.
  Genuine executions remain observable without changing the newer foreign-symbol
  row. Ambiguous canonical-ID terminal fallbacks were removed.
- All 137 Binance native tests, strict Clippy and formatting pass. Independent
  review reran 13 audit trading and 19 lifecycle tests and closed every finding.
  Final selected Binance WASM rebuild/replays and combined-host checks follow;
  the completed final Bybit/Aster replay gates remain recorded in their logs.
- A final ordering control also rejects known old regular/strategy native rows
  in a REST read started after a foreign-symbol NEW but before the old cancel
  reply. Unseen native generations can still establish the next binding.
  Stream/snapshot guards use the current symbol: inferring native namespace
  from order type would incorrectly reject triggered STOP_MARKET executions.
  The original stop tests remain unchanged; all 138 Binance tests, strict
  Clippy and formatting pass after this narrower correction.
- Independent final review reran 14 audit trading, 9 execution-event and 19
  lifecycle tests (42 total), all passing, and closed the snapshot ordering
  finding. Merged landing PR #1342 (`fc809a9d85`) at `1611908137` after it landed:
  the shared public-feed/consumer path changes require fresh selected artifacts,
  all 23 family replay invocations and root/combined-host verification again.
  Read-only integration review confirms trading dispatch, account callbacks,
  fill emission and order-store paths are unchanged by that merge.

## Final merged-source gate evidence

Source: Bybit numeric decoder `1c65ceabf2`; Binance/AsterDEX source and
selected artifacts remain unchanged from `0cdb22aae8`. Landing documentation
`702a62050c` merged at `1174b85861` changes no runtime code. Documentation-only
checkpoints do not change these artifacts.
All 23 replay invocations pass: 11 documented/scripted private and 12 public.
Public figures are medians of three runs; pairs below are **TS / Rust**.

| Profile | Frames | Main-thread CPU ms | Steady CPU ms | Retained MB |
| --- | ---: | ---: | ---: | ---: |
| Binance USD-M | 873 | 261.3 / 133.6 | 147.9 / 65.0 | 18.2 / 4.0 |
| Binance COIN-M | 202 | 248.6 / 102.9 | 161.6 / 55.5 | 14.0 / 1.9 |
| Binance Spot | 7,045 | 935.5 / 645.7 | 470.4 / 274.6 | 27.6 / 10.8 |
| Binance PM | 873 | 261.5 / 136.0 | 153.8 / 58.5 | 18.0 / 4.1 |
| AsterDEX | 384 | 252.9 / 140.7 | 107.6 / 76.2 | 28.4 / 6.2 |
| Bybit linear | 4,295 | 340.7 / 225.3 | 164.2 / 91.4 | 22.2 / 4.9 |
| Bybit inverse | 708 | 198.5 / 56.5 | 129.9 / 24.3 | 14.4 / 1.6 |
| Bybit Spot | 4,339 | 305.3 / 225.7 | 165.1 / 99.7 | 16.5 / 3.1 |
| Bybit Demo | 5,833 | 454.0 / 310.6 | 262.1 / 143.1 | 21.9 / 5.1 |
| HyroTrader (scripted public) | 160 | 38.1 / 30.6 | 10.2 / 9.4 | 13.6 / 0.9 |
| Fresh Binance public capture | 285 | 459.8 / 161.1 | 290.0 / 86.1 | 18.5 / 4.1 |
| Fresh Bybit public capture | 19,946 | 795.6 / 483.5 | 454.0 / 213.7 | 22.6 / 5.3 |

All public outputs match under the existing documented TS corrections and
unchanged thresholds. Process CPU is informational and does not always improve
(e.g. Bybit linear 436.3 / 437.6 ms); no claim is made about every CPU metric or
peak memory. Private performance is informational, with one correctness run per
tape. Native regressions carry the adverse ordering cases outside those tapes.

Selected WASM artifacts are below the unchanged 1,258,291-byte limit:

| Family | Bytes | SHA-256 (Node) |
| --- | ---: | --- |
| Binance | 997,069 | `f55f6fd02fe287bfe23d8c71460fef69ba5487b51d489b15826a5654f28766b6` |
| AsterDEX | 1,094,705 | `04d842f43383813042293c1d371e37834c3a363589b0899fc39e25fc78424f03` |
| Bybit | 1,178,207 | `39bc44feba817e5c91e5f10aa2957fc2770ef92447dad2060adab6d2bee92d4a` |

- Gate 1: banned-pattern scan passes.
- Gate 2: workspace formatting and strict all-target Clippy pass. Native tests:
  engine 90, Binance 140, Bybit 162 and AsterDEX 99 (491 total).
- Gate 3: strict default-feature WASM Clippy passes.
- Gates 4–6: all selected release builds, private/public replay gates and
  Binance controls pass, including the fresh 20-second public captures.
- Root typecheck: 44 tasks; root lint: 28 tasks. All other 25 root test tasks
  pass. The complete SDK `test-ci` suite passes 7,777 tests across 526 files
  with `--maxWorkers=2`, including the same unit suite as `test-unit`. The
  preceding default-worker import timeouts are documented above; no assertions,
  timeout values or product code changed to accommodate host load.
- Harness: 590 tests pass. The final combined default-feature release build
  passes 444 SDK WASM tests across 16 files, 76 host tests, 16 product-semantic
  tests and 9 scripted live-smoke controls.

Raw logs: `/tmp/audit-bb-<profile>-{public,private}.log`, selected builds/sizes
`/tmp/audit-bb-{binance,aster,bybit}-{build,size}.log`, native/strict checks
`/tmp/audit-bb-final-*.log`, SDK rerun
`/tmp/audit-bb-merged-sdk-ci.log`, other root tasks
`/tmp/audit-bb-merged-root-other-tests.log` and harness
`/tmp/audit-bb-merged-harness.log`. Public tapes remain anonymous; private tapes are
scripted/documented, without live credentials or funded trading. Existing
committed public fixtures remain the CI regression oracle; the additional fresh
captures were also trimmed and gated separately without replacing that coverage.

### Final Spot cursor review correction

- Read-only baseline comparison confirmed that quantified Spot cancel ACKs
  already advance the execution cursor without emitting a fill at `bd94abb524`.
  The subsequent matching TRADE is suppressed. This inherited notification gap
  is now recorded in `RUST_AUDITS.md` and added to the authorized follow-up;
  this first patch claims derivative execution corrections, not complete Spot
  execution delivery.
- The new foreign-symbol historical-event branch introduced a related regression
  after an **unquantified** cancel: a non-TRADE CANCELED packet advanced the cursor
  before the genuine TRADE. The correction retains terminal proof without
  advancing delivery quantity on lifecycle packets, and delivers later TRADE
  progress independently of terminal flags. Four ACK-order/symbol permutations
  assert exact fill price/quantity once, preserved new row and stale-read rejection.
  All 139 Binance tests, strict Clippy and formatting pass; independent review
  reran 20 lifecycle plus 9 execution-event tests and closed the finding.
- The initial combined build passes 443 compiled SDK WASM tests, 76 host tests,
  16 product-semantic tests and 9 scripted live-smoke controls. The Spot-only
  correction is reverified by a new Binance selected artifact, all nine Binance
  replay reruns, strict WASM Clippy/bans and a fresh combined release build.
  All 443 compiled SDK tests, 76 host tests, 16 product-semantic tests and 9
  scripted live-smoke controls pass again on the final artifact. Bybit and
  AsterDEX protocol code remains unchanged; their replay evidence still applies.
- Independent gate-evidence review at `7a544a6bc4` confirms all 12 public table
  rows, three artifact sizes/hashes, 478 native tests and every SDK/host/harness
  count against the logs. It verifies 23 passing replay checks plus the final
  nine Binance reruns, explicit evidence limits and deferred follow-up scope;
  no correction or false-green claim found. Proceed to PR correctness reviews.

### PR #1348 correctness review round one

- Opened https://github.com/Tealstreet/tealstreet-sdk/pull/1348 into
  `feat/rust-rewrite` at `1a16e2196e`. Three fresh independent reviews cover
  account/executions, trading, and TS/harness/AsterDEX/SDK boundaries. Findings
  and evidence are posted as code-correctness PR comments.
- Boundary review passes: 22 focused TS tests, 62 compiled WASM boundary tests
  and one frame-wire integration. The 22-test total covers 19 new tests plus
  three existing symbol-cancel tests; no evidence-count correction is needed.
- Account review reproduces an inherited PM cross-family reuse gap: strategy42
  canceled, regular78 reuses its client ID/symbol, then old strategy42 CANCELED
  retires the regular row. Native type/source ownership is needed; order type
  alone cannot identify triggered STOP_MARKET wire ownership. Record this as an
  explicit follow-up limitation and preserve the owner's Confirm decisions.
  Original snapshot and captured-ACK fixes do not claim complete cross-family
  stream isolation. All 95 focused native review tests pass.
- Trading review finds an introduced Bybit Demo cancel-all regression: a valid
  ACK can return only 500 affected native IDs while the captured set also
  contains live conditional orders. Fencing every captured ID permanently hides
  omitted live exposure from later snapshots. Fix before merge by honoring
  returned IDs and preserving/reconciling omitted targets, with a real 500-plus-
  conditional regression. The reviewer ran 77 focused tests successfully.
- Bybit correction passes 156 native tests, strict Clippy, formatting and bans.
  It parses the documented affected list strictly, fences all returned native
  IDs including uncaptured arrivals, retires their projected children, and
  preserves omitted live targets with a visible partial-cancellation error.
  Missing/malformed lists and bulk unknown-order errors cannot fence all targets.
  Six new regressions include the actual 500-plus-one case (failed before fix),
  omitted/unrelated snapshot rows, uncaptured arrivals, terminal races and bad
  ACKs. Independent re-review reruns all 49 trading variants and closes P1.
- The first CI head passes its other completed checks but Hyro's small scripted
  public fixture misses steady CPU by 0.1 ms (Rust 5.8 / TS 5.7 ms). Thresholds
  remain unchanged. Rerun CI on the corrected head; never merge while red.
  Earlier Bybit replay/size evidence must be replaced after this correction;
  Binance and AsterDEX protocol behavior is unchanged.

### Latest landing and owner decision

- Merged landing `ec5e2e954b` cleanly after the first correctness reviews.
  Incoming unknown-ID CancelOrders validation already resolves that requested
  addendum item; reverify its regressions rather than implement it again.
  Incoming broker headers and harness signing-clock corrections remain intact.
- Sam explicitly decided the Bybit Partial TP/SL drag behavior: match TS's
  SL/TP flip exactly, with a regression, rather than amend the leg in place.
  This is authorized for the fresh-branch follow-up after PR #1348 merges.
  The updated Bybit audit section records the decision; other Confirm items
  remain untouched.
- The corrected affected-list proof exposed a Demo replay difference for an
  already Closed row omitted from the cancel-all result. Retaining/removing
  that terminal row is lifecycle cleanup, with no production impact. Align
  cleanup without marking omitted live orders terminal or hiding exposure.

- Independent review of merge `6854f75f3f` passes: native cancellation proofs,
  whole-list unknown-ID validation, signed/unsigned broker headers, symmetric
  signing-clock reset and prior audit closures coexist correctly. The reviewer
  independently ran 61 Binance and 78 Bybit focused tests (139 passing).

- Terminal cleanup now matches the local lifecycle on validated successful
  affected-list ACKs. Captured Closed/Canceled rows may be removed, without
  adding native terminal proof; omitted Open rows remain visible with a partial
  cancellation error. The regression failed before the cleanup and proves that
  cleanup does not invent tombstones or remove later uncaptured arrivals.
  All 159 Bybit native tests, strict Clippy/format/bans pass; independent review
  reran 50 trading variants and found no open correctness issue.
- Merged-source root typecheck (44 tasks), lint (28 tasks) and harness (590
  tests) pass. Selected artifacts and family replay evidence are being rebuilt
  after the landing merge and final cleanup; preceding measurements are historical.

### Corrected PR final replay checkpoint

- All 23 replay gates pass again on `0cdb22aae8`: 11 private and 12 public,
  including both fresh anonymous captures. The table and selected sizes above
  now reflect this source. No thresholds, tapes or output normalization changed
  to accommodate the earlier Hyro CI timing miss.
- Full merged native checks pass: engine 90, Binance 140, Bybit 159 and
  AsterDEX 99 (488). Strict native/WASM Clippy, workspace format and bans pass.
- Latest raw replay log: `/tmp/audit-bb-merged-final-gates.log`; native/check
  logs: `/tmp/audit-bb-merged-{native,native-clippy,wasm-clippy,fmt,bans}.log`.
  Combined WASM/SDK verification passes: 444 compiled SDK tests/16 files,
  host 76, product 16, smoke 9, and all 7,777 SDK CI tests/526 files. All other
  25 root test tasks pass. This supersedes the preceding historical counts.
- The final-source secret scan covers 16 own commits with zero leaks. Final
  evidence review is read-only; publish the corrected head and review closure,
  then wait for all CI checks before merging into `feat/rust-rewrite`.

- Independent final evidence audit verifies 23 PASS/zero FAIL, every public
  median against three TS and three Rust results, artifact sizes/hashes and
  488 native tests. No missing writes, unserved requests, invalid signatures
  or protocol errors appear in those public results. Its requested latest
  SDK/harness count and raw-log pointer corrections are applied above; no new
  correctness or false-green replay finding remains. The full SDK CI command
  includes the identical Vitest unit suite, so no duplicate unit rerun is needed.

### Repeated Hyro CI CPU gate correction

- Corrected head `febd6bf06c` passes 39 CI checks; Hyro's scripted public
  steady CPU remains red (Rust 6.4 ms / TS 5.5 ms), while output, total
  main-thread CPU and memory pass. The earlier head missed this same metric
  by 0.1 ms. Do not merge or change thresholds/fixtures to hide the failure.
- Public ticker frames now deserialize the eleven used numerical fields
  directly into a compact delta instead of retaining the larger REST record.
  The eight unused Greek/IV fields still validate string/null types and
  duplicates with zero-sized storage. REST/options records remain unchanged.
  Generic closure dispatch avoids a mapper function pointer in this hot path.
- Differential documented/sparse payload tests preserve bitwise values, empty
  versus whitespace semantics, negative zero, overflow/subnormals, escaped
  strings/duplicate keys, invalid common/Greek types and full-frame atomicity.
  Engine controls cover seven profile/category combinations, including inverse
  volume mapping and forced-zero spot funding. All 162 Bybit native tests pass;
  strengthened library tests pass 18/18, strict Clippy/format/bans pass.
- Independent final source review and an independent 18-test execution pass.
  The final generic-mapper change also passes author 18-test/strict checks and
  independent inspection. No engine, harness, fixture or threshold changes.
- The initial candidate passes the local Hyro replay, but its host load differs
  substantially from the baseline; no quantitative speedup is claimed. Final
  selected build and all Bybit profile gates are running before publication.
  Binance/AsterDEX source and their selected artifacts remain unchanged.

- Final numeric-decoder Bybit gates: all eleven PASS (five private, five public
  profiles plus the fresh public capture). Public medians/sizes above now include
  those reruns. Hyro steady CPU passes locally at Rust 9.4 / TS 10.2 ms; this
  is a gate result, not an isolated before/after speedup measurement. Binance/
  AsterDEX's twelve existing replay controls still apply to unchanged code.
  Raw log: `/tmp/audit-bb-bybit-numeric-final-gates.log`.

- Final exact-source Bybit suite passes 162 tests after the generic-mapper
  change (`/tmp/audit-bb-bybit-numeric-final-native.log`). Together with the
  unchanged engine/Binance/AsterDEX checks this is 491 native tests. Strict
  WASM Clippy/bans pass again (`/tmp/audit-bb-numeric-{wasm-clippy,bans}.log`).
  The rebuilt combined default artifact passes SDK WASM 444, host 76, product
  16 and smoke 9. Root typecheck/lint/other 25 tasks and SDK CI 7,777 remain
  valid for unchanged TypeScript/host source. Publish after evidence review;
  CI on the numeric-decoder head must still be green before merge.

- Independent numeric evidence audit confirms all six latest Bybit public rows
  against three runs per implementation, eleven passing replay invocations and
  46 result files without request/signature/protocol failures. It verifies the
  1,178,207-byte hash, final 162-test suite and fresh combined checks. Source
  comparison confirms no Binance/AsterDEX/engine/harness changes since their
  prior gates; the latest landing merge is docs-only. No correction remains.

### First PR landed

- PR #1348 merged into `feat/rust-rewrite` as `5f84b3e21d` after all 40 CI
  checks passed on reviewed head `74543cc5a7`. One failed Bybit CI job was
  rerun once at that identical head, transparently documented on the PR. Hyro's
  successful CI result is Rust 17.0 / TS 19.7 ms main-thread, Rust 2.5 / TS
  2.6 ms steady CPU and Rust 0.8 / TS 13.4 MB retained. Failed attempts remain
  visible. No thresholds, fixtures or framework changes were published.
- A temporary shared-host size heuristic did not improve the ordinary local
  gate reliably and was discarded before publication. Named-WASM profiling and
  forced-string diagnostics were diagnostic only, never acceptance evidence.
- Fresh follow-up branch starts at the merged landing (including PR #1349
  leverage-side changes). The remaining authorized scope and phase plan are
  in `RUST_AUDIT_BINANCE_BYBIT_FOLLOWUP.md`.
