# Native frame server plan and checkpoints

Branch: `feat/rust-frame-server`, based on `origin/feat/rust-rewrite`.
Target: `feat/rust-rewrite`; never master. Full auto, stable commits, no stash.

## Phase 0 — inspect and design

Read the venue brief, Rust rewrite, native host, wire decoder, FrameApplier,
and all safe-cex-server exports. Record current cache/ticker responsibilities
and staged application migration in RUST_REWRITE.md. Review the plan with an
independent sub-agent before considering the design checkpoint accepted.

## Phase 1 — book delta wire and consumers

Add section 13 BOOK_DELTA: aligned u32 market/bid count/ask count/reserved,
then f64 price/amount pairs, bids then asks; zero amount deletes a price.
Keep format 1 and existing BOOK unchanged. Opt in to deltas per engine;
first publication, resubscription, snapshot replacement/gap, and changed market
identity use full BOOK. Coalesce all changes since the previous flush;
snapshots do not drain pending changes or change shared flush baselines.
Prevent book-only frames from outrunning pending market identity tables.
Mirror the section and typed views in wire.mjs/.d.mts. FrameApplier retains
per-identity book state, sorts levels and recomputes cumulative totals, handles
empty full resets and rejects a delta without its baseline. Repeated reads of
a frame must not mutate state twice or change previously returned books.
Verify Rust engine tests/clippy/fmt, decoder tests and SDK integration tests.
Run independent correctness review, fix findings, and commit this phase.

## Phase 2 — native account server

Use one host actor per explicitly configured account and a WebSocket listener.
Authenticate account-scoped opaque tokens before upgrade. Share typed command
JSON decoding with WASM rather than invent a second trading/query contract.
A connection flushes pending changes for existing clients, takes a full snapshot,
and registers in one actor turn. Broadcast binary frames in order. Allocate
internal command IDs, route results only to the requester, and bound outbound
queues/in-flight commands; a slow peer must disconnect and resnapshot.
Track subscriptions per client and retain a venue subscription while any client
needs it. Cleanup on disconnect and shutdown; prevent credential/account
creation through client messages. Keep account execution independent of client
arrival, and document lifecycle and TLS/deployment ownership.
Verify real loopback clients: snapshot before deltas, late join/reversion,
reconnect, account isolation, duplicate client IDs, invalid messages, shared
subscriptions, slow peers and shutdown. Review independently, fix and commit.

## Phase 3 — bandwidth and acceptance

Deterministically measure full versus delta frames for two 1,000-level books,
with explicit per-side depth and changed-level count. Include snapshot cost,
WebSocket overhead and bytes/second at a stated cadence; no load test.
Run bans, native workspace tests/fmt/clippy, wasm32 clippy/build, host decoder
and SDK tests/typecheck/lint, harness tests, unchanged Binance public and private
replay gates and combined venue WASM compatibility. Commit measured results
and phase reviews. No new venue tape is applicable to a host-only change;
use scripted native transport/loopback and existing Binance traffic as oracles.

## Phase 4 — PR correctness review and landing

Merge the latest landing branch as needed, repeat affected gates, push and open
a PR into feat/rust-rewrite. Run independent code-correctness reviews against
the PR diff, post findings on the PR, fix/review until passing or diminishing
returns. Merge only after required CI is green, then cut a clean follow-up
branch from origin/feat/rust-rewrite. Record actual gate numbers and limitations.

## Progress

- Initial architecture inspected; migration responsibilities recorded. Phase 1
  engine/consumer work and phase 2 host planning assigned to disjoint agents.
- Phase 0 independent review (frame_native): migration boundaries and protocol
  plan accepted. Clarified that disconnected in-flight IDs remain reserved until
  completion, and stalled socket writes require a deadline as well as bounded
  queues. Both are included in the implementation acceptance criteria.

### Phase 1 acceptance

- Added opt-in BOOK_DELTA and full baseline resets on subscriptions, gaps,
  whole-Book replacement, and market identity changes. Unique internal reset
  revisions prevent replacement books from colliding with an old baseline.
- Added typed decoder views and SDK reconstruction, including zero deletions,
  sorted totals, identity reorder, empty resets, repeated reads, malformed-frame
  atomicity, and detached source buffers. Forwarding workers explicitly skip
  book ingestion; re-enabling requires a full baseline.
- Independent reciprocal correctness reviews (frame_engine/frame_consumers)
  found and fixed reset-revision collisions and unread-snapshot ingestion.
  Both final reviews pass. Root also reviewed engine and consumer changes.
- Verification: engine 63 tests; engine native all-target and wasm32 clippy;
  engine fmt; host JS 54 tests; focused SDK frame tests 21 across five files;
  safe-cex typecheck/lint; actual Rust writer -> JS Frame -> SDK FrameApplier
  integration 1 test. All pass. No TS venue behavior changed.
- Deterministic measurement: two books, 1,000 bids and 1,000 asks each; ten
  amount changes per side per book (40 changes total). At four flushes/second,
  SCXF payload drops from 96,064 to 704 bytes/frame (384,256 to 2,816 B/s),
  a 99.267% reduction. One unmasked, uncompressed WebSocket binary message per
  flush adds ten bytes to the full message and four to the delta: 384,296
  versus 2,832 B/s. Book-only initial/resync snapshot costs 96,064 payload
  bytes, plus current store/market sections. Real deployments also incur
  TCP/TLS/IP overhead and workload-dependent change counts; not measured here.
  Reproduce with `corepack yarn -s rust:test-frame-wire` or
  `cargo run -p safe-cex-engine --example book_bandwidth` from sdk-rs.

### Phase 2 acceptance

- Added generic FrameServer/ServerConfig, heterogeneous preconfigured account
  actors, a Binance executable example and protocol/lifecycle documentation.
  Account engines run independently of client connections. Browser subprotocol
  and native Authorization authentication are checked before upgrade; query
  tokens and account creation are rejected.
- Shared the complete existing WASM command decoder with the native host.
  Independent review compared the moved enum and conversion byte-for-byte.
  Internal IDs isolate concurrent clients, preserve orphan reservations until
  completion, and never wrap or retry potentially submitted writes.
- Implemented atomic flush-before-snapshot joins, reference-counted interest,
  fair event selection, bounded subscriptions/commands/queues, slow-peer
  cancellation, and limits on both text and binary outputs. Ping flushes and
  socket operations have deadlines. Account RAII cleanup stops timers and
  transport tasks even when serving is aborted; upstream send/flush/close waits
  are cancelable and bounded.
- Independent phase review (frame_engine) and root audit found and fixed event
  starvation, subscription growth, detached transport tasks and cancellation,
  plus a race in the late-join regression itself. Consumer interoperability
  review (frame_consumers) also passes. No unresolved correctness findings.
- Verification: 48 native-host tests (12 unit, one host, 23 routing, 12 real
  loopback), native/WASM all-target clippy, examples and fmt pass. Loopback
  coverage includes authentication, browser negotiation, same/different-account
  result isolation, snapshot/reversion/reconnect ordering, shared subscriptions,
  orphan capacity, malformed/oversized input and snapshots, Ping, incomplete
  upgrade shutdown and abrupt serving-future abort. Deterministic unit tests
  prove queue overflow cancels a peer and releases its last subscriptions.
- Broad TS acceptance: root typecheck 44 tasks, lint 28 tasks, tests 27 tasks;
  both SDK runners pass 7,108 tests across 484 files, and server adapters pass
  27 tests in each runner. No dependency or lockfile additions were needed.
