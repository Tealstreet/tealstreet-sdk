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
   Propagate client-ID RNG failure before writes. Test the actual command/event
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
  generations have separate native identity maps; old replies/events cannot
  mutate a newer order reusing a caller client ID. Old native generations are
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
- Unknown `CancelOrders` IDs fail instead of reporting success without I/O.
- History maps `EXPIRED_IN_MATCH` as canceled.
- Spot/COIN-M history reports unsupported rather than plain empty lists.
- Sign with `recvWindow` from options.

Leave every `Confirm`/owner-decision item untouched. Repeat phased independent
reviews, regression tests, all touched-family gates, PR reviews posted to the
PR, green CI and merge for the second PR; no TS venue deletion. Read-only
reverification has begun while this first patch completes its gates.
