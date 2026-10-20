# Rust parity sweep, part 2

Branch: `feat/rust-parity-sweep`, from `origin/feat/rust-rewrite`.

Scope: audit every PR #1298–#1324 with `gh pr view`, cross-check the
`RUST_REWRITE.md` roster against current TypeScript, and close supported
capability gaps. Phemex income and definitive client-ID lookup are explicit
targets; extend history beyond a page only where TS supports that today.
The other lane owns Paradex remote signing, Bitrue history/client-ID/hedge/
margin mode, and KuCoin spot. Do not edit those surfaces. Keep the existing
1,258,291-byte per-venue WASM budget.

## Phases and verification

1. Discovery/public data: read the brief/spec and merged PR limitations;
   independently audit TS capability evidence and classify gaps by venue.
   Record specific public/product gaps before implementing them.
2. Private data: close supported account/settings/signing gaps identified by
   discovery, preserving failed-read and stale-snapshot safety.
3. Trading: close supported order/conditional/mode gaps identified by the
   audit, preserving caller-ID preflight and one-write semantics.
4. Queries: implement Phemex documented real-value income using shared
   `paging::RangeWalk` and a direct closed-inclusive client-ID endpoint;
   close TS-supported history/lookup gaps in other merged venues. Refuse
   incomplete/ambiguous results rather than claiming an authoritative miss.
5. Harness/gates: deterministic regressions from documented contracts;
   public/private gates for Binance and every touched venue, bans/fmt/clippy,
   affected native/SDK tests, typecheck/lint, combined host/WASM checks, all CI.
6. Landing: merge latest landing branch, push, PR into `feat/rust-rewrite`,
   post adversarial sub-agent reviews, fix/review to convergence, merge only
   on green exact-head CI, cut a fresh follow-up branch.

Every phase has a sub-agent implementation review and a stable commit.
Full auto: no human checkpoint, no stash/rebase/force push/master changes.
The final PR lists remaining TS-unsupported and live-key-only capabilities
with evidence, plus each gate's counts and CPU/memory/artifact measurements.

## Progress

- Fresh branch created from fetched landing; clean starting worktree.
- Discovery running: independent PR audits #1298–#1310 and #1311–#1324,
  plus independent roster/capability audit. Primary lane inspects Phemex.
- Confirmed Phemex TS income is a base fallback; fills are symbol-scoped,
  latest-page only (limit capped at 200); generic order history is a base
  fallback. Do not advertise unbounded TS history that does not exist.
- Phemex official docs describe real `amountRv`/`balanceRv` ledger rows and
  `/api-data/g-futures/orders/by-order-id` with `clOrdID` for closed-inclusive
  lookup. Implement these explicit user targets without guessing scaled
  numeric execution enums or relabeling derived PnL as venue-reported data.
- Initial cross-review found Binance USD-M and OKX TS definitive lookup
  overrides absent from Rust. Detailed gap census and phase scopes pending
  completion of the independent discovery audits.

### Discovery review accepted

- All 27 PRs inspected using `gh pr view`; 24 merged into landing. #1309
  (Bitunix), #1321 (GRVT), and #1315 (master/CLI) remain unmerged and excluded.
- Three independent reviews are recorded in `RUST_PARITY_SWEEP_AUDIT.md`.
- Supported declared gaps: OKX inverse swaps and EEA X-Perps (#1302),
  generic secondary GET credentials/egress (#1313). Also close the concrete
  Binance/OKX native client-ID lookup regressions found by roster review.
- Phemex explicit requested additions use documented reported ledger amounts
  unchanged and direct closed-inclusive ID lookup. TS does not support
  unbounded Phemex fills/order history, so keep its bounded scope explicit.
- Other declared gaps are already fixed by later PRs, TS-unsupported, other
  lane ownership, safety boundaries, live verification, or phase 9 adoption.
- Undisclosed broader API differences found during audit are retained as
  evidence in the census; these are not mislabeled TS-unsupported/live-only.
- Implementation divided into disjoint query, OKX product/account/trading,
  and secondary-signer scopes. Primary owns Phemex, wrapper, harness and docs.

### Public/private/trading/query implementation checkpoint

- Independent phase reviews accepted Phemex income+lookup, Binance/OKX
  lookup, OKX public/private/trading products, and secondary GET signers.
  Findings fixed: unknown-market lookup verdict, contradictory HTTP absence
  codes, exact collateral ticker selection, five secondary identity/route/nonce
  cases, and a live-capture untraded-X-Perp startup failure.
- Phemex money identity retains raw decimal digits; finite numeric output
  cannot collapse distinct running balances above binary-float precision.
- OKX mixed snapshots commit complete SWAP/FUTURES unions; native contract
  counts survive both worker/main frame bridges without growing the frame ABI.
- Paired TS fixes: primary-only OKX definitive lookup; Binance
  EXPIRED_IN_MATCH is canceled; blank untraded OKX ticker prices and zero-open
  percentages remain finite in both REST seed and stream.
- Focused current OKX suite85/85 and strict clippy PASS; Phemex96 and
  Binance39 passed in the preceding combined run; lookup15/15, independent
  frame16/16, transport/HOW251/251, harness331/331 PASS. Root typecheck
  44/44 tasks and lint28/28 PASS. Full final-head gates still pending.
- Actual public capture: OKX inverse books BTCUSD/ETHUSD,20seconds,100463
  frames/two books; trimmed8-second fixture retains41502records. First EEA
  capture correctly failed startup on an untraded placeholder and is not
  counted as coverage. Required-shape seed fix reviewed; rebuild/recapture next.
- Expanded global/EEA private development replays PASS with inverse coin
  collateral and X-Perp positions/orders. Harness review then corrected fake
  position financial arithmetic and added inverse/X-Perp trading lifecycles;
  final fixture/source replays and CI remain phase5 work.

### Harness/account equity review checkpoint

- Real global inverse public replay PASS: 41,499 WS frames, 509 markets,
  two books; steady CPU 1.08x TS/Rust and retained memory 4.31x TS/Rust.
- Successful EEA recapture: 29,709 frames/two X-Perp books. The public socket
  reconnects during startup; preserve the full 20 seconds so both real book
  snapshots are present instead of counting an empty eight-second book trim.
- Expanded private tapes now exercise inverse and X-Perp place/amend/partial
  execution/cancel lifecycles with native contract units and truthful margin.
  Their stronger oracle exposed account equity bugs in both implementations:
  native account pushes omitted live-order margin and represented aggregate
  equity as fake USD currency; TS local ticks restored stale REST equity.
- Paired fix retains a distinct aggregate anchor and real currency detail
  deltas, validates pushes atomically, preserves live-order margin, and fences
  delayed REST/tick/startup commits. Native OKX87/87 + strict clippy PASS;
  deterministic TS account7/7 PASS without network. Independent actual
  microtask red/green audit proves removing either tick/startup guard fails.
- OKX replay now compares per-currency balances through the existing generic
  comparison option. No normalization, frame ABI, or size ceiling is changed.
- Fetched newer Coinbase/Polymarket landing commits; merge before final gates.

- Expanded global private replay PASS with strict per-currency balances:
  35WSframes, three markets, 15commands, three executions. CI source/fixture
  phase independently reviewed PASS; two new public gates raise the pre-merge
  workflow to70replays plus7controls, without raising the wasm budget.
