# Replay gate honesty

Branch: `feat/rust-gate-honesty`, based on `origin/feat/rust-rewrite` at
`1b8592dbd2`. Scope is harness, fixtures and CI; venue divergences are reported,
not repaired or normalized away to make this change pass.

## Phases

1. Independent expectations: replace the Binance/Bybit margin copy with a
   chronological tape oracle. Reject Rust access by any TS fixer; test the
   source contract and hostile output mutations.
2. Comparison coverage: default fill prices/client IDs on, compare protective
   and conditional order fields, realized PnL and execution metadata, reject
   duplicate public symbols, capture equivalent fixed tape checkpoints.
3. Fixer contracts: explicit per-venue leaf field declarations, runtime mutation
   enforcement, overwritten-field counts, no undeclared Rust-side rewrites.
4. Measurement: at least seven alternating-order pairs, ten percent minimum
   improvement for main/steady/process CPU and retained memory, audits outside
   performance replay, idle-loaded implementation baselines.
5. Integration: combined shipped wasm budget, Bitget USDC public/private gates,
   explicit fingerprinted known failures with visible evidence, matrix runs,
   adversarial PR reviews and green CI before merge.

Each phase receives an independent sub-agent review, necessary corrections,
targeted tests and a focused commit. Use repository yarn scripts only. Final
validation includes harness tests, repository typecheck/lint/tests, Rust bans,
WASM builds and the complete replay matrix. Record raw results and limitations;
never retry an unchanged failed performance measurement to obtain a pass.

## Progress

- Initial inspection: clean worktree, no PR on prior follow-up branch; fetched
  origin and created the requested fresh branch. Read the port brief, rewrite
  spec and gate-honesty audit. Current gate compares final state, accepts three
  runs, has the circular margin fixer and no combined build budget.
- Phase 1: margin corrections now use Binance native wallet/configuration and
  Bybit positionIM inputs only. Moved all TS oracle functions into an isolated
  module receiving no candidate results. AST lint covers imported helpers,
  renamed candidate parameters, computed Rust access and captured aliases.
  Independent review found the first lint bypass, now corrected; re-review PASS
  with 2/2 targeted regressions. Full harness verification recorded below.
  Full harness suite: 588/588 PASS (`/tmp/gate-honesty-phase1-reviewed-tests.log`).
- Phase 2: protective/conditional order fields and parent IDs, realized PnL,
  fill price/fee/time/execution identity and client IDs are compared by default;
  the explicit exclusions map starts empty. Startup, tape-quarter, command and
  terminal snapshots use production receiver state, without final-state fixers.
  Duplicate diagnostics are checked before venue filtering and normalization.
  Independent review found filtered duplicates were initially ignored; fixed
  and adversarial re-review PASS. Real Bitrue TS/Rust private replays completed
  23 frames each with 11 identical checkpoint identities.
  Ordinary production fill events/frame rows lack timestamps and execution IDs:
  null/null equality is absence of evidence, not proof of those capabilities.
  Conditional execution price also stays null when the production wire lacks it.
  Full harness suite: 602/602 PASS (`/tmp/honesty-phase2-harness.log`).
- Phase 3: every correction is now inside the isolated TS-only boundary;
  KuCoin native request expectations no longer rewrite candidate requests.
  Immutable per-venue leaf contracts enforce additions/removals, exact array
  direction/reorder, empty/type changes and newly widened fill metadata.
  Per-write observation counts overwrites and catches temporary mutations;
  descriptors/prototypes/extensibility fail closed, and a whole-output audit
  provides a second check. Review found and corrected directional-array,
  reorder and proxy-reference escapes. Current targeted tests: 31/31 PASS;
  previous full suite before these review fixes: 625/625 PASS.
- Phase 3 final independent re-review: PASS, including descriptor/accessor and
  temporary-write controls. Commit `127bb37a07`.
- Phase 4: one fully audited public correctness pair, then at least seven fresh
  interleaved TS/Rust performance pairs with alternating first implementation.
  Main/steady/process CPU and retained memory require at least 10% improvement.
  Cold implementation imports and WASM compilation are included in load+runtime
  CPU totals; baseline GC, validators, audit checkpoints and final projection are
  excluded from performance intervals. Minimal replay routing/receipt recording
  remains measured symmetrically. Memory subtracts an idle-loaded baseline and
  counts WASM growth exactly once; finite negative JS deltas stay signed.
  Every performance result must match its own raw audited final state, commands
  and exact transport receipt after timing stops. Independent review PASS,
  including the cold-load follow-up. Full harness: 657/657 PASS
  (`/tmp/gate-honesty-phase4-all-tests.log`). Real Bitrue functional controls:
  3,238 frames in each mode/implementation, five audited checkpoints and no
  performance checkpoints; no performance-pass claim from these single pairs.
