# Coinbase Rust venue port

Branch: `feat/rust-coinbase`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and acceptance: `VENUE_PORT_BRIEF.md`.

## Scope and boundaries

One `safe-cex-venue-coinbase` crate serves `coinbase` and `coinbasespot`.
Preserve active Advanced Trade INTX perpetuals, CFM dated futures plus spot,
and the saved-account spot-only variant. Portfolio permission discovery decides
the authenticated product family. Spot-only must never probe CFM or INTX private
routes. Public-only INTX discovery remains keyless. Starbase modules are inert
in TS and remain outside runtime activation; do not guess their unresolved units
or private notification envelopes. Typed records replace the deleted first-port
JSON emulation described by stale Coinbase documentation.

## Phases

1. Public data: typed streamed discovery and ticker polling, full-depth sequenced
   books, candle stream/fetch, symbol aliases, subscriptions and socket watchdogs.
   Verify documented examples, gaps, reconnects, malformed snapshots and aliases;
   independent code-correctness review, fix, commit.
2. Private data and native JWT: portfolio selection, balances, positions, settings,
   paginated open orders, user snapshot batches and incremental executions.
   Stale/failed reads preserve newer state. Auth plan below is audited against
   official sources before implementation. Verify both algorithms, REST/user
   auth, permission probes, snapshot races, fill replay and CFM units; review,
   fix, commit.
3. Trading: preflight full batches and caller IDs; Market, Limit and StopLoss
   configurations; CFM quantity conversion; stable-ID spot edits with exact
   provenance; confirmed INTX cancel/replace; per-ID batch cancellation; fixed
   one-way/cross/leverage behavior. No automatic write retries. Verify ambiguous
   responses and cancellation failures; review, fix, commit.
4. Queries: bounded documented history/public-trade reads with typed output,
   pagination and explicit unsupported axes/filters. Never manufacture reported
   PnL by folding fills. Verify limits/cursors/errors; review, fix, commit.
5. Harness and gates: SDK/WASM/native registration for both aliases; freeze the
   TS reference outside production and remove replaced runtime code; documented
   private tapes with independent JWT verification; live two-book public fixture,
   declared TS corrections, identical-output/performance gates; Binance controls;
   bans/fmt/Clippy/native/WASM/host/SDK checks. Review, fix, commit.

After all gates pass, merge latest landing branch, rerun affected gates, push,
open PR into `feat/rust-rewrite`, post independent correctness reviews and fixes,
repeat until clear or diminishing returns. Merge only on exact-head green CI.
Cut a fresh follow-up branch from updated landing. Never stash/rebase/force-push
or touch master. Record measurements and unresolved evidence honestly.

## JWT design checklist (source audit in progress)

- Import keys once into an owned signer without Debug/private export. Accept
  normalized CDP downloaded JSON, qualified key names, SEC1 P-256 PEM, PKCS8
  P-256 PEM, escaped newlines and base64 Ed25519 secrets. Reject malformed keys
  before any authenticated I/O; errors never include credentials.
- Use `safe-cex-crypto::es256` for SHA-256/P-256 and its fixed-width 64-byte
  JWS signature, and `safe-cex-crypto::ed25519` for exact signing-input bytes.
  No host/WebCrypto or JavaScript signing. Zeroize temporary decoded secrets.
- Serialize typed JOSE header and claims; base64url without padding. Header
  includes `alg`, `typ`, exact `kid`, fresh nonce. Time is engine `Cx::now` in
  seconds, `nbf` now and `exp` now + 120. Reject invalid/overflowing clocks.
- REST URI binds uppercase method, canonical venue hostname and path, excluding
  query, scheme, fragments and routing/proxy prefixes. Sign immediately when a
  rate-paced request is dispatched, not when it is enqueued. Authorization stays
  on the real venue request; hosts retain routing responsibility.
- Each user/heartbeat subscribe receives a fresh connection-local JWT without
  URI. Reconnect generates new tokens, drops stale socket events, discards
  incomplete snapshot staging and requires a new snapshot. Server heartbeats
  drive watchdog health; no invented application ping or unsolicited JWT renew.
- Source audit must pin issuer/audience per algorithm, nonce entropy and key
  format validation, particularly 64-byte Ed25519 seed/public-key consistency.
  Tests independently verify signed bytes and inspect claims, key and time
  boundaries, and mutation rejection, rather than comparing random signatures.

## Progress

- Fresh branch created; clean landing brief/spec inspected. Independent scope,
  auth-source and public implementation work started. No acceptance gates claimed.
