# Hyperliquid fee-tier account identity

Base: actual landing `fea427d9f31` (#1699). Priority production repair before
the approved browser-warning follow-up. Rust scope only; TS production code and
harness oracle/fixtures/declarations remain with their respective owners.

Official contracts: [Info endpoint user address and userFees](https://hyperliquid.gitbook.io/Hyperliquid-docs/for-developers/api/info-endpoint)
requires the trading account, not its API agent. [Fee tiers](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/fees)
aggregate sub-account volume with the master; vault volume is separate.

Re-verified: `hyperliquid/src/account.rs::account_read` special-cases userFees to
`options.key`, while `user()` already selects `vault.or(application_id)` for
account reads. Fees also start before userRole resolves an unknown agent's
account, so changing only the body would leave discovery using a blank subject.

1. Commit this plan and index row first. Use `user()` for userFees like the other
   account info reads. Keep signer identity for userRole discovery and all
   signatures. Vault takes precedence over application_id.
2. Queue private fees from `private_bootstrap`, after configured identity or
   successful role discovery. Preserve the existing anonymous public/no-identity
   fee read/fallback, public-only behavior, spot branch and request routing.
   Do not introduce a new fee retry policy or change fee calculations.
3. Native regressions: configured agent != account; vault distinct from both;
   agent/vault role discovery emits no premature fee read and then queries the
   resolved trading account; returned tier reaches market fees. Preserve the
   anonymous fallback and userRole signer subject. Demonstrate failures on the
   old code, then pass after the fix. Keep existing tests/assertions intact.
4. Coordinate #1698's independent account-subject oracle; no shared tape,
   manifest or TS edits. Run touched Hyperliquid + Binance native tests, scoped
   strict clippy/fmt, independent plan/implementation/adversarial reviews and
   CI public/private gates. No local release WASM or replay matrix. Merge the
   earlier green predecessor first and real-integrate any relevant shared input
   advance; fresh separate guard, exact-head merge and actual-pin reports.

Caller impact: corrected native account/vault fee tier, unchanged options and
transport/signing contracts. TS has the analogous defect and is explicitly
outside this repair; the overseer will raise it with Sam.

## Regression evidence

All six focused regressions fail against the prior native implementation and
pass after the subject/order repair. They cover explicit account/vault identity,
agent/user/vault role discovery, failed/malformed/timed-out roles and duplicate
late replies, mapped maker/taker rates and anonymous 422 fallback. Configured
fees retain their position before mode/account reads. No fixture, declared
failure, signer bytes, fee arithmetic or TS code is changed.

Local verification: all 437 Hyperliquid + Binance native tests pass, including
the six regressions; touched-package formatting and strict all-target Clippy
pass. Independent plan, implementation/adversarial and final-delta reviews are
clean. The harness oracle predecessor #1698 is integrated before final CI;
its fee-subject contract uses the same account/vault precedence.
