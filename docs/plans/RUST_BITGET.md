# Bitget Rust port

Lane: `feat/rust-bitget`. Spec: `RUST_REWRITE.md`; process: the venue port
brief on `origin/feat/rust-rewrite`. Full automatic execution, phase review by
sub-agent, commits at stable points. Target is USDT-M first, USDC shared paths;
inverse only where its sizing and collateral behavior can be verified.

## Phases

1. Public: borrowed wire records, streamed markets/tickers/candles, three
   ticker sockets, ref-counted books/candles, snapshot/update sequencing and
   checksum validation, reconnect and ping watchdog. Register crate and wasm.
   Verify bans, fmt, clippy and documented-payload public tests; sub-agent audit.
2. Private: exact REST/WS signing, Classic/UTA detection and subscriptions,
   typed account snapshots, position/settings mapping, failed-read preservation,
   terminal tombstones and fresh-WS guards, polling and event reconciliation.
   Verify signing, snapshot races, reconnect, empty/error responses; audit.
3. Trading: sequenced jobs, whole-batch client-id preflight, Classic batches,
   UTA orders/strategy orders, sized vs position stops, brackets/trailing,
   amend with fresh newClientOid, cancel and account settings. Never retry an
   ambiguous write. Verify payloads, partial failures and lifecycle; audit.
4. Queries: paged executions, order history, income and public trades as rare
   JSON command results. Bound ranges/cursors and dedupe executions. Audit.
5. Harness/gates: generic replay requests including JSON bodies and commands,
   documented private tape, real 20-second two-book public fixture, Bitget
   correctness/CPU/memory and Binance regression gates, wasm clippy/build,
   repository checks. Audit and record actual measurements.

## Risks and decisions

- Classic v2 public streams stay shared across Classic/UTA private accounts.
- A sized TPSL is clamped to the protected side, floored to the lot; zero is
  a full-position sentinel. Cancel uses immutable native plan type.
- Native modify-order sends both price and size plus a fresh client id.
- Successful error envelopes and malformed streamed records must not clear
  last-known-good account state.
- Books must not publish updates before a snapshot or across a sequence gap.
- TS cumulative fill events violate one execution per fill; any intentional
  correction needs an explicit gate entry and commit explanation.
- Keep TS available to the replay oracle until the migration integration decides
  deletion; never weaken parity or performance gates to make a tape pass.

## Progress

- Read the spec, existing engine/Binance pattern, Bitget plan-type and client-id
  guidance, TS Classic/UTA startup, public and trading behavior.
- Fetched origin and read the updated Process section. Existing lane is clean.
- Public implementation: crate + workspace/wasm/SDK/harness registration,
  USDT/USDC product selection, streamed contracts/tickers/candles, three public
  sockets, refcounts, sequence/checksum recovery, 5s ping / 20s watchdog.
- Public sub-agent audit found string sequence decoding, missing snapshot sides,
  pre-open ping, malformed WS candles, and REST mark-price parity issues. Fixed
  all; exact sequence, malformed-data and watchdog regressions are covered.
- Native public checks: bans PASS; fmt PASS; clippy all targets PASS; 11 tests
  PASS. Repository typecheck/lint/tests and wasm clippy are running.
- TS defect corrected: books previously applied gaps without validating modern
  seq/pseq (and legacy checksum). Register this in KNOWN_TS_FIXES at harness
  phase; normal tapes must still compare identically.
- Next: private state/signing and reconciliation.

## Final integration

After all gates: merge `origin/feat/rust-rewrite` into lane (stop on conflicts),
re-run affected checks, push, PR to `feat/rust-rewrite`, adversarial local
sub-agent reviews posted to PR, fix/review until clean or diminishing returns,
merge PR, create fresh follow-up branch. Never touch master or force/rebase.

- Workspace relocation: adopted landing members glob and moved Bitget to
  `sdk-rs/venues/bitget`; host uses its direct path dependency. Overseer gave
  standing authorization for landing-branch layout conflict resolution.
- No further stashes: use branch WIP commits. Private-phase stash is restored
  after the relocation merge. Runtime-tree wasm test failure is overseer-owned.

## Private phase checkpoint

- Exact REST/WS HMAC signing; Classic/UTA detection and late 40085 promotion;
  separate private socket authentication, subscriptions and watchdog.
- Streaming nested arrays retain small validated envelopes for cursors and
  business errors. Binance's original streaming form remains unchanged.
