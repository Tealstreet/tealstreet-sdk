# KuCoin futures Rust port

Branch: `feat/rust-kucoin`, based on `origin/feat/rust-rewrite` (wave 2).
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.
Full auto: phase plan/audit, implementation, verification, independent sub-agent
review, fixes, focused commits. Never stash/rebase/force push/touch master.

## Scope and design audit

Classic linear futures (`kucoin`). Spot has separate HF endpoints, symbol and
account semantics; sharing HMAC/token primitives alone does not justify a second
venue port. UTA is deliberately inactive, consistent with the TS runtime.
Typed sans-I/O state machines with borrowed serde rows; large lists stream.
Dynamic bullet endpoints supply each connection's URL, token, and ping timings.
Public snapshot subscriptions shard below 400 topics; full-depth books buffer
and recover contiguous sequences against REST snapshots. Native lot quantities
use each market multiplier exactly once at the engine boundary.
API-v2 headers sign the raw query endpoint plus exact body; passphrase is HMAC
signed too. Private tokens authenticate subscriptions without an extra login.
Reads can retry; ambiguous mutations cannot. Reconciliation retains good state
on failures and fences stale snapshots/terminal orders. Caller IDs preflight the
whole batch before writes, replacements mint new IDs.

## Phases

1. Public: crate, signing, markets/tickers, bullet socket pool and watchdogs,
   sequence books, REST/WS candles, documented-payload tests. Review then commit.
2. Private: paginated normal/stop orders, positions/balance/settings/mode,
   stream fills and reconciliation race guards. Review then commit.
3. Trading: typed sequenced placement/brackets/stops, cancel/replace, settings,
   client IDs and rejection/retry contracts. Review then commit.
4. Queries: fills/orders/income/public trades in engine JSON shapes, bounded
   paginated reads and exact conversion. Review then commit.
5. Gates: wasm/TS registration, recorder/private tape and independent signature,
   token/subscription audits; live 20-second two-book fixture; correctness and
   CPU/memory gates; Binance regression gates; CI registration. Review and commit.

## Verification and landing

Per phase: crate fmt/clippy/tests and relevant TS checks after integration.
Final: bans, wasm clippy/build, private replay, public replay (Rust CPU and memory
below TS), Binance public/private, workspace/integration checks. Record actual
numbers here. Merge latest landing into lane, push, PR into feat/rust-rewrite,
adversarial sub-agent reviews posted on PR, address/review, green CI, merge PR,
then fresh follow-up branch from landing. No private credentials required:
private validation uses independently documented synthetic payloads.

## Progress

- Planning: read current remote brief/spec and TS contracts; interfaces agreed
  between public/private/trading modules. Parent owns queries/harness/landing.

### Core phases implemented and reviewed

Public: dynamic bullet endpoints, sharded snapshot pool (380 topics/socket,
100 symbols/frame), native ping deadlines/fresh-token reconnects, pre-seed
subscription reference counts, full-depth sequence recovery and REST/WS candles.
Private: streamed positions and paginated normal/stop books, balance, modes and
settings, per-execution fills, read retries, revision/timestamp/closure guards.
Trading: sequenced jobs, native lot placement, st-orders brackets, standalone
full/partial stops, cancel+replace with fresh IDs, both cancel families, cross
leverage/margin/mode commands, headless metadata reads and per-side leverage.
Queries: fills/orders numbered pages with census, exact ledger offset paging in
one-day windows, seven-day order/fill windows, recent public trades, typed JSON.

Independent phase reviews: trading agent reviewed public and queries; public
agent reviewed private; private agent reviewed trading. Findings fixed and
regressed: generation collisions, partial position risk resets, stale closures
and reopened positions, stale metadata after writes, zero-rounded replacement
prices, isolated per-side leverage, query parameter injection and one-ms ledger
windows. Native tests/clippy pass at the core checkpoint (ongoing harness adds
integration coverage). Optional broker signed headers and signed passphrase have
independent HMAC oracles. Crypto audit found no newly unused first-port facades.

Known TS corrections (also listed in gate.mjs): complete paginated order reads;
failed/delayed account reads retain newer state; correct side-dependent stop
classification and trigger/close flags; execution deduplication; reject silently
dropped conditional fields and incompatible bracket references; cancel-symbol
covers stops too. Missing TS futures history surfaces use independently declared
documented-payload oracles. Live replay found a TS buffered-book suffix loss on
cached snapshot gaps; a production reference fix and deterministic regression
are being verified. Private validation uses synthetic documented payloads.
