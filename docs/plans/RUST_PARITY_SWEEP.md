# Rust parity sweep, part 2

Branch: `feat/rust-parity-sweep`, from `origin/feat/rust-rewrite`.

Scope: audit every PR #1298–#1324 with `gh pr view`, cross-check the
`RUST_REWRITE.md` roster against current TypeScript, and close supported
capability gaps. Phemex income and definitive client-ID lookup are explicit
targets; extend history beyond a page only where TS supports that today.
The other lane owns Paradex remote signing, Bitrue history/client-ID/hedge/
margin mode, and KuCoin spot. Do not edit those surfaces. Keep the existing
1,258,291-byte per-venue WASM budget.

Current GRVT scope follows merged PR1337 and `GRVT_PRODUCTS_DECISION.md`:
perpetuals only. Earlier mixed-product work below is historical and superseded;
the final implementation retains exact cancellation, session and keepalive
repairs without exposing unrequested product kinds.

Latest union checkpoint: [RUST_PARITY_SWEEP_PRODUCT_UNION.md](RUST_PARITY_SWEEP_PRODUCT_UNION.md). Earlier runtime receipts and attempt boundaries: [RUST_PARITY_SWEEP_FINAL.md](RUST_PARITY_SWEEP_FINAL.md).

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

### GRVT public/private checkpoint

- Product-kind metadata now admits PERPETUAL, STABLE_PERP, FUTURE, CALL and PUT
  with exact native instrument identities; `isPerpetual` matches TS exactly.
  Private snapshots and fills preserve mixed-product account state without an
  implicit perpetual-only filter. Atomic metadata/read safeguards remain.
- Six new mixed-product tests and the complete GRVT91/91 suite PASS; strict
  all-target Clippy PASS. Independent phase review PASS with no findings
  (`/tmp/parity-grvt-phase-a-review.md`). Trading changes follow separately.

### Public gate measurement and index dispatch

- Merged-harness OKX linear and inverse public gates PASS. The first EEA gate
  matched exact output/memory but failed CPU while overlapping the root suite;
  the failed raw measurements are retained. One fixed three-pair control after
  that suite ended PASS on unchanged806844-byte WASM: main1.05x, steady1.08x,
  retained3.36x. Host load remained high, so this is not an idle-machine claim.
- Source inspection found a concrete independent hot path:3359 EEA index-price
  rows repeatedly scan750products (~2.52million comparisons and3359 temporary
  vectors). Derive underlying-to-instrument IDs once after the complete catalog
  commits and borrow them on updates. Exact underlying/native identities and
  SWAP/X-Perp fanout remain; no financial conversion or gate threshold changes.
- Extended underlying-isolation/unknown-index checks; OKX89/89 and strict
  all-target Clippy PASS. Independent review and rebuilt public gates follow.
- Independent index-dispatch review PASS with no findings
  (`/tmp/parity-okx-index-review.md`); only committed current-market IDs enter
  the derived map, and incomplete catalogs do not publish a new index.

### GRVT harness checkpoint

- Eight documented-pattern catalog IDs span two perps, stable perp, two dated
  futures, two call strikes and a put; removed the GRVT PERP-only projection.
  Independent EIP-712 controls assert exact per-product hashes and economics.
- Expanded private tape130records/17commands: six derivative placements, true
  individual partial fills, and exact snapshot-based cancellation with other
  expiry/strike/kind siblings present. The single global cancel remains `{sa}`.
- Independent harness review found and verified repairs to signed position
  notionals/PnL/short liquidation prices and fills above a BUY limit. Final
  Decimal inspection confirms all14position rows and six derivative fills.
- Harness387/387 PASS; independent fixture/source review PASS
  (`/tmp/parity-grvt-harness-review.md`). Actual mixed-product replay follows
  the native trading checkpoint and final merged build; it is not yet claimed.

### GRVT paired TS trading checkpoint

- Exact symbol cancellation reads/validates the raw complete subaccount snapshot
  before staging individual native/client identities. It preserves sibling
  expiries/strikes; global cancellation remains account-wide. Raw ACK checks
  reject false/malformed/error/mismatched responses before later writes.