- Typed snapshots, settings, position mode, liquidation fallback, full-snapshot
  and per-position freshness guards, terminal-order tombstones, open-order
  grace, virtual-stop polling, account reconciliation and bounded fill caches.
- UTA fill/strategy topics, product filtering, dual strategy leg identity and
  status-less strategy snapshots reflect current documented native payloads.
- Sub-agent review findings fixed and covered by regressions: late promotion,
  old HTTP generations, socket replacement, UTA scope, execution semantics,
  missing strategy status, TP/SL ID collision, hedge-close direction, virtual
  refresh strategy preservation, funding fee alias, map pruning.
- Removed unused first-port `crypto/wallet_utils.rs` and its export; no native
  venue, host or engine caller exists. Retained the signing primitives.
- Bans/fmt/clippy pass; engine 12 tests, Bitget public 11 + private 15 tests pass.
- TS fixes are explicit in `harness/gate.mjs`'s KNOWN_TS_FIXES registry; normal
  Bitget tapes receive no mismatch normalization. Next: trading commands.

## Trading phase checkpoint

- Sequenced Classic/UTA write jobs: preflight caller IDs across the entire
  batch, native bracket and stop payloads, max-lot splitting, batch limits,
  immutable plan types, sibling-aware cancel, leverage/margin/mode settings.
- Native modify uses fresh newClientOid and both size/price. Resolve asynchronous
  identity through the new client ID, respecting terminal detail and fresh WS.
  Failed batch cancellations block replacement; acknowledged partial successes
  remain terminal. Only explicit known rejection permits cancel-and-replace.
- Unknown transport/server write outcomes are never retried or rolled back.
  Reads retry boundedly; signature rejection is re-signed once. UTA strategy
  replacement preserves both legs and rolls back only a known failed placement.
- Post-write private grace triggers Classic fallback and full REST polling;
  literal pongs do not count as authenticated payloads. Old settings snapshots
  cannot overwrite acknowledged leverage/margin/mode changes.
- Sub-agent review found and fixed partial-cancel handling, async modify identity,
  terminal/snapshot races, sibling cancellation, UTA client-only identity and
  full-position amend, lot-induced caller-ID splits, and partial-write health.
- Bans/fmt/clippy pass; public 11, private 15, trading 23 tests pass. TS defects
  recorded in KNOWN_TS_FIXES: lot splitting lost quantity, Classic cross wire
  spelling, stale modify identity, unsafe fallback after unknown write outcomes.

## Query phase plan

Stream typed fills/history/bills/trades lists into rare JSON results. Follow
native cursors even on short pages, dedupe identities, split accepted time
windows, bound page/window counts, and fail loudly on incomplete/error reads.
Classic hedge fills derive execution direction from position side + tradeSide;
UTA uses execution direction and the documented positive charged-fee convention.
Bill rows emit gross PnL and signed commission separately with distinct IDs.
UTA bills remain explicitly unsupported rather than calling incompatible v2.
Verify documented samples, huge IDs, fee signs, hedge closes, cursor cycles,
range boundaries, malformed pages and retry failure; sub-agent review afterward.

## Query phase checkpoint

- Streamed Classic/UTA fill and order history, Classic account bills, public
  trades, typed Binance-compatible JSON results. Native cursors continue across
  short pages; empty terminal pages end cleanly. Cycles/page caps fail loudly.
- Inclusive 30-day windows, identity dedupe, execution timestamps in seconds,
  order/income/public-trade timestamps in milliseconds. Classic hedge-close
  direction, signed rebates, and separate gross PnL / commission bill rows.
- Explicit UTA income unsupported error; no incompatible Classic fallback.
- Query sub-agent review fixed filtered malformed rows, invalid financial
  numbers, missing required Classic profit, unbounded detection wait, and
  query-triggered late UTA promotion. Shared promotion also re-plans explicitly
  rejected trading jobs before any accepted placements; its wait is bounded.
- Native checks: 62 tests (public 11, account 15, trading 24, queries 12),
  fmt/clippy/bans PASS. WASM clippy and release build PASS.
- Harness work started: strict JSON body comparison preserving caller IDs,
  subscription routing for same-URL sockets, passphrase options, documented
  Classic private tape. Initial Bitget private and both Binance gates PASS.
  Public 20-second two-symbol recording captured; fixture gate running.