- Phase 5 integration checkpoint (not ready to land): CI now records every
  strict result and mandates a separate verifier of all 31 suites / 106 exact
  venue/tape/book cases. The one known-failure manifest is intentionally empty
  until full matrix evidence is collected. Candidate/fixer integrity and
  performance-versus-own-audit workload failures cannot become exemptions.
  Exact private command timeouts can be declared incomplete failures without
  claiming completed parity or performance evidence.
  Added authored Bitget USDC public/private fixtures: public 36 frames and five
  checkpoints match; canonical private preserves a Rust `getAccount` timeout.
  Synthetic provenance and diagnostic-prefix limitations accompany the fixture.
  Combined standard web/Node build currently measures 11,948,833 bytes, SHA256
  `b440a9e996bc45e3f8b4428742e4ffa70e8da8a991d9f2b9b663ae40bdacc039`,
  below the new fixed 12,582,912-byte ceiling; standalone remains 1,258,291.
  Landing advanced while working; merge its consumer/public-feed changes before
  taking final measurements. Legacy fill oracle compatibility with widened
  metadata is under review; never erase actual TS fee/time/execution evidence.
- Landing merged at `d972b4d77d`, retaining URL identity caching and audit-mode
  guards together; merged tape/measurement/CI coverage controls: 98/98 PASS.
  The interim repository test run crossed the merge and is not final evidence;
  its two product-option failures pass 14/14 in a fresh post-merge run.
- Phase 5 implementation review: PASS after fixing legacy sparse-fill oracles.
  They now correct a detached TS shadow, preserve actual non-null execution
  metadata only through unique execution matches, and retain raw fills when
  matching is ambiguous. New null metadata initialization is explicitly bounded;
  metadata erasure, invention or reassignment remains forbidden. Coinbase's
  legacy price assertion follows numeric-string comparison semantics.
  Review caught Proxy cloning in the first adapter; descriptor-based detached
  cloning plus 32 production observer/factory integrations corrected it.
  Current focused results: 114/114 adapter/contracts, 14/14 real gate CLI tests,
  23/23 policy/budget/coverage/USDC tests. Full matrix evidence and final PR
  reviews remain required; known-failure declarations are not populated yet.
- Final post-merge local checks: harness 727/727 PASS; repository typecheck
  44/44 and lint 28/28 tasks PASS. Combined standard web/Node artifacts are
  12,172,135 bytes each (ceiling 12,582,912), SHA256
  `51e335bd0acd407f75b51b26bf780d083ea49bd720b9533c2fc2cc9906c2d3c3`.
  PR #1346 opened as a draft to collect the isolated 106-case matrix.
- Adversarial PR round 1 found an own-audit exception-policy hole: performance
  snapshot leaf/duplicate diagnostics could be declared venue exceptions.
  Protect every per-implementation performance-run diagnostic, verified 6/6.
  CI also exposed a CLI test's dependency on an untracked generated tape;
  replaced it with a self-created temporary fixture, 8/8 controls PASS.
  The blocked first matrix attempt reached no quantitative replay cases and
  was cancelled. Execution-ID projection correction is still in progress.
- Repository test verification on stable post-merge source: all 27 tasks PASS;
  safe-cex unit and CI suites each pass 7,748 tests in 523 files. Source logs:
  `/tmp/gate-honesty-final-{typecheck,lint,repo-tests}.log`.
- Round 1 follow-up tightened final diagnostic keys: whole market/ticker rows
  and books now produce leaf paths, so one declared field cannot hide a second
  final-only error introduced after checkpoints. CLI/policy controls 21/21 PASS
  (`/tmp/gate-honesty-leaf-review-tests.log`). Findings and corrections posted
  on PR #1346; phase and PR reviews are distinct evidence.
- Adversarial round 1 corrections preserve actual TS execution identity for
  Hyperliquid/Spot, GRVT and fallback-aware WOO/WEEX/MEXC events; missing native
  wire identity stays null. Actual Hyperliquid handler/checkpoint controls and
  89 scoped tests PASS; real TS replay retains execution 9001 on order 1002.
  Flat-position realized PnL and independent bracket trigger sources now survive
  projection. Coverage checks independently require every tape boundary and
  account/book sections, including identical-producer omissions; schema and
  coverage errors cannot be declared as venue exceptions.
- Initial isolated CI matrix run `39417011625` collected genuine strict FAILs,
  including raw TS defects previously hidden by terminal fixers. Its own-audit
  receipt controls exposed startup timer jitter and a Kraken Spot subscription
  race. Fixing deterministic startup scheduling, never exempting those checks.
  KuCoin's repeated empty command results falsely looked reordered to the audit;
  reserve unchanged positional rows before matching displaced common rows.
  Orderly's existing null leverage fallback now declares exactly its two
  finite-positive keys; extra fields/types remain forbidden. Contract controls
  53/53 PASS; re-auditing captured CI raw KuCoin/Orderly output has zero integrity
  errors. No unchanged quantitative replay was retried for a pass.
- The current host now rejects unsupported `getAccount` explicitly: Bitget USDC
  canonical replay completes but its account query differs from TS. The older
  timeout observation above remains historical evidence from the older artifact.