- Independent review found and verified zero-client sentinel handling and
  pre-await client-lifecycle evidence. The expanded ACK review then found an
  internal authentication-resend race: client-only writes now disable that
  hidden resend; native-ID retries recheck disposal/main/subaccount scope after
  authentication. Ambiguous writes stay single-attempt; ACKs are not terminal.
- Focused142/142, full SDK typecheck and lint PASS; independent final review
  PASS (`/tmp/parity-grvt-ts-cancel-review.md`). Removing only the API retry
  guard makes the real401/code1000 lifecycle regression fail; exact source was
  restored and all checks pass. No test skips or timeout increases.

### GRVT native trading/query checkpoint and final hardening

- Native exact-symbol cancellation streams the complete `{sa}` open-orders
  read, validates typed rows and final slim envelope before any write, and
  stages only exact native instrument identities. Partial/auth-rejected reads
  reset staging without losing the original lifecycle baseline. Native-ID
  dispatch, per-write/auth guards, partial failures and pending ACK state reuse
  the existing sequenced job machinery; global cancellation is unchanged.
- Independent review found and verified two repairs: the initially unstreamed
  unbounded list now uses `request_streamed`/`on_item`; mixed ACK/Order proofs
  and explicit error/code fields cannot bypass rejection through fallback,
  including null/false/zero markers. The paired TS Order-proof guard was also
  tightened after its earlier checkpoint.
- GRVT104/104 native (19mixed-product cases), strict Clippy and independent
  final native review PASS. TS168/168 focused, full SDK typecheck/lint and
  independent final re-review PASS. Independent signatures pin every product
  hash/decimal refresh; chunked read/partial-failure/retry controls prove no
  cancellation prefix escapes a failed/incomplete snapshot.
- New roster PRs Coinbase1325/Polymarket1327/Breakout1328/perf-docs1331 were
  independently audited: no additional declared supported gap; live-key and
  TS-unsupported limitations are preserved in the census/PR draft.

### Landing integration: Binance profiles and Breakout

- Merged landinge391337054 (Breakout1328, BinanceSpot/COIN-M1326, GRVTperf-docs1331)
  after checkpointing GRVT. Separate constructor flags retain OKX contract-count
  units and Binance inverse contract multipliers in worker and main-thread
  appliers. Coinbase, Polymarket and Bitget frame contracts stay covered.
- Secondary GET support extends to the newly landed Binance profiles. Spot's
  single dynamic client strips public auth while retaining read egress; COIN-M
  keeps its separate direct public client. Native four-profile HMAC controls
  and actual SDK transport tests preserve primary writes/auth/lookup exceptions.
- Integration review caught Spot being swallowed by the futures public bypass.
  Corrected that policy; provider-derived matrix then exposed different signed
  and public Spot prefixes. Optional native-host `readPublicHttp` preserves that
  distinction after credentials are stripped, with no WASM/engine-frame growth.
- Binance+OKX198/198 native PASS. Final331/331 focused SDK/frame/transport tests,
  full SDK typecheck/lint and independent integration review PASS. Frozen golden
  has56existing entries unchanged,0removed,4new Spot/COIN-M +/-secondary cases;
  each uses the actual TS provider, then native tests consume the exact fixture.
- Final plan is57local replays (27public/30private),11contract controls and
  all25venue WASM ceilings. Merged CI preserves both OKX public additions and
  all Binance variants:91replays(34public/57private),25venue/30Rustjobs.
  Final root/compiled-WASM/size/replay verification follows on this union.

### Final merged native and transport quality

- Locked workspace native2431+3doctests PASS. The later native-host routing
  repair adds one test; its final complete41/41nativehost run plus strict
  all-target Clippy PASS replaces the prior40host cases, yielding2432+3 on
  byte-identical remaining Rust sources. This is a split-run attribution;
  exact-head CI will rerun the complete final workspace.
- Workspace fmt/bans/native all-target Clippy PASS; final locked full-workspace
  wasm32 Clippy PASS. Native-host strict golden counter now requires all60
  cases; the test-only header conversion warning and original stale56counter
  failure are retained in logs and corrected before the passing final run.
