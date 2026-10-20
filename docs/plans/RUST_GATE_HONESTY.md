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