- Integrated round-1 harness checks: 749/749 PASS before the final timer control;
  startup/polling/KuCoin controls then pass 38/38 including an actual EngineHost
  deadline test reproducing the old 2-versus-3 OKX EU pings at a 19,953ms horizon.
  Frozen startup now keeps the same exact receipts under startup wall jitter.
  The first measured CI run completed 98 canonical cases (20 PASS / 78 FAIL);
  Binance's pre-gate lifecycle integration blocked its eight canonical cases
  because client-ID/TIF differences are now visible. Preserve that generated
  lifecycle transport proof and add a frozen sanitized strict lifecycle case
  to the mandatory matrix rather than ignoring its financial gate failures.
  CI combined web/Node artifacts: 12,172,412 bytes each, SHA256
  `9c36fc12f77b9766641b37ebf3599c1ee91b8c32ad6c274e99874cd7cbbaaede`.
- Preserve generated lifecycle transport/cleanup proof separately from financial
  certification: both actual implementations must complete every command and
  audit. Freeze the sanitized scripted capture verbatim as an additional strict
  Binance case (now 31 suites / 107 mandatory cases), SHA256
  `710a58aa511ed26bfe55bad1fef6b67a2544e5a119178560763261675d8862bf`.
  Its financial gate remains FAIL on 22 client-ID/TIF diagnostics, never ignored
  by the integration test or exempted through a variable capture hash.
- Round 2 review found shared omission of account subsections could still pass
  when `account: {}` remained. Coverage now requires normalized financial arrays,
  explicit hedge/balance evidence and the newly compared row fields; all such
  omissions are non-exempt. Actual CLI controls erase the same 24 sections/row
  keys on both checkpoint producers while leaving complete final outputs: all
  fail. Gate tests 16/16 and unit/financial/policy controls 18/18 PASS.
  Compatibility audit of 714 historical replay artifacts / 2,884 snapshots found
  only the intentionally new flat-PnL and leg-trigger fields absent, no unrelated
  schema gaps. No values were inferred or existing scope normalized away.
- Corrected-head CI exposed a startup-clock regression before any Hyperliquid
  output and before Breakout private startup: frozen Date prevented paced
  bootstrap request budgets from advancing. Logical startup now advances only
  to declared timer deadlines, then pending timers are rebased from that logical
  ready time. Both actual correctness CLI failures reproduced before the fix
  and complete afterward; startup controls 39/39 PASS. Strict receipt equality
  remains unchanged; no quantitative retry was used to mask these failures.
- Round-2 integrated harness: 754/754 PASS before the final base-row/clock follow-up.
  Verifier logging now distinguishes unexpected/invalid results from genuinely
  declared known FAILs; invalid PASS reports are never counted as certified.
- Independent follow-up required base economic/identity row keys as well as the
  widened fields. Every always-projected order/position/fill field and each
  settings leverage shape now has a presence contract. Previously undefined
  production values serialize explicitly as null, preserving null-vs-zero
  differences without inventing facts. Shared erasure controls, including final
  outputs, pass 17/17 CLI tests; unit controls 33/33 and lifecycle 9/9 PASS.
  Presence does not establish facts absent in both production consumers.
- Priority dispatch closes the remaining real native-timer jitter path: a
  delayed parent wake now drains earlier logical children before later timers,
  including Promise children and intervals; callback generations reject stale
  queued/rearmed wakes and the ready transition. Independent adversarial review
  PASS (ties, clears, repeats, reentrant start); 140 scoped controls PASS.
  Actual Hyperliquid public and Breakout private correctness replays complete
  with exact unchanged receipts. Integrated harness then 764/764 PASS.
- Initial known-failure declarations cover 49,262 exact diagnostic paths from
  104 completed corrected-head CI cases plus two corrected private pairs
  (106 distinct cases: 25 strict PASS / 81 strict FAIL). Hyperliquid public still
  requires the complete n>=7 CI result. Preserve the earlier Hyro seven-pair
  steady-CPU miss as an additional historical declaration even though the next
  run passed; no favorable-run selection. Group only exact literal paths sharing
  a rationale to avoid repeating prose; no patterns/ranges are permitted.
  All 106 collected reports independently validate against these declarations
  with zero integrity/unknown errors; full final-head CI remains required.
- Independent normalization/schema/null-oracle review PASS, 99/99 scoped tests.
  Existing Orderly missing-value corrections now accept the serialized null
  spelling only under the same native limit/zero-market payload evidence;
  unexpected populated values still fail. No additional correction fields.
- Independent grouped-declaration review PASS: all 49,262 literal keys match
  collected evidence; adjacent row/field/prefix substitutions and grouped
  integrity exemptions are rejected. The sole extra historical diagnostic is
  the verified Hyro steady-CPU miss. Human routing report names every one of
  the 49 failing venue aliases in RUST_GATE_HONESTY_RESULTS.md; no full private
  parity is claimed. Final 107-case/head certification remains pending.
