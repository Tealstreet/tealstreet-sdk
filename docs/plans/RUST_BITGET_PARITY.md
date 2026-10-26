# Bitget family parity

Branch: `feat/rust-bitget-parity`, based on latest `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process: current `VENUE_PORT_BRIEF.md`.
Full automatic execution, phase-end independent reviews, stable-point commits,
PR into `feat/rust-rewrite`, adversarial findings posted to PR, merge only on
green final-head CI. No stash, rebase, force push or master changes.

## Phases and verification

1. Public/inverse foundations: add coin-futures product and registration;
   preserve TS native USD placement quantities and documented display conversion.
   Verify contracts, precision, quote filtering, tickers/books/candles and routing.
   Audit shared model/frame support for collateral/native quantities before edits.
2. Private/inverse: per-coin position reads committed atomically; multi-coin
   balance valuation, settings and WS reconciliation; inverse order display and
   native quantities. Failed reads preserve authoritative state. Verify documented
   payloads, both coins, REST/WS races and empty/error responses; independent review.
3. Trading: per-symbol collateral throughout inverse writes; Classic and UTA
   trailing lifecycle using documented request/response shapes. Preserve immutable
   loss_plan/pos_loss identity, exact native sizing, fresh modify client IDs,
   reads-only retries and ambiguous-write safety. Independent review and tests.
4. Queries: UTA ledger bills with typed streamed records, signed fees, pagination,
   bounded time windows and explicit failure on incomplete reads. Retain shared
   history JSON shapes. Independent review and tests from official docs.
5. Harness/gates: inverse, UTA bills and trailing documented private-tape cases;
   exact outbound requests and command results. Register any demonstrated TS bugs
   explicitly; never weaken correctness/performance checks. Bans/fmt/clippy/native
   tests, wasm clippy/build, Bitget and Binance public/private gates, root quality
   checks. Independent phase and adversarial PR audits, fix and repeat.

## Plan audit

- Current inverse TS uses native USD order amount/filled, converts only remaining
  at order price and positions at ticker mark/last/entry to 8 decimal places.
  Do not apply a blanket base/quote conversion to writes.
- Inverse UTA stays gated as TS; Classic per-symbol collateral must support ETH
  as well as BTC. Existing engine representation needs audit before claiming parity.
- UTA TS currently rejects bills/trailing. New documented engine support needs
  explicit independent tape assertions as well as preserved existing TS oracle.
- Trailing callback units and v2/v3 payload fields must be checked against official
  Bitget docs, not guessed from the existing unsupported branches.
- Sub-agent work is restricted to disjoint files; parent owns model/host/harness
  integration and commits. All implementations receive an independent audit.

## Progress

- Fetched landing branch, read current brief/spec and Bitget guidance; clean fresh
  branch created. Inverse audit and documented ledger/trailing implementation
  research are running in separate bounded scopes.

## Public checkpoint

- Inverse registered through wasm, Rust SDK and harness, with coin/scoin product,
  USD filtering, inverse flag, native lot precision and TS maxTradeNum precedence.
- Independent public audit found zero leverage fallback mismatch; corrected to
  TS defaults 1/20. Documented-schema regression covers both product environments,
  two markets, wrong quote filtering, precision and unchanged book quantities.
- Public native tests: 16/16 PASS. Private/trading/query work continues separately.

## Private checkpoint

- Inverse snapshots read each collateral coin and commit positions atomically;
  preserve native amounts/filled, round display remaining/positions to 8 decimals,
  keep native notional/PnL, and value per-coin collateral into aggregate USD.
- Optional collateral columns cross the real transferable frame. Both worker
  consumers retain coin balances and inverse native extras from existing columns.
- UTA trailing fetch/read lifecycle added beside TPSL; native callback metadata
  stages until successful fresh snapshots. Classic-only inverse never promotes UTA.
- Independent review found duplicate collateral double-counting and fractional
  native-extra reconstruction; fixed with atomic duplicate rejection and decimal
  subtraction. Native private cases cover failed/late reads, two coins, snapshots,
  invalid data, native fills and shared stream/REST consistency.
- Verified engine48 tests, private account15 + inverse12 + review3 tests, Worker/wire
  26 tests, TS frame4 tests PASS. Native amend regression reserved for trading.
- TS defects explicitly listed in gate notes: inverse WS conversion/aggregate
  balance, native stop clamp and sized amend. No parity output normalization.

## Trading checkpoint

- All Classic inverse writes resolve collateral by symbol, preserve native
  quantities, clamp protective sizes against native position notional and retain
  fresh native modify client IDs. Direct and resolved amend acknowledgements both
  restore display remaining and native metadata.
- Classic trailing uses documented plan modify/cancel payloads; UTA uses strategy
  placement, distance parameters and no unsupported client ID. Amend and known
  rejection rollback retain callback settings, trigger basis and activation price.
  Ambiguous writes never retry or recreate protection.
- Independent trading review found callback drift and stale REST cache overwrite;
  corrected with staged native metadata, freshness guards and exact preservation.
  A second review found UTA activation basis loss; now parsed from native params.
- Native trading20 tests plus inverse amend regression and strict clippy PASS.
  Intentional TS fixes are listed in gate notes, including invalid callback
  preflight, documented Classic hedge side and native trailing amend/cancel.

## Queries checkpoint

- UTA income uses financial-records with streamed documented rows, bounded 30-day
  windows, cursor walking, exact IDs, product/symbol filters and atomic failure.
- Preserve raw signed amount and optional signed fee metadata. Funding, explicit
  fee and transfer enums are classified; trade remains other because docs do not
  establish gross/net PnL. No additive fee duplication or invented decomposition.
- Inverse missing fee currency resolves each symbol's base coin, including ETH.
- Independent query review clear. Existing history13 + new UTA ledger10 tests and
  strict clippy PASS; tests include byte-wise streamed decoding, late UTA signal,
  malformed lists, failed later pages, repeated cursors and nonoverlapping windows.

## Final boundary review

- Independent review found stale venue timestamps can make HTTP acknowledgements
  overwrite a later WS callback. Captured monotonic WS revisions now protect
  same-clock arrivals as well as old exchange timestamps; regression passes.
- Real Worker hydration now retains the trailing SL trigger basis. Native replay
  already exposed that field, so an adapter regression pins the production path.
- External limit/index trailing configurations remain visible/cancelable, and
  unsupported edits fail before cancel. Supported market-triggered trailing paths
  retain their exact native parameters across size edits and rollback.
- Native trailing21 + existing trading25 PASS, inverse amend direct/resolved paths
  PASS, strict clippy PASS. Flat ETH metadata test clears positions first and
  verifies ETH collateral in metadata, leverage, margin and cancellation.

## Harness checkpoint

- Independent inverse overlap tape compares two coin balances, native/display
  quantities, exact per-coin writes, fresh amend identity and native fills with
  TS: PASS, 30 records / 13 WS frames, 2 markets/tickers. No normalization.
- New native-contract integration tests pin UTA USDT/USDC ledger pagination and
  raw signed fees, trailing REST/WS reads, placement, callback preservation,
  known-rejection rollback, cancellation and duplicate execution handling. A
  Classic tape separately pins native modify/cancel and percentage preservation.
  Four independent cases PASS, including a tampered distance rejected before
  write consumption. Exact signatures/protocol/write-consumption checks retained.
- Fresh USDT public recording: 20 seconds / 150,439 WS frames / two books; trimmed
  first six seconds replaces the existing public fixture (39,457 replay frames).
  Preliminary public gate PASS, TS/Rust CPU598.0/547.5ms, steady396.3/376.2ms,
  retained6.0/2.5MB. Final source/artifact reruns remain required below.
- Live inverse recording attempt returned an empty contracts list and rejected
  BTCUSD/ETHUSD book subscriptions. Do not count it as a public gate or commit an
  empty fixture. Inverse verification uses explicitly documented-schema native
  tests and the private overlap tape; USDT public gates cover shared hot paths.
- Root typecheck44, lint28 and test27 tasks PASS. Harness113 and Worker/wire26
  tests PASS; native workspace and final WASM/CI checks are running.

## Landing integration

- Integrated Hyperliquid landing5c47683969 under standing merge authorization.
  Kept incoming optional venue features, 11-venue registrations, single-venue
  build/matrix and unchanged1,258,291-byte per-venue browser budget.
- Kept incoming shared strict request/write-sequence transport. Inverse batch
  fixtures explicitly declare TS cross versus native crossed; no global spelling
  normalization. Retained direct collateral/native-extra comparisons and native
  integration signature/protocol counters under the new Tape method API.
- Incoming measurement model samples after runtime/host construction, validates
  absolute counters and net arithmetic, permits signed component deltas and waits
  for backing-buffer stability. Retained that landing model; strict CPU/steady
  CPU/total-memory comparisons remain. Pre-merge numbers above are not final.
- Independent registration/feature and Bitget engine/query merge audits clear;
  nativeBitget123, sharedharness147 and nativecontractintegration4 tests PASS.
  Final rebuilt per-venue measurements and final-head CI remain required.

## Documented trailing lifecycle checkpoint

- Primary UTA strategy WS schema documents TPSL/trigger only. Replaced the UTA
  replay's inferred trailing pushes with staged documented REST snapshots; kept
  adapted full trailing frames only as explicitly labeled defensive race tests.
- UTA writes trigger full snapshots, coalescing writes that arrive during a read;
  eight-second polls reconcile full UTA strategy lists. Absent trailing orders
  expire after fresh-stream grace while Classic/TPSL preservation remains intact.
- Captured order revisions protect rows, callbacks and absence decisions even
  when an old HTTP response arrives after the five-second grace has expired.
- Official REST read mode spread maps to the placement API's distance enum.
  Preserved ratio mode and activation basis. Deduplicated write notifications
  so a single HTTP acknowledgement starts one reconciliation, retaining partial
  accepted-write handling and excluding identity GETs from write notifications.
- Exact published TPSL push derives closing direction from its protected side;
  documented long protection stays long through amendment. TS direction defect
  is explicitly registered in KNOWN_TS_FIXES.
- Inverse startup now performs the same eligibility probe as TS; Classic40084
  routes coin futures, while inverse UTA success fails with scoped guidance.
  This preserves the strict landing request oracle without request suppression.
- Resumed native suite, formatting and strict clippy PASS. Repository test suite
  PASS27 tasks with concurrency1; an earlier parallel cold-import timeout was
  isolated and resolved without changing test timeouts or product code.
- Final native-contract replays, rebuilt per-venue gates and PR/CI still required.

## Final local gates

- Latest landing c5bc2c4adf merged; phase-end independent review clear, including
  delayed snapshot revisions and exact published TPSL close direction.
- Bitget136 native tests, harness147, native-contract integrations4, WASM adapter25
  and Worker/wire26 tests PASS. Bans, workspace fmt, strict native/WASM clippy,
  repository typecheck44, lint28 and tests27 tasks PASS.
- Rebuilt Bitget-only WASM926,001bytes < unchanged1,258,291byte budget. Rebuilt
  combined host and its complete adapter/Worker checks PASS as well.
- Fresh Bitget public PASS:39,457frames,846markets/tickers,2books. TS/Rust CPU
  646.8/594.0ms; steady432.7/395.1ms; retained5.8/2.5MB; Rustlinear1.2MB.
- Bitget Classic private and inverse private strict parity PASS; inverse13frames,
  2markets/tickers, exact collateral/native metadata and request identities.
- Binance public PASS:873frames,917markets,783tickers,3books. TS/Rust CPU
  258.3/118.3ms; steady160.1/49.7ms; retained4.2/2.2MB; linear1.5MB.
  Binance private PASS. Private CPU/memory measurements remain informational.
- Next: push, PR into feat/rust-rewrite, post adversarial independent reviews,
  fix findings, require all final-head CI green, merge and create follow-up branch.

## PR adversarial review, round 1

- PR1317 targets feat/rust-rewrite. Three independent adversarial reviewers
  reproduced five defects against the real engine; findings posted on the PR.
- Private fixes: decode documented single-mode WS equity without duplicate-field
  aliases; pin position reconciliation guards for the entire serial collateral
  walk, including delays beyond ten seconds and zero-size closure updates.
- Trading fixes: distinct replacement identity uses requested amend price/size
  despite a newer cancellation of the old identity; preserve an actual newer
  replacement row. External opening trailing remains visible/cancelable and must
  reject unsupported editing before cancellation changes exposure.
- Query fix: explicit historical fee coin bypasses current-market fallback.
  Delisted-row handling must not fail unrelated fills. TS drops unknown markets;
  this is not a claim of TS retaining delisted history.
- Focused regressions, independent second-round reviews, final artifact gates
  and final-head green CI remain required. Earlier local numbers are provisional
  until these changes pass their affected gates.

## PR adversarial review, round 2

- Independent reviewers cleared equity, delayed position reconciliation and lazy
  fee-asset changes, with46 focused tests passing.
- A further confirmed amendment race left the source ID open when the replacement
  became terminal before the ACK. Retire the source before checking replacement
  terminal state; never reopen the replacement. Direct/resolved ACKs, fill/cancel,
  and previously-visible/unknown replacement identities receive regression cases.
- Defensive one-way trailing rows lacking an unambiguous protected side must
  reject editing unless explicit native reduce-only intent is present. Preserve
  visibility/cancellation. Side-only placement is documented; omission of posSide
  in native GET responses is not established, so this is a defensive schema case,
  not claimed live exchange evidence.

## Transport and Bitrue landing integration

- Committed adversarial fixes0ea27d2e13; focused trading64 and independent
  boundary6/private+history46 checks pass. Round2 reviewer cleared all fixes.
- Landing advanced to fc18c55874 with transport PR1313 and Bitrue PR1314.
  Merged as a9f4416f81. Kept incoming socket roles/feed recovery, twelve-venue
  build/CI roster and workspace structure. Resolved append-only script and public
  test conflicts by keeping both additions.
- Independent engine/account and harness/host integration audits are running.
  Final native/WASM, repository and replay gates are being rerun on this merge.
- Earlier root tests hit unrelated Hyrotrader dynamic-import timeouts under Rust
  compilation; no product/test timeout changes. Verify again with bounded test
  concurrency before claiming the repository gate green.
- CI exposed an incoming transport/Bitrue test integration omission: one Bitrue
  WsConnect match did not ignore new channel/context fields. Added the structural
  wildcard; Bitrue account15 tests pass, with full workspace clippy rerunning.
- Shared-machine load reached148 on10 logical CPUs during local TS import
  timeouts. Do not weaken test timeouts or claim a local root-test pass; final
  complete CI remains required and is the repository test source of truth.

## Reviewed artifact gate proof

- Source704f836c78: independent adversarial and transport-merge reviews clear.
  Bitget147 native, harness153, dedicated UTA/Classic integrations4, combined
  WASM adapter29 and Worker/wire35 tests PASS. Workspace native tests, strict
  clippy/fmt, bans and WASM checks PASS in CI; typecheck44/lint28 pass locally.
- Final rebuilt CI Bitget public PASS:39,457frames,846markets/tickers,2books.
  TS/Rust main CPU861.3/776.0ms, steady522.1/476.9ms, process1187.3/1138.4ms,
  retained5.8/2.5MB, Rustlinear1.2MB. CI WASM928,011bytes, local928,001bytes;
  both below unchanged1,258,291byte per-venue budget.
- CI Classic private12frames and inverse private13frames PASS with strict
  requests, native/display metadata, two inverse collateral coins and command
  results. Private performance remains informational. Native UTA USDT/USDC
  ledger/trailing and Classic trailing integrations4 PASS.
- Final rebuilt CI Binance public/private PASS. Public873frames,917markets,
  783tickers,3books; TS/Rust main CPU154.2/72.4ms, steady80.8/24.7ms,
  retained4.2/2.2MB, Rustlinear1.5MB. Combined host built before29/35 tests.
- Live inverse contracts remain unavailable in the recording attempt; no empty
  public tape counted. External unsupported opening/ambiguous/limit/index UTA
  trailing remains visible/cancelable and fails editing before cancellation.
- Final documentation head must still receive complete green CI before merge.
  Posted PR reviews and the eventual merge record are the landing proof; no
  test timeout, replay assertion, performance threshold or budget was relaxed.
