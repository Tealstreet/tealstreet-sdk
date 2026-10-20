# Binance / Bybit trading audit fixes

Branch: `feat/rust-audit-binance-bybit`, based on landing `bd94abb524`.
Scope: the requested trading findings in `RUST_AUDITS.md` (audit source
`df2d6a4a03`); TypeScript venues remain. No engine default changes.

## Phases and acceptance

1. **Binance private executions.** Reverify H1 (COIN-M triggered stop expires
   before its fill) and H4 (USD-M non-TRADE fills and missing trade-ID dedupe).
   Separate lifecycle freshness from execution identity. Test documented trade
   and lifecycle packets, duplicates, ordering, and late terminal executions.
2. **Binance trading and reconciliation.** Reverify H2 (cancel/fill race can
   replace full original size), M5 USD-M/PM stale snapshot resurrection, and
   UUID failure fallback. Defer replacement sizing until cancellation evidence;
   retain newer fills/terminal rows and fail closed on uncertain cancellation.
   Propagate client-ID RNG failure before writes. Test the actual command/event
   races and all changed family paths.
3. **Bybit private/trading.** Reverify M5 Demo/Hyro canceled snapshot race,
   M10 inverse amend units, and execution-ID dedupe. Test same-time REST/write
   races, native contract vs store units, repeated and distinct executions.
4. **Integrated verification and landing.** Independently review each phase,
   fix findings, run gates 1–6 for both families and all affected variants,
   SDK typecheck/lint/tests and host checks. Record actual numbers and evidence
   limits. Strike resolved audit bullets, push, PR into `feat/rust-rewrite`,
   post adversarial code-correctness reviews, fix/repeat, merge only on green
   CI at the reviewed head, then cut a fresh follow-up branch.

Every code fix receives a regression reproducing its failure scenario. Existing
code determines whether an audit item still holds; resolved items are recorded
as such rather than changed again. Reviews must inspect error paths and stale
responses, not merely test success. No account-value normalizer will be added
for the fixes. TS differences are documented in `KNOWN_TS_FIXES` and the PR.

## Verification plan

- Bans; format; strict all-target Clippy; complete Binance/Bybit native suites.
- Strict default WASM target Clippy; selected family release builds/size checks.
- Public and documented private replay gates for Binance USD-M, PM, COIN-M,
  spot; Bybit linear, inverse, spot, Demo and HyroTrader. Existing committed
  public captures remain the regression oracle because no public behavior is
  intentionally changed; Hyro's public fixture is explicitly synthetic.
- Root `corepack yarn typecheck`, `corepack yarn lint`, `corepack yarn test`;
  compiled WASM/host tests and harness checks as applicable.
- Public Rust CPU/retained memory must beat TS at unchanged thresholds.
  Private documented tapes prove scripted behavior, not live credentials or
  funded trading. The new native regressions pin adverse race behavior that
  the existing final-state replay oracle does not cover.

## Progress

- Initial inspection: clean worktree, no existing audit PR; fetched origin,
  read landing audit and brief before creating the requested fresh branch.
- Reverification underway against current implementations; H1, H2 and H4
  confirmed. Existing inverse/spot safety state does not cover USD-M/PM.

### Bybit phase review checkpoint

- M5 confirmed for Demo (removed rows lack timestamp fences) and Hyro (a REST
  reconciliation can start during a cancel, after dispatch but before its ACK).
  Order revisions now fence accepted cancels and terminal stream events, with
  regression cases at an identical engine timestamp, cancel-all, already-gone,
  partial batch rejection and full rejection controls.
- `execId` previously unused: bounded 4,096-ID per-product dedupe now survives
  reconnect and preserves distinct executions with equal economics. Invalid
  quantity/price packets do not consume identity. Legacy packets without IDs
  and duplicates older than the retained window cannot be guaranteed deduped.
- **M10 dropped after reverification.** The cited local amend assignment is
  reachable only for Demo/Hyro. Those variants cannot enable inverse products;
  standalone inverse and combined linear/inverse wait for authoritative stream
  updates. The new regression passed before production edits and preserves
  coin `remaining` plus native `remainingNative` across amend/stream updates.
- Verification: 147 native tests, strict all-target Clippy and format pass.
  Pre-fix Demo/Hyro snapshot, repeated execution and same-clock terminal failures
  were captured locally before the corresponding fixes.
- Independent Binance trading author reviewed the entire Bybit phase: no open
  correctness findings. Root additionally required malformed-first/valid-retry
  execution coverage; implemented and passing. TS differences documented beside
  `KNOWN_TS_FIXES`, with no new comparison normalization.
- Public live capture succeeded: 19,981 WS frames, 3 HTTP responses, both books;
  existing committed public and documented private variant gates are running.