- Landing advanced again to `702a62050c` (broker/cancel fixes, Bitrue private TIF
  and signing-clock setup). Merged as `2b10e43733`, resolving the extracted oracle
  conflict without restoring its old Rust-access API; preserved incoming notes
  in the isolated module and incoming post-load signing-clock resets together
  with deterministic startup. Independent merge audit PASS, 38/38 controls.
  Post-merge full harness 766/766, typecheck 44/44, lint 28/28 and repository test
  27/27 tasks PASS. New full matrix is run `39422195309`; declaration updates
  require its exact new evidence, not exemptions for a changed workload.
- Post-landing matrix review (`39422195309`, head `2b10e43733`): Hyperliquid
  public now completes seven measured pairs with matching own-audit receipts.
  Independent raw/source reviews classified 12 newly visible Paradex canceled-row
  paths, one Bitrue TIF path (the sorted rows already have different IDs), and
  nine OKX balance paths: native retains an 83.6 reservation after order 101
  closes while TS refreshes it. KuCoin's landing fixture change preserves the
  same 60 failures under its new tape hash. Added only those literal paths and
  refreshed hashes; historical observations, including Hyro CPU, remain declared.
- Corrected a separate TxFlow fixture assumption, never an exemption: both
  implementations wait 25 ms after each of two initial ticker reads before
  recording account startup. The scripted limit-500 execution window now starts
  at epoch+50; explicit limit-2000 history/query windows remain at epoch.
  Independent generation verified only 17 such timestamp fields changed.
  Both canonical correctness replays have zero unserved requests/protocol errors
  and exactly the prior three interim settings differences. New tape SHA:
  `0dc0d15f920189c2c708efb537aca6df5f5cb3342decb03df03306587a22976b`.
  Scoped protocol tests 15/15, integrated harness 766/766, policy tests 9/9 PASS;
  independent fixture review PASS. Full updated-head matrix remains required.
- Combined shipping browser/Node artifacts after the broker landing merge are
  each 12,174,800 bytes, SHA256
  `637e35fe2648c20d8e3783ab430f68b0691eee56406de80525b2e10d2b750195`,
  under the unchanged 12,582,912-byte combined ceiling. Standalone budgets stay
  unchanged. This size result does not establish combined-load performance.
- Complete post-merge matrix evidence: 107/107 strict reports collected, 27 PASS
  / 80 FAIL. The final Coinbase suite exposed seven TS performance receipts
  differing from its own audited public replay; these are protected harness
  failures, not venue exceptions. Tracing and correcting this before the next
  matrix. Two Coinbase ES256 private variants also expose additional raw
  terminal-order checkpoint paths, under independent evidence classification.
- Coinbase public receipt defect corrected with the existing replay-readiness
  hook: wait for the actual public socket and completed market/ticker seeds
  before book registration. The production handler otherwise schedules a 100 ms
  pre-open retry; auditing could advance startup through it and rebase polling
  differently. Generic paced clock semantics and private readiness are unchanged.
  Actual audit/no-audit receipt-only controls now each perform nine product GETs,
  deliver 201 frames and match complete transport receipts and terminal state.
  No quantitative performance claim comes from these diagnostic controls.
- Coinbase private evidence independently classified: INTX ES256 adds 23 and
  spot ES256 adds 45 literal paths. Earlier native REST reconciliation omits
  closed order 40 while TS retains it, shifting stop 50's sorted row; eight
  additional TIF leaf paths compare same-ID orders with native PostOnly/GTC
  versus absent TS TIF. All commands/writes/signatures/protocol checks complete.
  Exact manifest refresh adversarial review PASS, including failed attempts to
  exempt the seven public own-audit errors. Updated full CI and final reviewer
  findings are recorded on PR #1346; the single known-failure list retains FAIL.
- Post-Coinbase-fix verification: 767/767 integrated harness tests PASS with no
  skips; targeted startup controls 12/12 PASS. Old public receipt errors remain
  rejected by the verifier even if explicitly declared, proving the fix did not
  weaken the failure policy. Final-head CI performs new seven-pair measurements
  after this workload correction; no unchanged measurement was retried to pass.
- Final independent readiness review PASS: real Coinbase negative control
  reproduces the old 900 ms/10-read phase versus corrected 1,000 ms/9 reads.
  Manifest refresh review preserves every old key, rejects adjacent unknown
  fields/hashes and all public own-audit errors. Results audit confirms all
  107 cases, 49 aliases and both combined artifacts; PR wording distinguishes
  measured standalone performance from combined size and lists the private
  terminal-retention/TIF divergences. Full updated-head CI is the landing gate.
- Updated-head CI `39424799340` confirms the TxFlow fixture correction. Two
  previously unseen literal paths were independently classified: KuCoin Spot
  checkpoint-5 caller ID (TS omits it; asynchronous cancel reconciliation
  determines whether the row still exists), and Backpack startup SOL book
  visibility (the previous observation exposed BTC instead). Final books and
  all fourteen Backpack measured/audited receipts agree. Added only these two
  paths and evidence metadata; every earlier observation remains declared.
  Independent refresh review PASS with ten adjacent/protected-error controls;
  policy/verifier tests 9/9 PASS. No comparator or threshold changed. The full
  matrix completes before the next certification push; base remains `702a62050c`.
