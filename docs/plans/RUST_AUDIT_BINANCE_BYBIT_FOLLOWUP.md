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
- Queries/auth source checkpoint: history EXPIRED_IN_MATCH maps canceled.
  Native Spot/COIN-M fills/orders/income reject unsupported reads before auth
  or I/O; actual SDK constructors brand each matching Base fallback while
  USD-M/PM methods remain supported. All four aliases forward recvWindow to
  native ordinary/direct-primary signing, preserving 0/fractions/default 20,000
  and secondary-read routing; Spot WS remains 5,000. Intended native query and
  SDK branding regressions failed before implementation. An exact old-checkpoint
  signer probe fails custom 4,321, and current source passes. Independent review
  reran 18 native plus four actual-constructor tests successfully; author strict
  Clippy/typecheck/ESLint passed. Eight actual selected-WASM HMAC scenarios are
  prepared but await the root's serialized build. An earlier wrong-selected-
  package 'unsupported venue' attempt is explicitly not regression evidence.
- First pre-integration Bybit gate attempt: selected artifact 1,195,457 bytes,
  SHA256 57f1c7141f387a96cfa2954b84743d28c0c42a6b50addea69604b707a38b3473.
  Nine replay gates passed; Hyro public failed steady CPU (TS 4.8 ms / Rust
  5.2 ms, main 23.3 / 22.8 ms). The chain stopped before the fresh capture gate.
  This is not final passing evidence. Landing PR #1345 then arrived with shared
  engine/wire/frame changes; merge it and rerun the final source gates with
  unchanged thresholds. Do not disguise the miss as a passing run.
- Private Spot checkpoint: authoritative execution totals remain separate from
  delivery. Quantified ACKs size replacements without consuming later TRADEs;
  native trade IDs dedupe in a bounded 16,384-entry window independently of
  terminal state/client reuse. Only the first fully accepted REST observation
  seeds an immutable history floor; later reads and ACKs never raise it, and
  stream-first orders retain zero floor. Sixteen new native cases plus 20
  lifecycle controls pass, including bootstrap/rejected joins, malformed retry,
  reordered IDs, current/historical/symbol reuse, reconnect, bounded eviction
  and flat/nested OCO ACK races. Core/strengthened historical regressions and a
  malformed-execution regression failed before their fixes. Independent source
  review and final 16-case rerun passed. The new two-case real SDK fixture was
  source-reviewed; compiled proof and full native/Clippy remain pending after
  the incoming wire merge. No claim of unbounded or mixed-idless exactly-once
  delivery, historical backfill or funded private verification is made.
- Landing PR #1345 merged cleanly at 678177c203. Independent integration review
  confirmed receive-window options and history brands survived the SDK merge.
  Its fill bridge now exposes both id/orderId, so the new Spot worker/main
  fixture explicitly expects both native identity fields. Full checks and all
  replay gates must now certify this integrated source, replacing earlier
  pre-integration performance evidence.
- Integrated-source checks: 561 native tests passed (engine 102, Binance 181,
  Bybit 179, AsterDEX 99); bans, scoped format, strict native all-target Clippy,
  default WASM Clippy and Binance-only WASM Clippy passed. Root typecheck 44,
  lint 28 and other-package tests 25 passed; SDK CI passed 7,844 tests/530 files.
  The selected Binance artifact is 1,014,611 bytes. Actual compiled SDK proof
  passed: six primary/secondary USD-M/PM HMAC cases, eight variant controls
  including Spot/COIN-M HMAC and partial-error IDs, and two Spot worker/main
  delivery cases (16 tests total). An initial broad multi-venue SDK selection
  against the Binance-only artifact produced unsupported-venue failures and
  is not a valid gate; selectors were corrected. The partial-error test also
  now expects BaseExchange's existing 'Exchange Error:' event prefix. Combined
  SDK/host suites and final replay gates remain to run with unchanged policy.

## Integrated replay evidence

At initial PR head 23a106668a, all 23 venue replay gates pass: 11
documented/scripted private replays and 12
public replays (three runs each), including two fresh anonymous 20-second
captures. No executable oracle, threshold or committed fixture changed.
Combined verification passed: 455 compiled SDK tests/17 files, host 89,
harness 590, product semantics 16, scripted lifecycle 9 and frame-wire
integration 7. The final fetch still points to merged landing c44f22bd8e.

Public medians below are TS / Rust; CPU is ms and retained memory is MB.

| Replay | Frames | Main CPU | Steady CPU | Retained memory |
| --- | ---: | ---: | ---: | ---: |
| Binance USD-M | 873 | 190.4 / 85.7 | 102.2 / 33.1 | 18.2 / 4.1 |
| Binance COIN-M | 202 | 172.8 / 68.8 | 113.5 / 38.1 | 14.0 / 1.9 |
| Binance Spot | 7,045 | 637.4 / 423.7 | 280.1 / 142.4 | 27.6 / 11.2 |
| Binance PM | 873 | 227.9 / 94.6 | 110.9 / 37.5 | 18.0 / 4.1 |
| AsterDEX | 384 | 228.9 / 119.6 | 98.6 / 57.0 | 28.5 / 6.3 |
| Bybit linear | 4,295 | 264.8 / 170.3 | 127.8 / 48.3 | 22.3 / 5.1 |
| Bybit inverse | 708 | 120.4 / 40.6 | 74.7 / 16.5 | 14.5 / 1.7 |
| Bybit Spot | 4,339 | 252.1 / 146.9 | 159.7 / 67.5 | 16.5 / 3.2 |
| Bybit Demo | 5,833 | 452.4 / 305.4 | 268.0 / 150.5 | 22.0 / 5.2 |
| HyroTrader (scripted public) | 160 | 34.9 / 30.9 | 11.3 / 8.7 | 13.6 / 0.9 |
| Fresh Binance capture | 213 | 499.9 / 179.8 | 364.1 / 98.4 | 19.1 / 5.0 |
| Fresh Bybit capture | 46,956 | 2070.1 / 849.7 | 1410.2 / 474.6 | 23.2 / 5.9 |

