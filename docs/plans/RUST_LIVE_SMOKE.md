# Credentialed lifecycle smoke

Branch: `feat/rust-live-smoke`, base `origin/feat/rust-rewrite`.

No credentialed network run is authorized in this task. Verification uses
scripted transports only. The executable uses the existing Wasm EngineHost;
native network primitives provide its transport. No engine shapes change.

## Phases

1. Configuration and private recording: explicit environment selection,
   per-venue environment-only credentials, endpoint fences, canonical private
   tapes with credential/session/signature redaction. Document supported engine
   test environments and credential names in RUST_REWRITE.md. Review and commit.
2. Lifecycle: wait for private readiness, inspect the account frame, select an
   active market, derive a minimum-quantity resting buy from current book and
   market constraints, place once, amend, cancel, cancel symbol orders and
   restore the original leverage. Bound waits, never retry writes, preserve
   partial native IDs, attempt cleanup on failure. Review and commit.
3. Gates: scripted fake transport drives the real Wasm host, records the run,
   and replays its tape through TS/Rust. Exercise configuration, secret leaks,
   sizing, failed/ambiguous writes and cleanup. Run existing harness and Binance
   regression gates and repository checks. Review and commit.
4. Landing: merge latest landing branch, push PR into feat/rust-rewrite, post
   independent adversarial correctness findings, repair and re-review, merge
   only at a green exact CI head, cut a fresh follow-up branch.

## Safety and compatibility audit

- No default production fallback: choose a reviewed test/demo profile or refuse.
  Mainnet requires the explicit --mainnet switch. Every REST/socket destination
  is checked against the profile before transport I/O; redirects are refused.
- No credential arguments, files or arbitrary endpoint overrides. No raw engine
  log, exception or request reaches stdout/stderr. Private tapes retain account
  and order data but no usable authentication material; create with mode 0600.
- Use authoritative current book quotes, a tick-aligned price below the bid,
  and the smallest step-aligned amount permitted by discovered size/notional
  rules. Never increase size after rejection. Refuse unknown leverage, invalid
  books, non-empty account orders/positions and insufficient sizing metadata.
- A dedicated empty account is required because cancel-all is intentional.
  Cancel the selected symbol to bound scope. Never flatten an accidental fill.
- Amendment may replace the native ID: retain both original and returned IDs.
  Ambiguous placement triggers symbol cleanup but no placement retry.
- Live private recordings gate correctness, not performance. Existing public
  fixtures remain the CPU/memory regression evidence. Unsupported venue features
  fail explicitly; do not silently mark skipped lifecycle steps passing.

## Progress

- Read landing brief, rewrite spec, root/package exchange contracts; clean fresh
  branch created from latest landing. Configuration and recorder work delegated
  in disjoint scopes. Lifecycle design under implementation.