- Completed `39424799340`: all 107 reports, 27 strict PASS / 80 strict FAIL.
  Coinbase public has no performance/own-audit errors; independently inspected
  all seven receipts for both implementations (201 frames each). The remaining
  sixteen previously unseen CFM ES256 paths are independently source/raw
  classified: four missing TS stop client IDs, nine stale-open/closed-row
  lifecycle paths, and three shifted-row paths. Exact commands, writes, tape
  boundaries and intrinsic receipts agree; all protocol counters are clean.
  Added those literal fields with timing/alignment caveats. All 107 collected
  reports now assess with zero unexpected diagnostics; policy/verifier 9/9 PASS.
  Final certification reruns preserve all earlier observations, including the
  historical Hyro performance miss; favorable samples never erase a failure.
- Final CFM refresh adversarial review PASS: exactly sixteen evidenced literal
  paths, no changed prior declarations; seven neighboring/hash/protected-error
  probes reject. No harness code or threshold changed in this refresh. All
  requested phase and PR reviews are recorded on #1346; merge still requires
  green CI on the published final head.
- Certification run `39426820375` exposed fourteen additional literal Paradex
  paths. Source/raw review confirms unchanged tape, boundaries, 33 events,
  fifteen commands, nine writes, exact intrinsic receipts and zero protocol
  errors. Checkpoint 8 briefly retains native batch rows 2001/2002, shifting
  different-ID comparisons against TS retained 1001; checkpoint 14 retains
  canceled native 1000 for one extra boundary, then both clear at checkpoint 15.
  Added only these observed paths with timing/alignment caveats. Independent
  adversarial review PASS (six adjacent/protected-error negative controls),
  policy/verifier 9/9 PASS. Earlier declarations remain intact; no gate code,
  performance threshold or protected failure changed. Collect full run before
  the next certification push.
- Completed `39426820375`: 107/107 reports, 27 strict PASS / 80 strict FAIL.
  Every report assesses with zero unexpected diagnostics against the reviewed
  fourteen-path refresh. The sole red suite was the correctly undeclared
  Paradex result at execution time; all other Rust and repository checks passed.
  Publish this declarations-only refresh and require green exact-head CI before
  merge. The report document pins a complete reviewed collection and directs
  final certification to the PR so later runs never imply universal parity.
