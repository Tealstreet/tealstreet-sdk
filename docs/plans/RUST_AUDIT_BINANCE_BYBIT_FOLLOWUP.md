# Binance / Bybit audit follow-up

Branch: `feat/rust-audit-binance-bybit-followup`, fresh from landing
`5f84b3e21d` (PR #1348 merged after all 40 CI checks passed). The base also
includes leverage-side PR #1349; preserve its command fields and validation.

User-authorized scope: the remaining Binance port-review addendum, inherited
Spot quantified-cancel fill delivery and PM cross-family client-ID ownership,
Binance/Bybit subscribed books during resync, and Sam's explicit Bybit Partial
TP/SL drag decision. TS venues remain. Other Confirm items remain untouched.
Judge production impact, not incidental TS lifecycle differences.

## Phases

1. **Public data.** Enumerate subscribed Binance/Bybit books during resync so
   a snapshot can clear a consumer that missed the gap clear, then deliver a
   fresh book normally. Preserve sequence checks, category isolation and
   unsubscribe behavior. Coordinate with the order-races lane's engine contract;
   do not copy its engine changes. Binance REST candles compute the trailing
   start from end/interval/limit when a limit is supplied, cap at 1,500, preserve
   from-only/default behavior, and use real calendar subtraction for months.
   Tests cover documented book gap/recovery and exact candle request ranges,
   including month-end/leap dates and all four Binance profiles.
2. **Private PM.** Explicit CM account rows must not create PM positions.
   Missing/UM family rows and other variants retain their behavior. Track each
   canonical client's observed native source (regular/strategy), symbol and
   native ID. Bind only accepted stream/snapshot generations; old family
   terminal replays and delayed captured replies cannot mutate a newer owner.
   Cancellation routing must use that observed source, since a triggered
   regular order can have STOP_MARKET type. Preserve legitimate source handoff,
   primary/read-key policies, independent execution delivery and existing PM
   TRIGGERED semantics. Test both reuse directions, same/different symbols,
   fresh versus historical snapshots, triggered orders, cancels and late fills.
3. **Private Spot.** Separate authoritative cumulative execution from delivery
   state. A quantified cancel ACK updates sizing but cannot consume a genuine
   later TRADE notification. Seed a bootstrap floor only when an accepted REST
   snapshot first introduces the native identity; later reads/ACKs never raise
   an existing delivery floor. Native trade IDs dedupe in a bounded window;
   idless fallback uses delivery progress. Preserve late/reordered executions,
   native/client reincarnations, cross-symbol reuse and stale-read fences.
   Change the existing quantified-ACK zero-fill assertion to one fill and add
   bootstrap, malformed-first retry, duplicate, reordered and amendment controls.
4. **Trading.** COIN-M partial batch rejection retains accepted IDs on the
   visible error and stops later bracket steps. Reuse the existing typed JSON
   partial-error contract consumed by the host (`message`, `partialOrderIds`);
   populate optional index/count metadata only with correct caller semantics.
   Do not silently resolve a partially rejected placement. PM close:true honors
   explicit nonzero size; zero still means the whole position. Bybit Partial
   TP/SL follows the detailed contract below. Binance and Bybit file scopes
   allow bounded parallel authors, with independent correctness reviews.
5. **Queries/auth.** Map EXPIRED_IN_MATCH as canceled. Native Spot/COIN-M
   historical fills/orders/income explicitly reject unsupported reads without
   I/O; SDK methods retain matching branded Base capability fallbacks so
   ingestion distinguishes unsupported from a successful empty result. Preserve
   supported USD-M/PM histories, COIN-M public trades and primary-account lookup.
   Forward recvWindow through SDK options, selected-feature WASM options,
   native signing and direct primary lookup, retaining read-key routing,
   default 20,000 and the separate Spot WS 5,000 contract. Verify actual HMACs
   through RustEngineExchange, not only mocked constructor options.
6. **Gates and landing.** Bans, format, strict native/WASM Clippy, complete
   touched native suites, selected builds/size and public/private replay gates
   for every Binance/Bybit profile plus AsterDEX when shared wire changes.
   Root typecheck/lint/tests, compiled SDK/host/harness checks. Public live
   captures remain anonymous; private evidence is documented/scripted only.
   Push, PR into feat/rust-rewrite, post independent code-correctness reviews,
   fix/repeat, merge only on green CI at reviewed head, then a fresh branch.

At each phase: audit the concrete plan, implement with scenario regressions,
verify, independently review, fix and commit. No stash/rebase/force/master.
Unknown CancelOrders IDs are already fixed by landing PR #1347; reverify its
mixed-batch preflight test rather than implement the same fix again.

## Bybit Partial TP/SL contract

This owner decision is intentionally exact trading parity. For nonvirtual,
position-bound derivative Partial stops, derive the new SL/TP kind against mark
using the raw requested price before rounding: Sell above mark is TP, Sell at
or below mark is SL; Buy at or below mark is TP, Buy above mark is SL. Preserve
Buy/Sell: the flip changes SL/TP kind, not order side.

- Linear/Demo/Hyro (including combined inverse) replace with a usable mark for
  price or amount edits. Standalone inverse requires a price edit for this path.
- Cancel first through the existing native family, then call trading-stop
  directly; do not route through generic place planning, which may create a
  free conditional instead. Rejected/unknown cancellation errors cannot place
  a replacement; preserve existing already-gone semantics where explicit.
- Use the updated amount or original total amount, not remaining coin units.
  Explicit/rounded zero follows TS Full-mode conversion only in this branch.
- Same-kind trigger semantics retain the old leg's supported reference. A type
  flip uses the new leg's default (TP LastPrice, SL MarkPrice). IndexPrice maps
  to TS's defaults. Preserve TS replacement price serialization deliberately.
- One-way uses positionIdx 0; hedged replacement uses Sell 1 / Buy 2 as the
  closing leg. Preserve category, including native inverse units.
- Missing-mark fallback uses trading-stop, with the original kind and exact
  profile-specific fields. It must not become order/amend. Standalone inverse
  retains its distinct amount-only/fallback behavior.
- Preserve Full, virtual, trailing and free-floating stop behavior. Successful
  replacement needs authoritative order reconciliation; never locally resurrect
  the old canceled native ID merely to copy a TS lifecycle quirk.
- Regression matrix includes both directions/sides/equality, trigger defaults,
  explicit amount/zero, one-way/hedged/categories, missing mark, deferred/rejected
  cancellation, replacement errors, not-modified and stale snapshot controls.

## Planning evidence and progress

Read-only plans were prepared before this branch for PM ownership, Spot
execution delivery, Bybit Partial TP/SL and query/auth boundaries. Reverify
anchors against the fresh base before editing. The relevant files are
account.rs/wire.rs/trading.rs in Binance, trading.rs/account tests in Bybit,
Binance lib.rs/sign.rs/history.rs, WASM options and SDK capability construction.
No new error architecture or unrelated engine helper migration belongs here.

- Initial branch creation: clean worktree; read the latest brief/audit guidance.
- First-PR proof is in RUST_AUDIT_BINANCE_BYBIT.md; its experimental shared-host
  heuristic was discarded before publication. This branch inherits no host
  experiment or changed gate threshold. Its new work must pass its own gates.
- Public phase: Binance/Bybit snapshots now enumerate subscribed resync books;
  five new tests prove missed-clear recovery, fresh depth, refcount unsubscribe,
  cross-category isolation and all four Binance candle profiles. The Binance
  three cases and initial Bybit case failed before implementation. Candle
  ranges cap before subtraction, preserve omitted/zero options and subtract
  UTC calendar months once before clamping, including leap/year crossings.
  UTC is deliberate: browser-local Dayjs DST behavior is not claimed as a
  native parity guarantee. Independent public review passed. Engine snapshot
  contract changes remain owned by the order-races lane; no engine edits here.
- Private PM phase: explicit CM account events are ignored; canonical owners
  record observed regular/strategy source, symbol and native identity across
  stream, accepted snapshots and captured cancellations. Triggered regular
  stops use regular cancellation regardless of order type. Sixteen dedicated
  tests pass. Independent review reproduced malformed stream/snapshot economics
  stealing ownership; validation now precedes binding and malformed REST joins
  fail atomically with a visible log. All four review regressions pass, closure
  review passed, and strict Binance Clippy passed. A trade-first execution emits
  immediately but ownership waits for regular NEW/accepted snapshot; PM
  TRIGGERED semantics remain untouched.
- Bybit Partial-stop phase: cancel/recreate follows the owner-selected TS
  matrix, including raw price/type projection, leg trigger defaults, total
  native quantities, zero conversion, hedged/category routing and the distinct
  inverse/missing-mark fallbacks. Partial replacement alone requires strict
  matching cancellation proof. Successful shared-family writes reconcile orders
  even with a healthy socket, using Hyro's managed snapshot path. Full/virtual/
  trailing/free stops retain their controls. Full Bybit suite: 179 passed,
  including 15 new focused cases; strict Clippy/format/bans passed. Independent
  review and an independent 15-case rerun passed. Evidence is scripted, not a
  funded private-account capture.
- Binance trading phase: partial COIN-M failures retain proven native IDs in
  the existing host-readable error envelope and stop subsequent steps. Both
  mixed/short batch replies and failures after earlier successful steps are
  covered; optional caller counts/indices are omitted rather than guessed.
  PM explicit nonzero stop quantity survives close:true and does not cancel an
  existing full stop; zero still closes the full position. The initial three
  regressions failed before implementation. Independent review and all 26
  portfolio/variant trading tests pass. The additional actual SDK partial-error
  regression awaits the serialized Binance WASM build.