Selected WASM bytes (fixed budget 1,258,291): Binance 1,014,611; AsterDEX
1,099,765; Bybit 1,196,557. SHA256 respectively:

- Binance: `5dc0f3ec39d28603aab62d85ff42746e0f9061154b03dd8d0555f600e72aedf9`.
- AsterDEX: `4fdb57a1390afaf71c2ea907fc525c56cb09cfcc567f11390ff7654ce0bf957b`.
- Bybit: `977a812d30670e180d0bc9358215d9ffd5e0212a91e1dfe2eaf15546abe5155c`.

This evidence certifies the initial integrated runtime at 23a106668a. The PR
review closure below changes Bybit and requires fresh affected-runtime evidence.
Earlier pre-integration failures above remain recorded.

Independent gate review passed at e6390fdb06: all 94 replay output files have
the expected run counts and zero unserved requests, missing/unused writes,
invalid signatures or protocol errors. Artifact sizes/hashes, test counts and
unchanged executable gate policy were independently verified.

## Deferred assignment for handoff (wind-down)

Owner subsequently instructed this lane to finish the current PR, merge only
on green CI, write the remaining assignment to the requested handoff and stop.
The next Bybit assignment below is unstarted and must not begin during wind-down.

Owner confirmed the Partial cancel/recreate decision against TS, including
the missing-mark path. Before wind-down, the next assignment was a fresh branch
from landing for the Bybit highs and mediums in RUST_AUDITS.md: Brazil routing,
per-coin balances, funding/settlement rows in fills, hedge funding dedupe, then
the medium findings excluding the settled Partial TP/SL item. Reverify current
source and judge production impact; do not fold that new scope into this PR.
Coordination: thread-dmam5utc39vm, owner reply received 2026-10-20.

## PR correctness review closure

Three independent reviews covered Binance, Bybit and SDK/WASM/public/query
boundaries and were posted on PR #1354. Binance review reran 78 native tests;
boundary review reran 26. No introduced P1/P2 found in those scopes.

Bybit review found a P2 in the new forced Partial replacement refresh: ordinary
Linear and combined inverse could reopen the canceled native ID when a replica
returned its old Untriggered row alongside the actual replacement. Two tests
reproduced both failures before the fix. A third regression independently
proved the analogous delayed private Untriggered publication failure.

Accepted/already-gone Partial cancels now retain the old native ID in the
existing bounded terminal cache on all applicable profiles. Ordinary cancel
cache policy is unchanged. Snapshot reconciliation and private open publication
reject those identities; private terminal updates still apply. Hyro cumulative
fill processing runs before the publication fence; other execution topics are
independent. The new stream test covers all five profiles and proves late
Closed/filled state updates, without claiming emitted-FILL proof from that
assertion. Two independent source reviews passed the snapshot and stream fix.
Affected native verification passes: 564 tests (engine 102, Binance 181, Bybit
182, AsterDEX 99), bans, format, strict native all-target Clippy and default /
Binance-only WASM Clippy. Latest landing PR #1352 merged cleanly at 0eeb54ad80;
its shared listener isolation is independently source-reviewed for our
fill/error contracts. Integrated JS checks pass: typecheck 44, lint 28,
other-package tests 25 and SDK unit 7,851 tests / 531 files. Network/teardown
warnings did not fail the SDK suite. All 40 CI checks at 0eeb54ad80 pass.
Final integration replay and compiled-suite evidence is posted on PR #1354
before merge; no executable gate policy is changed. Bybit selected WASM is
1,196,681 bytes, SHA256
`c6fe81237533f92c60b1aec776056c9a14ae7a40a0ccbc629e292698236c8105`.

Independent Bybit evidence review at 0eeb54ad80 passes all 11 replay gates and
46 actual JSON outputs: exact run counts, matching frames and zero unserved
requests, missing/unused writes, invalid signatures or protocol errors. Build,
hash and replay timestamps prove the reviewed selected artifact was used
throughout, before the next Binance build. The final fresh 20-second Bybit
capture has 12,782 replay frames: CPU TS/Rust 687.1/401.6 ms main and
458.1/215.0 ms steady; retained memory 22.6/5.2 MB. Hyro public steady CPU is
10.2/4.8 ms with unchanged thresholds. No private funded verification is claimed.

Boundary review caveat: the resync tests prove explicit full-snapshot empty
BOOK delivery and subsequent recovery. They do not prove an ordinary gap clear
was emitted: Binance on_depth and Bybit on_book clear their state and return
before book_changed on the gap path, also in the base. Comments now accurately
say pending incremental frames are discarded. This preexisting ordinary clear
gap remains in the handoff; no claim that it was fixed by this PR.

Initial PR CI at 23a106668a also missed two performance gates: Hyro steady CPU
TS 5.6 / Rust 5.7 ms and untouched Paradex main CPU TS 297.8 / Rust 311.5 ms.
No correctness mismatch or threshold change. These failed attempts are not
passing evidence; the final head must pass CI before merge.
