# Backpack Rust port

Branch: `feat/rust-backpack`. Spec: `RUST_REWRITE.md`; process: latest
`origin/feat/rust-rewrite:docs/plans/VENUE_PORT_BRIEF.md`.

## Phases

1. Public data: new sans-I/O venue crate, borrowed wire structs, streaming
   markets/ticker/mark seeds, on-demand depth snapshots plus buffered sequence
   bridge, bounded gap recovery, reconnect/watchdog, candle streams and reads.
   Verify sequence bridges, strict subsequent deltas, stale snapshots,
   reconnects and empty candles. Review with a sub-agent before committing.
2. Private data: ED25519 canonical signing with safe-cex-crypto, separate
   private socket, authenticated subscriptions, periodic REST reconciliation,
   global leverage/cross settings, net position flips, execution deduplication,
   and per-domain revision guards. Failed reads preserve prior state. Review.
3. Trading: sequenced jobs for single/native pair placement, brackets and
   triggers, cancel/replace amend, cancels, account-wide PATCH leverage;
   reject unsupported mode/margin changes. Preflight every caller client ID
   and the complete batch before any write; canonical uint32 decimal IDs.
   Validate cancel/replace before cancelling; never retry writes. Review.
4. Queries: streaming fills/orders/funding/public trades in engine JSON
   shapes, bounded pagination and explicit errors for unavailable queries.
   Verify official payload examples and boundary filters. Review.
5. Harness gates: wasm/SDK registration, venue harness hooks, deterministic
   private tape using public test credentials, live two-book public recording
   and compact fixture, generic replay extensions with Binance regressions.
   Run bans, fmt, clippy, tests, wasm lint/build, public/private venue gates,
   Binance public/private gates and repository checks. Review and record metrics.

## Delivery

After all gates pass, merge `origin/feat/rust-rewrite` into the lane (stop on
conflicts), rerun affected checks, push, open PR to `feat/rust-rewrite`, run
adversarial sub-agent reviews and post findings, fix and repeat, merge PR and
cut a fresh follow-up branch. Never touch master, rebase, or force-push.

## Progress

- Read current spec, port brief, root/package/venue guidance, engine/Binance
  seams and TS Backpack behavior. Verified official signing API documentation.
- Worktree initially clean on requested branch. Added initial signing and
  PATCH transport groundwork; public implementation in progress.