- Landing advanced to `6fea5de843` (leverage-side PR #1349) before final merge.
  Merge conflict was only the moved legacy TS-oracle block: retained the gate
  byte-for-byte and preserved the exact incoming Lighter note in `ts-oracle`.
  Retained incoming replay forwarding of leverage side and position-mode symbol.
  Independent staged-resolution review PASS; harness 767/767 PASS. Repository
  typecheck 44/44, lint 28/28 and tests 27/27 PASS from Turbo cache (safe-cex
  unit/CI each 7,772 tests). Incoming Rust/TS changes require rebuilt artifacts
  and a fresh full matrix; historical artifact bytes in the results document
  are now explicitly collection-pinned rather than labeled the current build.
  The pre-merge draft head was pushed after the conflict interrupted the merge;
  it is not certification evidence. Complete and publish the reviewed merge.
- Focused incoming command-contract review PASS: serde-defaulted optional side
  and symbol preserve canonical tape decoding; malformed side values still
  reject and both hosts/replay paths forward the fields. Canonical no-side
  tapes do not certify new side-selective behavior; incoming wrapper/native
  tests provide that evidence. Per-leg settings comparisons still require the
  venue descriptor's independent-leverage contract. Fresh CI must expose any
  changed venue diagnostics and remeasure combined size; upstream three-run
  performance claims do not satisfy this branch's stricter measurements.
- Merged-head run `39429258321` completed 107 cases: 27 PASS / 80 FAIL.
  All native/repository checks and 35 Rust jobs passed. The only unknowns were
  four Spot EdDSA client-ID paths at checkpoints 7–10: both rows are stop ID 50
  with identical geometry; TS omits the client ID while native retains it.
  Independent source/raw and adversarial reviews PASS (seven protected/neighbor
  probes). Private transports are unchanged except one additional native public
  metadata refresh (24 to 25); no private performance claim/exemption is made.
  All 107 reports assess clean against the four-path refresh; policy 9/9 PASS.
  Both rebuilt shipping artifacts are 12,199,435 bytes, SHA256
  `085ac4da04b16f6a5faa5e1d3097f332656b6682ebf2f9d5ba02776031862435`,
  within the unchanged 12,582,912 ceiling. All 49 result rows match this collection.
  Landing advanced again (`5f84b3e21d`), producing a PR conflict; commit this
  evidence before the next authorized integration merge, without stashing.
- Integrated landing `5f84b3e21d` (Binance/Bybit audit PR #1348). Gate remains
  byte-identical; all thirteen incoming audit-comment lines are preserved in
  the isolated oracle. Independent resolution review PASS. Frozen lifecycle
  input compatibility review PASS: complete cancel ACK identity/quantities and
  zero-size non-trade lifecycle events satisfy the new native guards. Keep its
  bytes and six mandatory-success commands unchanged. Canonical Binance tape
  now supplies two distinct trade IDs and complete cancellation economics, so
  regenerate and classify its new hash against rebuilt native output.
- Local post-merge verification: harness 767/767 PASS; typecheck 44/44 and lint
  28/28 PASS. An overlapping repository run hit two import timeouts; both
  controls pass in isolation (37/37). Full sequential repository run PASS:
  27/27 tasks, safe-cex test-ci and test-unit each 7,791 tests. No timeout or
  gate threshold changed. Rebuild Binance and run correctness-only private
  controls before publishing; full CI still supplies quantitative measurements.
- Rebuilt standalone Binance from committed merged source `389ede07e5`:
  web/Node each 1,000,570 bytes under 1,258,291, SHA256
  `d6bb7df17bda8befe2a899dd6a72c2f0c0736f338b79efc25885a83124d1eb2a`.
  Correctness-only canonical pair: fourteen events, three commands all OK,
  zero unmatched/missing/unused/signature/protocol errors; exactly the prior
  seventy-eight diagnostic paths under new tape SHA
  `8066c3b4de3742be5345977c9f66d270898b1b8ebbf220f1cffc384f433f6ba8`.
  Frozen pair: unchanged input, thirteen events, six commands all OK, all
  integrity counters zero, exactly its twenty-two existing paths. Both strict
  results remain FAIL. No local quantitative performance claim.
- Exact new-hash manifest clone independently reviewed PASS: groups byte-equal
  to the prior canonical declaration, all historical entries retained, six
  adjacent/hash/protected-error probes reject. Independent generation verifies
  precisely three input record changes; no fixture or comparator workaround.
  Policy/verifier tests 9/9 PASS. Publish this reviewed merge and hash binding;
  full CI rebuilds combined artifacts and runs seven interleaved public pairs.
- New-head CI `39434565069` exposes one additional literal Paradex path at
  checkpoint 9: TS retains open existing order 1000 after cancelSymbolOrders,
  while native applies the taped empty orders response. Independent raw/source
  review PASS: unchanged tape, commands and checkpoint identities; per-engine
  receipts match run 39426820375 (private refresh counts vary in other runs).
  Both process 33 events, all fifteen commands succeed and nine writes
  match, all integrity counters zero. Keep every prior observation and declare
  only this path, with no comparator/threshold/integrity relaxation. Clarify
  results-document wording: final certification status is recorded on the PR,
  never implied by a historical collection. Require another green exact-head
  CI after the declarations-only update.
- Full run `39434565069` collected all 31 suites / 107 cases: 26 strict PASS
  and 81 strict FAIL. Woo public now exposes an already-declared startup
  checkpoint difference (earlier PASS remains historical evidence). Both
  shipping artifacts are 12,258,087 bytes, SHA256
  `27d6c1b92cda35313d566996cf83c9ca156ee02a82506e4266912dfa047e2bc8`,
  below the unchanged 12,582,912 ceiling. All generic/native checks pass.
- Coinbase exposes eighteen more literal paths under unchanged existing tape
  identities: three CFM ES256 same-order stale-open statuses; eleven CFM EdDSA
  row shifts/terminal-retention paths; four base-coinbase Spot EdDSA missing TS
  client IDs (distinct from the separately declared coinbasespot alias).
  Independent raw/source review PASS. Each runner processes 61 events and all
  fifteen command results are unchanged, including existing error results;
  writes and checkpoint identities agree, all integrity counters zero. Private
  refresh counts vary; no private performance claim or exemption. Preserve all
  previous observations; only these exact eighteen paths are added. Results
  table now pins the reviewed collection and its shipping bytes. Require green
  exact-head CI after this declarations-only refresh before merge.
- Final combined declaration/table review PASS: exactly nineteen new literal
  keys, all historical keys/rationales preserved; all 107 reports assess clean.
  Twenty neighbor/hash/protected-integrity probes reject; policy controls 9/9
  PASS. All 49 table aliases and combined artifact bytes/hash independently
  match the cited run. Publish this stable declarations-only checkpoint, then
  wait for fresh full exact-head green CI; no venue FAIL is relabeled PASS.
- Published head `4434823df0` certified green in Rust run `39436814801`
  (all 36 jobs) and both repository CI runs `39436802157`/`39436814696`.
  Independent certification verifies all 31 suites / 107 cases: 27 strict PASS,
  80 strict FAIL, zero invalid/unknown; all 41 public cases have seven samples
  per runner (574 total), the 10% contract intact. Combined bytes/hash remain
  12,258,087 / `27d6c1b92cda35313d566996cf83c9ca156ee02a82506e4266912dfa047e2bc8`.
- Landing advanced to `f895a98f26` (Paradex/emitter PR #1352) just before merge,
  creating a conflict. Authorized integration preserves the gate byte-for-byte
  and moves the exact four incoming Paradex audit-comment lines to the isolated
  oracle. Independent staged-resolution review PASS. Earlier c44 wire changes
  were already tested in the certified virtual merge; thirteen newly incoming
  files change Paradex rejection/SBE behavior and TS listener isolation.
  Require integration review, local checks and fresh green CI before merge.
  Wind-down instruction: finish this PR, write handoff, start no queued work.
- Scoped incoming integration review PASS: TS emitter preserves listener order,
  once/off/context and original event objects; fill capture/worker callbacks
  keep their existing semantics. Paradex successful canonical batches, integer
  leverage and valid SBE inputs retain the same input/hash contract. Partial
  rejection/SBE repairs require no new fixer. Local verification is running;
  publish the reviewed integration so independent CI can run concurrently.
- Integrated run `39439275804` exposes twelve additional literal Paradex
  checkpoint-9 row-shift paths: TS retains canceled caller 1001, native retains
  only batch terminal rows 2001/2002 beside existing 1000. This repeats the
  checkpoint-8 reconciliation family. Independent raw/source review PASS:
  same tape, 33 events, fifteen successful commands, nine writes, checkpoint
  identities and per-engine receipts as certified run `39436814801`; all
  protocol/completion counters zero. The incoming partial-batch error paths
  are not exercised, successful placement behavior and account repair code
  are unchanged. Add only these twelve observed paths, preserve all history,
  and require another green exact-head CI. No comparator/threshold relaxation.
- Completed integrated collection `39439275804`: all 31 suites / 107 cases,
  27 strict PASS / 80 strict FAIL. Both repository CI runs are green; only
  newly undeclared Paradex/Coinbase paths make the replay verifier red.
  Coinbase adds eight exact paths: INTX EdDSA retains stop 50 in TS at row 3
  versus native row 5 after closed-40 retention (not missing by ID), and base
  Spot ES256 compares the same stop's missing TS client ID at row 5.
  Independent raw/source review PASS: unchanged tape hashes, boundaries,
  commands/results and writes; all protocol/completion counters zero. Private
  metadata poll counts vary as already documented. Combined web/Node each
  12,262,898 bytes under 12,582,912, SHA256
  `e0fbbd617e4f7df444db8da269a433cee6fe5ff7ac089325437cbac953fe29e7`.
  Local harness 767/767, typecheck 44/44 and lint 28/28 PASS; tests still running.
  Declare only the eight paths alongside reviewed Paradex twelve; keep every
  historical observation and all strict protections. Require fresh green CI.
- Final refresh review PASS: exactly twenty evidenced keys, every historical
  identity/key/rationale preserved. All 107 reports assess clean; fifteen
  neighbor/hash/protected-integrity probes reject and policy controls 9/9 PASS.
  All 49 result rows and combined size/hash match the complete integrated run.
  Publish this stable declarations-only checkpoint; fresh full exact-head green
  CI and completed local checks are required before final merge and wind-down.
- Landing advanced to `d17518bc5f` (socket-supervisor PR #1350) before the
  declaration refresh could start PR CI. Authorized merge conflicts are limited
  to gate and CLI tests. Keep the strict gate byte-identical: the incoming
  midpoint utility remains diagnostic-only, with its standalone regressions;
  its final-ticker comparison exception conflicts with this task's requirement
  not to loosen comparisons. Port CLI controls to assert qualified decimal
  midpoint differences still FAIL, equal values PASS, and other fields, books,
  transports, private finance and own-audit work remain exact. Preserve incoming
  auth notes in the isolated oracle and shared parser/venue repairs unchanged.
- Resolution-plan and scoped supervisor integration reviews PASS. Supervisor
  timing is sans-I/O and deterministic; stale timer ownership cannot revive a
  canceled socket, pongs do not extend auth deadlines. Rebuild all venue audit
  baselines and seven-pair measurements; earlier socket-lane reports do not
  satisfy the strict gate. Local root validation from the unchanged TS source:
  typecheck 44/44, lint 28/28, tests 27/27 tasks (safe-cex unit/CI 7,847 each).
  New harness controls and staged resolution are under review before publish.
- Strict merge controls reviewed PASS. Twenty-two original CLI tests passed;
  the new private fill/fee fixture initially lacked native execution evidence.
  Add an independent empty user snapshot and FILLED update; focused five-case
  private controls now PASS without touching the existing oracle. Reviewer
  verifies candidate-only price/fee mutations still reach financial comparison.
  Gate remains byte-identical; no midpoint exemption is executed. Full updated
  harness is running. Publish reviewed integration for independent full CI.
- Socket integration `c645b24b37` passes the full local harness (877/877).
  CI `39443426730` completes 30 suites / 105 reports; Weex stalls in checkout
  and supplies no evidence, so this collection is explicitly incomplete.
  Coinbase exposes fourteen exact CFM ES256 paths at checkpoints 15–16: TS
  retains stale 12/30 rows and native contains 11/13/50. Stop 50 exists in both;
  whole-row absence is positional. Independent raw/source review PASS: same
  tape, boundaries, 61 events, fifteen unchanged command results, five matching
  writes and zero protocol/completion errors. Native receipt is unchanged;
  TS periodic census/poll counts vary. Add only these paths, keep all history.
  Shipping web/Node both 12,306,304 / 12,582,912 bytes, SHA256
  `1363f5ae01f9d12f9547519822e6fddbeff456056949488676245f33e073f5bb`.
- Landing advances again to `3686c4ffa1` (Binance/Bybit follow-up PR #1354).
  Authorized conflict resolution changes only the exact incoming Bybit comment
  and moves two Binance dedup comments to the isolated oracle. Runtime gate,
  six strict midpoint control groups, and all policy protections unchanged;
  independent staged-resolution review PASS. Incoming native/host/wrapper
  changes require rebuilt CI and the frozen Binance six-command lifecycle.
  Latest load rule: focused local checks only, CARGO_BUILD_JOBS=4, no local
  WASM/replay/size/timing sweeps. No such heavy local process is running.
  A fresh complete 107-case CI remains mandatory; stalled checkout is not a
  known venue divergence and cannot replace missing replay coverage.
- Focused integration verification PASS: Binance wrapper 4/4, fixer AST lint
  1/1, policy/verifier 9/9. Independent fourteen-key policy review PASS with
  seven neighboring/hash/protected-integrity probes; all prior observations
  preserved and incomplete Weex coverage explicitly stated. Scoped native/SDK
  integration review PASS: no ABI/frame/capture change, branded unsupported
  Spot/COIN-M history stays distinct from empty success, and the frozen USD-M
  fixture's six mandatory-success commands/bytes remain intact. Publish the
  reviewed integration and require fresh complete CI; no local heavy checks.
- Takeover (claude-jitask) integrates landing `3a6737115e`, `9cb7452673` and
  `ed2128a337`. Incoming MEXC mini-ticker and Bitrue l/L corrections run through
  the audited oracle under declared contracts (MEXC subscriptions are add-only
  with multiplicity); #1380's sub-10 ms main/steady CPU allowance applies to the
  medians, with the 10% rule above 10 ms and process CPU/memory unchanged.
- Shipping combined WASM grew from 12,258,087 to 12,749,706 bytes with the
  2026-10-20 audit-fix merges (#1355, #1357, #1360, #1361, #1365, #1370, #1353,
  #1369, #1376, #1372, #1381, #1364, #1382, #1380). Overseer decision: raise the
  combined ceiling to 13,631,488 (13 MiB); per-family budgets unchanged.
- Adding the Bitunix family (#1309) grew the combined WASM from 13,523,241
  (landing `c50c51abd9`) to 13,931,829 bytes at opt-level 3, and to 13,870,074 with
  Bitunix size-optimized (CI `39525669706`). Users download only their venue's
  family since #1393 (Bitunix 843,722 of 1,310,720), so the combined figure caps
  total package size and a new family legitimately adds its crate. Overseer
  decision (2026-10-21, +Bitunix family): raise the combined ceiling to
  14,155,776 (13.5 MiB); per-family budget unchanged.
- CI `39453323762`: landing changed the MEXC, BloFin and Backpack private tape
  generators. New hashes declare only previously declared paths (MEXC 82 of
  118, BloFin 91 of 91, Backpack 164 of 164). TxFlow public newly fails on
  empty checkpoint-0 books after #1353 dropped TxFlow's `books()` time filter;
  that is a real regression routed to the landing auditor, not declared.
- Landing `949bf75f18` integration (`9a84faaf0e`): landing's HyperliquidSpot fixer
  removed reads from the Rust candidate; replaced by an add-only TS expectation
  of tape-declared `frontendOpenOrders` reads. CI `39460207328`: HyperliquidSpot
  private re-hash declares the same 7 paths. Bitrue private (tape gained l/L
  executions) keeps its 50 order paths; 50 checkpoint fill paths are the TS
  cumulative-delta defect `fixBitrue` corrects only in final state, each Rust
  fill independently matched to a raw tape execution (overseer-approved); 4
  checkpoint-11 order paths repeat checkpoint 10's declared divergence. TxFlow
  public empty checkpoint-0 books remain a real regression with its owner.
- Coinbase INTX EdDSA private (unchanged tape `f117e75e`) varies between CI
  `39460207328` and `39463152700`: the Rust REST order census in flight at record
  86 (cancelOrders) resolves at checkpoint 6 or 7. TS and final state are
  identical. The union of both observed path sets is declared so either outcome
  passes (overseer option a); deterministic census under replay is routed to
  the incidentals lane.
- Landing `2319bd42ac` (#1421) raises the single-venue family budget to
  1,310,720 bytes; ported into `WASM_BUDGETS.venue` (this PR's replacement for
  the gate job's `WASM_BUDGET_BYTES`). Coinbase CFM ES256 checkpoint 12 shows
  the TS-side census-timing twin (run `39469014934`); both outcomes declared.
