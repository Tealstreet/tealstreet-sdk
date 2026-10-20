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
