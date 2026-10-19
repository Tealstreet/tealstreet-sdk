# Bitunix Rust futures port

Branch: `feat/rust-bitunix`; target: `feat/rust-rewrite`.
The venue brief and `RUST_REWRITE.md` are the acceptance contract. Full auto;
review each phase with a sub-agent. Never touch master, rebase, or force-push.

## Phases

1. Public data: borrowed wire records, streamed market/ticker startup,
   sharded ticker/price sockets, legacy full snapshot books and candles,
   ping/watchdog/reconnect state. Register crate and wasm factory. Verify
   bans, fmt, clippy and documented-payload public tests, then agent audit.
2. Private data: double SHA256 signing, private login, account object/array
   handling, position aliases, settings, debounced reads and tick resync.
   Guard snapshots by generation so in-flight reads cannot undo events.
   `realizedPNL` is already net of fees AND funding: carry verbatim.
   Verify scripted startup, rejection, race and reconnect tests; agent audit.
3. Trading: sequenced jobs for place/batch/brackets/TP-SL, native amend,
   cancel/cancel-all and account settings. Caller IDs must survive exactly
   once or reject the whole batch before writes; never retry unknown writes.
   Preserve sibling protections and raw identities. Verify payloads, batch
   pacing, failure and ID contracts; agent audit.
4. Queries: fills, order history, income, public trades and candle reads in
   engine JSON shapes; time-window paging/dedupe and fee conventions.
   Verify documented payloads and page boundaries; agent audit.
5. Harness/gates: venue adapter, deterministic private tape, live 20-second
   two-book public recording and gzipped fixture; compare TS/Rust including
   outbound requests. List intentional TS bug fixes explicitly. Run all
   brief gates, Binance regressions and repository checks; agent audit.
6. Integrate: merge origin/feat/rust-rewrite into lane, rerun gates affected
   by integration, push, PR into feat/rust-rewrite, adversarial local agent
   reviews posted as PR comments, fix/repeat, merge PR, fresh follow-up branch.

## Audit notes

- Legacy `depthop` is a full book, not a diff sequence; replace both sides.
- Futures socket binary frames are gzip; host must deliver decoded text once.
- Public sockets share URLs: replay must distinguish connection instances.
- TS order reconciliation returns empty on failures; Rust must preserve state
  and document that correction. Streamed envelopes need success validation.
- TS private orders lack per-execution notifications; investigate cumulative
  fill quantities and record any intentional improvement as a known TS fix.
- Read contracts first; do not transliterate the TS class structure.

## Progress

- Planning: repository/engine/venue contracts inspected; official signing docs
  checked. No implementation yet. User's revised process authorizes integration
  after all gates pass and overrides initial never-push/merge instruction.

### Phase 1 review and verification

- Implemented crate and wasm registration, streamed seeds, shard pacing,
  snapshot books, candles and socket watchdogs; gzip frames decoded in hosts.
- Reviewer `public_review` found and prompted fixes for early stream startup,
  duplicate-socket retries, unchecked envelope codes, candle request options,
  native binary support, JS close ordering, malformed streamed arrays and
  gzip truncation. Added opt-in bounded `FieldEnvelope` streaming mode.
- Verification: bans PASS; venue clippy all targets PASS; 7 public tests +
  10 engine tests and doc tests PASS. Browser host ordering test added.
- Environment: corepack is in Volta node 24.21.0 bin; prepend it to PATH.
- Full venue gates remain pending phases 2–5. No gate numbers claimed yet.


### Workspace nesting checkpoint

- Merged `origin/feat/rust-rewrite` at b32847fca6 in b218c4f417; adopted
  automatic `venues/*`, engine/crypto and hosts paths. Preserved phase-2 WIP
  using named stash ef104f904eab6a3fb85ccf179e3a8e4ba5e39ce3 and restored it.
- Phase-1 new host test PASS. Full JS host suite waits for wasm build: existing
  Workers test cannot load the not-yet-generated bundle.
- Crypto audit `public_review`: wallet_utils and zklink/contract have no
  external callers or tests. Removing these first-port facades, keeping
  signing primitives and their independent vectors.
- Phase-2 review `private_review` found alias/null handling, snapshot ordering,
  cached terminal resurrection, parent/virtual-leg corruption, private silent
  heartbeat, wrapped balance extraction, cumulative fills, settings and WS
  signing whitespace issues. Fixes and regressions in progress; not ready.

### Phase 2 checkpoint

- Implemented double-SHA256 REST/WS auth, account object/array/wrapper reads,
  streamed position/order wrappers, settings and position identities, regular
  and compound TP/SL mapping, startup/tick/debounce reconciliation, stale read
  serials/generations and terminal guards, cumulative execution deduplication.
- `realizedPNL` is carried verbatim, net of fees and funding; regression pins
  -3.5 with independent fee/funding fields. TS missing live fills and failed
  reconciliation clearing state are intentional corrections to list in the
  harness allowlist during phase 5, and in the final change report.
- Review fixes include REST advancement of execution baseline, rejecting
  identity-free order rows, updating native TP/SL protection metadata and
  ignoring delayed updates after terminal events.
- Verification: venue clippy PASS; 1 signing + 7 account + 7 public tests and
  10 engine tests PASS; crypto suite 28 unit + 1 independent integration PASS.
  Added another malformed-order snapshot regression before final phase run.
- Remaining phases: trading, queries, harness/gates and integration.
