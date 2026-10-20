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
