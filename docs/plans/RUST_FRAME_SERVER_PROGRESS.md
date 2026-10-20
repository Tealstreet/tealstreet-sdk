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