- Final331/331 SDK/frame/transport tests, full SDK typecheck/lint and independent
  merge/policy review PASS. Native host consumed the same frozen provider-derived
  golden fixture. `readPublicHttp` is optional host policy, contains no secret,
  and adds no WASM payload/engine field.
- Runtime is frozen at47f68ce883. Final root44/28/27 checks and combined25-venue
  release build started; no public CPU scoring overlaps this quality work.

### Landing integration: consumer cutover, frame server and scripted smoke

- Merge landing0063068241 (#1330/#1332/#1333), retaining both shared command
  decoding and secondary credentials modules. Independent core/native and SDK
  merge reviews PASS; incoming default/deletion work remains separate phase9.
- Semantic merge review caught OKX being enabled in two sizing policies. Keep
  the exact native-contract policy in both constructors; the generic policy
  must not overwrite inverse native filled quantities. Independent regression
  now drives all four actual RustEngineExchange constructors and main-thread
  BaseExchange readers with the same inverse frame. Independent review PASS.
- Incoming cutover fixture uses actual BTC-USDT-SWAP IDs for OKX aliases;
  financial assertions remain unchanged. Focused cutover225/225 and frame/chaser
  77/77 PASS; enhanced inverse suite12/12, full SDK typecheck/lint PASS.
- Previous root44/28/27, compiled SDK111/111 and host65/65 receipts passed before
  this merge; affected final-source checks are being rerun. Fresh workspace
  native and combined WASM builds are running, with no public CPU scoring.
- Final runner now includes frame-wire1 and scripted smoke9 integration
  controls alongside57 planned replays,11 venue controls and25 feature budgets.
  Scripted smoke is not funded live execution evidence. The fixed1,258,291-byte
  ceiling and strict output/performance comparisons are unchanged.

### Final-source checks and extended GRVT lifecycle replay

- Root typecheck44/44, lint28/28 and test27/27 tasks PASS. Both SDK test targets
  pass7491/7491 across508files; final bounded SDK typecheck covers the last
  incoming fixture edit. Compiled SDK379/379 across11files and host68/68 PASS.
- Real native cutover initially failed only EEA aliases: the old incoming
  helper supplied SWAP-only input. It now selects the actual captured EEA
  SWAP/FUTURES tape and maps the demo host without changing payloads/assertions.
  Focused EEA controls and independent fixture review PASS; the complete379
  rerun above includes both corrected profiles. Original377/379 is preserved.
- Explicit workspace native2460+3doctests, bans and strict native Clippy PASS
  before the following GRVT timer change. The lone fmt failure was a routing
  test's line wrapping, now corrected without semantic change.
- Expanded mixed-product GRVT replay matched account/output/commands/signatures
  but exposed a missing native keepalive: TS schedules a client ping five
  seconds after pong. Native now retains one replaceable timer fenced by
  socket/auth generation; data does not cancel it, retirement does. Existing
  idle thresholds are unchanged. GRVT108/108, strict Clippy and independent
  timer review PASS. Rebuilt mixed-product replay and final WASM Clippy follow.
- All four OKX private profiles PASS on the combined artifact, covering native
  inverse counts and EEA X-Perp lifecycles without output normalization.
- Final locked workspace2464+3doctests, native/WASM Clippy, fmt and bans PASS
  on fd4d6e4156; rebuilt SDK379/379 and host68/68 PASS with the timer repair.
- Rebuilt mixed GRVT replay isolated a harness identity error: optional bare
  heartbeat timing reversed between implementations at the five-second tape
  boundary. Exact GRVT ping/pong are now excluded from subscription identities,
  as JSON heartbeats already were. Raw socket auditing still precedes this
  comparison and rejects missing/wrong/extra pongs and pre-credential sends.
  Auth/feed/strike identities and all financial/scoring/budget checks remain
  exact. Negative controls fail without the fix; harness537/537 and independent
  review PASS. Both original private failures remain recorded; final replay
  follows this bounded harness correction.

### Final selected-artifact gate checkpoint

- Mixed GRVT private tape PASS on the rebuilt combined engine:130records,
  17commands,8markets,72frames; exact financial/command/signature/protocol
  comparisons retained. Four OKX private profiles PASS again on that artifact.
- Selected OKX node/web814,314bytes each (ceiling1,258,291), identical SHA256
  `4eaad3f31104d8e87601016e51236a32c1415b67771dbfd0a5678e2a4a0d6550`.
  Three-run public gates all PASS with two books per capture:

  | Capture | Frames | Markets | Main CPU TS/Rust | Steady CPU TS/Rust | Retained memory TS/Rust |
  | --- | ---: | ---: | ---: | ---: | ---: |
  | OKX linear | 8,904 | 509 | 1.16x | 1.23x | 4.19x |
  | OKX inverse | 41,499 | 509 | 1.07x | 1.05x | 3.73x |
  | EEA X-Perp | 29,268 | 750 | 1.07x | 1.07x | 3.43x |

- Measurements follow completed root/native/WASM quality checks, with no own
  compilation/tests overlapping scoring. Prior pre-optimization EEA failure
  and unchanged-binary control remain historical; these are the final rebuilt
  artifact's first full selected gate sequence, without raised thresholds.
- Primary verification is complete. Transfer bindings ownership to the serial
  runner for remaining50replays,24feature builds,11venue controls and10incoming
  integration controls. Source stays frozen until its results are reviewed.


### Final local gate completion

- Serial runner106/106 steps PASS with frozen source:50replays,24feature builds,
  11venue controls,10integration controls and537shared harness tests. No failure
  or retry in this final sequence; independent aggregation audit PASS.
- With the primary OKX7gates and1feature build:57planned local replays
  (27public/30private),all25budgets PASS. Extra combined GRVTprivate PASS is
  separate58th evidence. Largest module Paradex1,255,331bytes; ceiling remains
  1,258,291. Detailed counts, sizes and public measurements are committed in
  [RUST_PARITY_SWEEP_GATES.md](RUST_PARITY_SWEEP_GATES.md).
- Bindings lease returned; no more local runs needed without a new source
  change or review finding. Next: normal push, PR into feat/rust-rewrite,
  independent adversarial reviews posted to PR, exact-head green CI, merge and
  fresh follow-up branch. No live-key or separate phase9 gap is claimed closed.

### PR1340 adversarial review and advanced landing union

- Opened PR1340 into feat/rust-rewrite after all original local gates passed.
  Three independent reviewers covered queries/finance, transport/GRVT and
  SDK/harness; findings are posted as PR comments. Two reviewers independently
  found cached secondary-account orders bypassing primary lookups. Binance/OKX
  now bypass that cache whenever secondary credentials are configured, in Rust
  and paired TS, without mutating the cache or changing primary-only shortcuts.
- Finance review also found PM lookup returning native IDs where cancellation
  expects canonical client IDs. Preserve the PM client ID and native info ID;
  USD-M retains native IDs. Regression controls distinguish numeric caller and
  native IDs and verify cancellation against reconciled canonical store rows.
- Red controls reproduce both findings; focused TS16/16 and native201/201
  affected-crate tests PASS. Independent repair re-review and final union
  verification follow; no review finding is waived by the original gate run.
- Landing advanced to86ed5e3d2e while original gates ran: merge TxFlow1329,
  server adapter1334, replay/performance1335 and Lighter variants1336. Only
  textual conflict combines GRVT derivative-signature and index-equivalence
  regressions. Independent SDK/harness union review PASS.
- Incoming deterministic KuCoin harness control scheduled20ms before clock
  startup and could lose time to real scheduler delay. Use explicit frozen
  startup for that test, preserving its full20ms assertion and runtime clocks.
  Initial564/565 failure is retained; final565/565 harness PASS. Correct stale
  Lighter Rust alias/remote-only documentation from the incoming port.
- Rebuild combined artifacts and rerun root/native/WASM quality. Shared replay
  finalization changed, so repeat affected gates with the expanded manifest:
  123steps,58child replays plus primaryOKX7=65planned local gates; extra combined
  GRVT remains separate66th proof. Budget now covers26unique venue features
  (27profile builds including the Lighter alias) at unchanged1,258,291bytes.
  Incoming CI config97replays (36public/61private),32Rustjobs. Prior counts and
  measurements remain historical until this merged-source sequence passes.

### Landed GRVT scope decision and final source alignment

- Landing advanced again to02e9db0e8b/PR1337. Its explicit product decision
  supersedes this sweep's generic-mapper inference: TS sends `{}` and the
  documented API defaults omitted/empty kind to PERPETUAL only. Parent verified
  that official contract independently. Retain the landed retirement boundary;
  no Future/Call/Put/STABLE_PERP capability or live availability is claimed.
- Remove unused ProductKind metadata and synthetic six-product expansion.
  Preserve exact-symbol streamed cancellation, authentication/ACK/lifecycle
  fences, metadata refresh and pong keepalives. All19 meaningful native safety
  cases remain with seven supported perpetual fixtures; incoming retired-product
  rejection controls remain. GRVT115/115, strict Clippy and formatting PASS.
- Private GRVT tape now41records,2perpetuals,5commands. BTC exact cancellation
  reads an authoritative snapshot and preserves ETH; an independent ETH terminal
  event precedes the existing empty-store global cancellation. Native/perp
  signature,401/session,ACK and raw protocol checks remain. Only the synthetic
  six-product signature test was removed; harness564/564 PASS.
- Earlier b88 merged root44/28/27,SDK7621cases per target,compiled409/409 plus
  host68/68 and native2555+3doctests/Clippy passed. These are historical until
  final affected checks on the perpetual-boundary union complete. Primaryv8
  public scoring was held; no incomplete quality pass was used to allow it.
- Resolve docs to the landed decision and concatenate both incoming product
  boundary and retained pong tests. Independent final source review follows;
  no recorded failure or prior implementation claim is silently erased.
- Independent retirement-alignment review PASS: exact account/trading/history/
  signing/keepalive sources are unchanged from reviewed b88. Independent lookup
  repair re-review also PASS at b88, with cache and PM canonical-ID safeguards
  accepted. Final merged-source quality and gates follow this source freeze.

### Final performance diagnosis and bounded OKX price indexing

- Source8258 quality PASS:2562native+3doctests, native/WASM Clippy/fmt/bans;
  root44/28/27 with7624SDKtests per target; compiled409/409 and host68/68.
  Combined GRVT19frames/2perpetuals/5commands and all4OKXprivate gates PASS.
- Final inverse public gate failed CPU: main TS2308.8/Rust2432.5ms and steady
  1660.7/1821.0ms; strict output and retained memory passed. Preserve the full
  first sequence, including per-run variation (Rustmain2536/2432/2054ms).
  Independent diagnosis found no new public hotpath regression, but no claim
  that variance alone explains the failure. No blind sampling or threshold change.
- CI8258 passed36/37checks; sole Hyro steadyCPU miss5.4/5.6ms, while mainCPU,
  correctness and memory passed. Independent review supports one unchanged
  job control, not repeated retries. A new runtime commit instead requires
  fresh exact-head CI; the original failure stays linked in PR comments.
- Remove concrete OKX price-update overhead: cache native IDs to current
  ticker rows and avoid repeated readiness ID allocations. Rebuild only after
  either complete table commit; retain duplicate-symbol selection, missing
  ticker readiness and underlying fanout. Numeric logic and dirty-mask writer
  are unchanged. Four new event-driven controls cover these invariants.
  OKX94/94, strict Clippy/fmt PASS; independent source review PASS. Rebuilt
  public/private gates must establish actual performance; no speedup is inferred.

### Final checkpoint and landed spot-product union

- Source d61c7d5187 final checks PASS:2566native+3doctests, native/WASM
  Clippy/fmt/bans,409compiledSDK cases and68host cases. Combined GRVT private
  and all4OKXprivate profiles PASS. Selected OKX node/web819500bytes each,
  under unchanged1258291ceiling. Three-run public linear/inverse/EEA PASS;
  inverse mainTS2033.6/Rust1913.1ms,steady1480.3/1317.0ms,memory19.4/5.4MB;
  EEA main1709.4/1518.5ms,steady1297.7/1152.0ms,memory22.3/7.3MB.
  These receipts precede the new landing union and are not its final proof.
- Latest requested fetch/merge brings f4ea9e06ab/PR1339 Hyperliquid spot and
  Kraken spot into this branch cleanly, preserving landing structure.
  Rebuild combined bindings and repeat affected compiled/native/root checks;
  extend the frozen serial replay manifest with their public/private and
  integration controls. Review merge seams independently before final landing.
- Source remains frozen during final verification. Persistent PR comments/body
  will carry actual final aggregate receipts; no old gate numbers are relabeled
  as current. Merge remains gated on exact-head green CI and review convergence.

- Merged combined build found one compile-time constructor seam: incoming
  KrakenSpot shares KrakenOptions and must initialize the sweep's new optional
  read-credential field. Its private reads are primary-signed POSTs, so set
  that field to None explicitly. Original failed build retained; repeat the
  affected build/native/WASM checks on the fixed union.

### Final spot-account admission review repair

- Fixed-union4e3dd1622e Rust quality PASS:2633native+3doctests, all strict
  Clippy/fmt/bans. Root44/28/27 tasks PASS with7649SDK cases per target;
  combined421compiledSDK cases and68host cases PASS. GRVT and4OKXprivate
  gates PASS. No public score was sampled before the following review repair.
- Independent union review found Kraken Spot missing from shared secondary
  account admission. Its real TS helper selects secondary public GET egress,
  while all private reads/writes use primary POST signing. Add the actual
  API-Key routing policy entry; preserve native read_credentials:None.
- Actual production TS factory/helper control proves public AssetPairs/Ticker
  GET routes and primary Balance/WS-token POST signatures against an independent
  HMAC calculation; compare routed request bytes and native policy. The red
  control fails exactly at unsupported-account admission, then258focused
  transport/native-golden cases PASS with the one-entry policy repair.
  Independent repair re-review follows. Rust source is unchanged by this fix.
- Expanded final runner:136steps,64child replays (30public/34private),
  28profile builds/25unique features,19venuecontrols+11integrations. With
  primaryOKX:71planned gates (33public/38private),26unique feature budgets;
  additional combinedGRVT private is separate72nd proof. CI now101replays,
  34Rust jobs after landed spot aliases. These remain planned until completed.

- Independent Kraken Spot repair re-review PASS: one production policy entry,
  GET-only secondary selection and primary POST signer preserved; existing
  generic native exported-plan consumer needs no venue-specific mapping.
  Red/green258-case receipts inspected independently. Final source frozen
  after this reviewed repair; rebuild and affected root/compiled checks follow.

### Final evidence and review convergence

- Frozen runtime96a39f0421:71selectedreplays (33public/38private) and the
  additional combinedGRVTprivate proof PASS. All26unique feature budgets
  remain below1258291bytes; maximumParadex1255331. Root44/28/27tasks,
  SDK7652per target,compiled421+host68,harness574,native2633+3doctests,
  19venuecontrols+11integrations and strictqualitychecks PASS.
- Initial WOOsteady and Breakoutmain CPU misses remain recorded. Independent
  artifact/source/per-run reviews justified exactly one unchanged three-pair
  control for each; both PASS, as did same-headCI. No input/binary/scoring
  changes or further unchanged retries.136unique successful childsteps across
  138actualstepattempts;72acceptedreplays across74actualattempts.
- Independent final reconstruction review PASS validates every command, exact
  PASS output, control count, budget, source and binding hash. Correctness
  findings and repair re-reviews are posted onPR1340. Fullfinaltables and
  provenance are inRUST_PARITY_SWEEP_FINAL.md; prior receipts stay historical.
- RuntimeheadCI39/39PASS. This documentation-only checkpoint changes no
  runtime source; wait for exact final PR-head greenCI, merge into
  feat/rust-rewrite, then cut a fresh follow-up branch.

### Landing product-semantics union after completed local verification

- While final documentation-head CI ran, landing advanced to5e92f93a/PR1341.
  Its shared product metadata, Bybit combined products and OKX units overlap
  this branch. Merge it before landing; resolve conflicts by retaining the
  incoming typed semantics plus reviewed lookup/read-routing/equity safeguards.
- NativeOKX, SDK/frame and harness/WASM/Bybit conflict work is delegated to
  disjoint reviewers. Root owns documentation and final integration. Preserve
  both independent regression suites and actual product fixtures; no financial
  normalization, threshold change or budget increase.
- Completed96a39runtime receipts and both first CPU misses/control diagnoses
  remain historical evidence. Fresh merged-source verification is required;
  previous72accepted checks are not relabeled as proof of this later union.

- Product union conflict resolution checkpoint: incoming metadata/classification,
  combined Bybit options and USD-facing OKX risk retained with all lookup,
  read-credential, exact-filled and equity-revision safeguards.54focusedTS
  cases and3reference integrations PASS, including7race controls. New fresh
  REST reference drives real revision-fenced read rather than assuming a new
  array is fresh; explicit-empty ticker quotes retain proven values and
  malformed supplied quotes remain visible.
- First merged native118-case target had one incoming placement setup failure:
  it omitted completed account reconciliation. Complete the test setup and
  preserve readiness preflight/economic assertions. Independent native review
  found missing order currency reserve attribution; fallback to proven settle
  currency for REST/WS orders, with linearUSDT/inverseBTC regressions. All41
  product cases PASS; fresh complete workspace/combined checks follow.
- Independent registration and SDK unit/provenance reviews PASS; native reserve
  fix re-review pending. New CI inventory102replays(39public/63private),
  34Rust jobs. Parentadds incomingEEA publiccapture:8OKXreplays, so72planned
  selected local replays plus separate combinedGRVT=73proofs on the new union.
  Child manifest remains136steps/64replays/28builds; combined product-control
  group adds16cases. No old scores are attributed to this new runtime.

- Merged combinedSDK run reproduced two EEA startup timeouts. Synthetic
  FUTURES ticker book/volume fields were empty despite native required numeric
  fields; change only those fixture values to explicit0, retain blanklast/open,
  readiness/parserchecks,callerIDs and5s timeout. All57compiledSDK-file cases
  PASS. Full root44/28/27tasks,7666SDKcases pertarget,2683native+3doc,
  isolatedengine79 and strictquality PASS;16combinedproductcontrols PASS.
- CIalsoexposed existingPhemexREST/WS normalization inconsistency; paired
  productionfix/regressions are being completed and separately reviewed. No
  gatefinancialnormalization or failedread/revision guard is changed.

### Phemex REST/WS consistency repair after union CI

- Same reported accountBalanceRv/used fields normalized differently by channel:
  REST used rawtotal while both TS/nativeWS used rawtotal minuscurrencyUPnL.
  Real-timer final-writer order exposed1300vs1280 withUPnL20. This is a
  production consistency defect, not a financial-output normalization issue.
- Pair TS/nativeREST with the existingWS convention. Free remains raw-minus-used;
  TS aggregatefree sums proven currencyfree instead of subtracting adjusted
  total. Reported income, native rawaccountinfo,lookup,Triggered finality and
  stale/failedread guards remain unchanged. KNOWN_TS_FIXES documents the fix
  without rewriting either replay balance.
- Meaningful native red1000vs990 retained; green97Phemexnative +41focusedTS
  tests PASS. Positive/negative two-currency combinations, WS-to-newREST
  equality and failed-sibling retention are covered. Independent repair
  re-review PASS, with source hashes and red/green receipts posted onPR1340.
- Commit the paired repair, rebuildcombinedbindings and run freshfinalquality/
  gates. Earlier product-union source receipts remain attributed tob00644c7d8.

- Final8f71quality PASS:2684native+3doc,featureoff79,root44/28/27tasks
  with7670SDKcases pertarget,compiled421+host68+16productcontrols. Combined
  GRVTprivate and all8OKXreplays PASS. SelectedOKX824320bytes fixedbudget.
  Commitdocumentationcheckpoint before the64-child serialrun so finalheadCI
  overlaps execution. Runtime source remains8f71; no source/HEAD changes during
  the subsequent bindings lease. Actualfinalaggregates will be posted onPR1340
  with independent reconstruction, preserving older attempts separately.
