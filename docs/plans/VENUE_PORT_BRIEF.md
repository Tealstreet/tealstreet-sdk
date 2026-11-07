# Venue port brief (one lane per venue)

You port one venue from TypeScript safe-cex to the Rust engine. Read
`docs/plans/RUST_REWRITE.md` first; it is the spec.

## Reference

- The finished pattern: `sdk-rs/venues/binance/` — `lib.rs` (public
  data, socket watchdog, `Venue` impl incl. `books()`), `account.rs` (private
  stream, REST reconciliation, stale-snapshot guards), `trading.rs` (commands
  as sequenced jobs, client-order-id contract), `history.rs` (rare reads as
  JSON), `paths.rs`, `sign.rs`, `wire.rs` (borrowed serde structs), and
  `tests/` (scripted tests from documented payloads, `tests/support/`).
- The engine: `sdk-rs/engine/src/` — `Venue`, `Cx`
  (`request_streamed` + `on_item` for large lists), `Store` (settings,
  hedged), `json::ArraySplitter`, `paging::RangeWalk`, `num`, `wire`.
- The TS you replace (behaviour, never code to transliterate):
  `packages/safe-cex/src/exchanges/<venue>/` (its `CLAUDE.md` first), plus
  `packages/safe-cex/src/exchanges/CLAUDE.md` and `packages/safe-cex/CLAUDE.md`
  for cross-venue contracts: client order IDs, reads-only retries, one fill
  per execution, reconciliation never commits a failed read as empty.

## Build

A crate `sdk-rs/venues/<venue>/ (crate `safe-cex-venue-<venue>`)` implementing `Venue`:

- Public: markets, tickers (REST seed + stream), full-depth books with the
  venue's sequence/gap rules, candles (stream + `FetchCandles`). Large REST
  lists stream through `request_streamed`. No `serde_json::Value`.
- Private: signing exactly as TS, private stream auth + subscriptions, REST
  reconciliation as TS's tick does, position mode, per-market leverage and
  margin into store settings, stale-snapshot guards.
- Commands: place (brackets, conditional types as TS), amend, cancel,
  cancel-all, leverage, margin type, position mode; queries (fills, order
  history, income, public trades) in Binance `history.rs`'s JSON shapes.
- A liveness watchdog on every socket with the venue's ping protocol.
- Open every socket with `Cx::connect(SocketSpec::new(url).channel(..))`, tagged
  `SocketChannel::{Public, Private, Business}`; upgrade headers go in `.headers(..)`.
  Binary protocols add `.binary()`; decode borrowed
  `Event::WsBinary` payloads in the venue. Hosts preserve bytes on this explicit
  path and keep the existing text/decompression path for other sockets.
  Keep the venue's real URL; hosts apply the TS account routing policy. Roles
  are required even when public and private feeds share a URL.
  Independent feeds sharing a role and URL add `.feed(..)` with a stable
  feed identity. Its recovery hint reports missing protocol payloads; hosts
  select a declared fallback. Never embed Tealstreet URLs inside venue code.
- Tests from the venue's documented payload examples.

Register it (the workspace picks up `venues/*` itself): `hosts/wasm`
(a feature), `sdk-rs/registry` (`safe-cex-venues`: a feature, the `build` match
and the venue's construction rules, with tests in `registry/tests`), `RUST_VENUES` in
`packages/safe-cex/src/exchanges/rust/rust.exchange.ts`, a `venues.mjs` entry,
and `sdk-rs/harness/private-tape-<venue>.mjs` with a root `package.json`
script beside `rust:private-tape`. Extend `replay.mjs` generically; never
break Binance. Declare what the venue answers beyond the common commands in
`Venue::capabilities()` (account identity, client-order lookup, history axes,
cancel-all, leverage reads, symbol-routed commands); the bridge routes from
that record, never from the venue's name.

## Gates (all must pass before you report done)

Toolchain: `export PATH="$HOME/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH" DYLD_LIBRARY_PATH="$HOME/.rustup/toolchains/1.99.0-aarch64-apple-darwin/lib" CARGO_INCREMENTAL=0`, then from `sdk-rs/`:

- `./scripts/check-bans.sh`
- `cargo fmt -p <crate> --check`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`
- `cargo clippy -p safe-cex-wasm --target wasm32-unknown-unknown -- -D warnings`
- From the repo root: `corepack yarn -s rust:build-wasm`, your private tape
  script, then `corepack yarn -s rust:gate --venue <venue> --tape sdk-rs/harness/tapes/<venue>-private.jsonl --runs 1 --books ''` → PASS.
- Public: `corepack yarn -s rust:record --venue <venue> --seconds 20 --books <two symbols> --out sdk-rs/harness/tapes/<venue>-smoke.jsonl`, trim with `rust:trim-tape` into `sdk-rs/harness/fixtures/<venue>-public.jsonl.gz`, and `rust:gate` on it → PASS (identical output, Rust CPU and memory below TS).
- Binance still passes: `rust:gate` on `sdk-rs/harness/fixtures/binance-public.jsonl.gz`, and `rust:private-tape` + its gate.

## Rules

- Never `git stash`; commit work in progress on your branch. Conflicts from
  merging `origin/feat/rust-rewrite` may be resolved without asking, keeping
  the landing branch's structure.

- No JS emulation, transliterated TS method bodies, Axios/BaseExchange-shaped
  facades or `serde_json::Value` (`check-bans.sh` enforces). Typed state
  machines.
- A genuine TS bug is fixed in Rust and listed (a `KNOWN_TS_FIXES` entry in
  `gate.mjs` with a comment, and in your commit message). Never silently
  diverge.
- Short comments, only for non-obvious decisions; match Binance's density.
- Delete what becomes provably unused as you go: helpers in `sdk-rs/crypto`
  left from the first port that no venue calls, and anything your venue
  replaces. Never keep dead code "for later".
- `corepack yarn` only, never `npx`; never `node <script>` directly (use the
  yarn scripts). No load tests.
- Commit on your lane branch with clear messages at stable points. Never
  rebase, force-push or touch `master`.

## Process

1. **Plan-execute, full auto, no human.** Plan the port in phases (public
   data, private data, trading, queries, harness gates). Do extra detailed
   planning only where the venue needs it. At the end of each phase run a
   sub-agent review of that phase and fix what it finds.
2. **Checkpoints.** The overseer sends `/compact` at stopping points; keep
   your plan and progress in commits so nothing is lost.
3. **When every gate passes:** push your branch, open a PR **into
   `feat/rust-rewrite`** (never `master`), run adversarial sub-agent reviews
   locally against it and post the findings on the PR. Address them and
   repeat until you judge it passing or the returns are diminishing. Wait for
   the PR's CI to be green (fix anything red), then merge the PR into
   `feat/rust-rewrite` and cut a fresh branch from it for
   any follow-up.

Report when done: the PR, gate numbers, TS bugs fixed, anything unfinished.
