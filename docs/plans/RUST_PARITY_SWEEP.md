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

### Landing integration checkpoint

- Merged fetched Coinbase/Polymarket landing `a0c11afd87` in `f87356127d`.
  Five shared-file conflicts resolved as unions, independently reviewed: both
  frame bridges retain Coinbase native placement, Bitget inverse sizing, OKX
  contract sizing and typed prediction metadata. Transport retains secondary
  identity/primary exceptions plus landing alias-aware signature invariants.
- Post-merge root typecheck44/44 and lint28/28 PASS; transport259/259,
  independent combined account/transport271/271, bridge20/20 and core69/69
  PASS. Bans and workspace formatting PASS. Workspace native/Clippy, combined
  WASM/host tests and final replay measurements remain in progress.
- The account review independently removed each tick/startup guard and proved
  its intended assertion fails, then restored and re-ran all7 account tests.
  Fixtures reject unexpected network requests; there are no live-key test calls.

### Final artifact regressions repaired

- First combined artifact passes host64/64. Its actual SDK-WASM run found
  obsolete tests: OKX fixture omitted required native `instType`, EEA reused
  SWAP rows across FUTURES requests, and secondary keys were still expected
  to reject. Updated deterministic contracts now57/57 PASS, including actual
  HMAC/read-proxy vs primary write/definitive-lookup identity and three
  incomplete-pair preflight cases. SDK typecheck/lint and independent review
  PASS; no skip, timeout increase or product fallback introduced.
- Global/demo private gates PASS, but strict EEA currency tables caught a
  binary-noise reserve difference (7.5200000000000005 vs7.52). Native arithmetic
  now follows per-row decimal precision for reserve/PnL additions and balance
  subtraction, preserving coin precision. Independent review caught/fixed a
  scaled-integral rounding edge above2^52; exact tiny-coin/negative-PnL and
  cancellation regressions cover it. OKX89/89 and strict Clippy/re-review PASS.
- Full root test27/27 tasks PASS, including safe-cex7298/7298 in both configured
  runs. Native pre-precision checkpoint2149 tests+3doctests PASS, workspace
  native/wasm32 Clippy PASS. Rebuild/final native totals and replay suite next.

### Frozen implementation verification

- Final source `47f4bb19ff`: locked workspace2151 native/unit/integration tests
  plus3doctests PASS,0failed/ignored. Workspace formatting, native/all-target
  Clippy, locked workspace wasm32 Clippy and bans PASS. Runtime/test fingerprint
  remained unchanged across the final native run.
- Rebuilt combined WASM: real SDK102/102 across7files PASS, host64/64 PASS.
  All four expanded private OKX profiles PASS with exact currency balances,
  native units and full inverse/X-Perp lifecycle commands. Current SDK full
  typecheck/lint PASS; earlier root44typecheck/28lint/27test tasks PASS.
- Single-venue OKX size + three public gates now running; remaining touched
  venues/Binance and Aster routing regression batch follows serially before PR.

### GRVT landing refresh

- GRVT#1321 merged while the final batch was starting. Refreshed `gh pr view`
  receipt and independent TS census:25/27 requested PRs now merged; Bitunix
  #1309 remains open and #1315 remains a master/CLI PR. GRVT perpetuals
  declared limitations add no supported perpetual mutation/query gap. Its
  excluded derivatives and broader undeclared account/book APIs are recorded
  honestly in the census, not mislabeled TS unsupported or live-key-only.
- Paused runner cleanly after Backpack/Binance public/private PASS; those
  measurements are preserved as pre-GRVT evidence, not final new-source scores.
- Merged landing5d7589f296 in e191a54b8d. Wrapper conflict retains the complete
  lookup union, GRVT fill helper and fourth OKX frame flag. Independent merge
  review288/288 PASS; root typecheck44/44 and lint28/28 PASS.
- Landing includes shared sampler cleanup/measurement repairs and a verified
  Bybit numeric fast path. Rebuild all-venue artifact and rerun public metrics
  on that merged harness. Comparisons, financial output and byte limits stay
  strict; do not reuse old sampler scores as current measurements.
- Updated final plan:49 local replays (23public/26private),7contract controls,
  all24per-feature size limits; CI83replays(30public/53private),29Rustjobs.

### GRVT supported-product extension (scope correction)

The deeper source audit supersedes the initial perpetual-only conclusion above:
TS catalog, generic instrument-hash signing, individual/global cancellation,
positions/orders and chaser fills support returned FUTURE/CALL/PUT instruments.
The original port's product boundary is a declared supported gap under this
sweep. STABLE_PERP follows the generic TS path but must retain its exact native
kind and the TS `isPerpetual` flag rather than guessing financial semantics.

1. Public/private phase: retain validated native kind alongside signing metadata;
   admit documented derivatives, set exact product flags, preserve metadata-refresh
   safety, and read complete mixed product account snapshots/history without a
   hardcoded perpetual filter. Test simultaneous same-base/perp/expiry/strike
   identities, quantities, PnL and stale/failed snapshot protection.
2. Trading/query phase: reuse validated generic native signing and conversions.
   Exact symbol cancellation must obtain authoritative instrument-scoped open
   orders and cancel only those native identities. Never use kind/base/quote
   bulk cancellation for dated/option symbols: the endpoint has no instrument
   selector and would also cancel sibling expiries/strikes. Fail closed on
   incomplete reads; retain global account-wide cancellation. Paired TS repair
   follows the same safe symbol boundary. Test partial failures/session rotation,
   pagination/truncation, no write retries, fill progress and sibling preservation.
3. Harness/gates phase: extend documented mixed-product fixtures and independent
   signature/identity controls, review each phase, then rerun GRVT/Binance and
   touched-venue gates plus the fixed per-venue size ceiling. Live non-perpetual
   availability is unverified, not evidence of TS unsupported capability.

Native GRVT and paired TS/harness work have disjoint owners and cross-review;
commit at stable phases. No gate normalization, raised budget or broad-copy TS
method bodies. The census/PR body must reflect the closed product gap rather
than treating the original port boundary as a waiver.
