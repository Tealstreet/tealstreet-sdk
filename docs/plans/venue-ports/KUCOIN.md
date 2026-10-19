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
