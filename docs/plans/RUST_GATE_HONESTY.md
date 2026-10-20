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
