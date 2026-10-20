# Rust security audit fixes

Branch: `feat/rust-security-fixes`, based on landing `bd94abb524`.
Scope: Security section of `RUST_AUDITS.md`; original audit at `df2d6a4a03`.
Reverify each finding before changing it. No TS venue deletion, credentialed
live trading, weakened parity oracle, or performance threshold changes.

## Phases

1. Hosts (M1–M4 and host Low items). Require deployment-provided per-account
   token before Workers WebSocket upgrade, never trust the first client's
   credentials as authorization. Reject a conflicting start and serialize
   startup before its first await; recheck client membership and generation
   after loading. Catch malformed client JSON and options parsing without
   echoing secrets. Disable redirects in native HTTP, browser/Node fetch and
   Workers fetch (including outbound WebSocket upgrades). Native errors must
   carry no request URL. Bound decompression and polling intervals while
   preserving explicit pause semantics. Regressions exercise unauthorized
   upgrade, conflicting/racing starts, disconnect during loading, malformed
   input, redirect exfiltration, signed URL errors, bombs and interval edges.
2. Live smoke and recording (M5 and tape/scanner Low items). Hard fixed USD
   notional ceiling covers both placement and amendment with native contract
   units; use post-only where the adapter supports it. Environment-specific
   credential names have no mainnet fallback. Replace tape denylisting with
   explicit protocol/header allowlists, strip Set-Cookie from recorder output
   and existing fixtures without refreshing market data, and narrow the
   synthetic-key scanner exemption. Regressions use deterministic fake
   accounts/transports only; preserve replay payload/timing evidence.
3. Signers and credentials (M6 and cryptographic Low items). Allocate monotonic
   nonces atomically across engine objects for each signing identity; HL and
   TxFlow share their underlying protocol signer namespace. Registry keys
   contain public identity or one-way fingerprints, never raw secrets. Retain
   sequence on engine recreation and clock rollback; native threads share the
   registry, Wasm shares within one module memory (independent processes or
   separately instantiated Wasm memories need external coordination). Redact
   credential options Debug, forbid Polymarket credential serialization,
   zeroize owned secrets including error paths, shorten Paradex authentication
   authorization, compare receive windows to TS, and hedge production Lighter
   Schnorr nonces while retaining explicit vector APIs. Test collisions,
   concurrency, drop/recreate, secret formatting, and production nonce entropy.
4. Integration and gates. Independent correctness review of each phase;
   address findings before committing each phase. Run workspace formatting,
   bans, strict Clippy/native tests and Wasm checks/build, host/harness and TS
   quality gates, isolated venue size/performance gates for every changed
   venue plus Binance, private fixture gates, and combined consumer checks.
   No live private evidence is claimed from scripted tapes.
5. PR and landing. Merge current origin landing structure, push and create PR
   into `feat/rust-rewrite`. Post independent code-correctness reviews on the
   PR, fix/retest/review until accepted, merge only with green CI, then create
   a fresh follow-up branch from the landing branch. Strike addressed security
   findings in `RUST_AUDITS.md` with regression evidence and report any dropped
   finding explicitly.

## Progress

- Read current origin audit and venue-port brief before branching; clean
  worktree, no stash. Host, signer, and harness owners independently reverify
  current findings and implement disjoint phases with regression coverage.
- All six Medium findings and listed Low categories remain present on this
  baseline. Receive-window subcases will be compared to current TS rather
  than shortened speculatively.

### Hosts checkpoint

- M1–M4 implemented with token-before-upgrade, one pending/running start,
  post-load membership/generation checks, generic client errors, no redirects,
  and URL-free native HTTP errors. Gzip/zlib output capped at 16 MiB.
- Poll interval command and native typed-event paths clamp to 1000–120000 ms;
  positive infinity/null still pause. Startup-option forwarding follow-up is
  part of the integration/credential phase (the shared bound is exported).
- Independent phase review accepted the host fixes, and identified that
  startup forwarding gap for correction. Native engine 86 tests, native host
  64 tests, JS host 34 tests, TS account transport 257 tests pass. Full JS had
  78 passing before the new generic Wasm parse-error assertion; that assertion
  is deliberately committed with its rebuilt Wasm implementation later.
