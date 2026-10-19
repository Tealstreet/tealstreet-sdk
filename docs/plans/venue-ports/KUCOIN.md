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

### Query/risk boundary follow-up

Independent final review passes. Added symbol-required itemized funding through
`/api/v1/funding-history`, 60-day windows, inclusive time overlap/exact ID dedup,
explicit incomplete/same-ms saturation errors, failed-page rollback, and shared
350 ms query pacing (11 funding tests). Pending settlement offset -1 is excluded
before parsing completed ledger cursors. Lot-to-base boundaries quantize to the
contract multiplier so 3 lots at 0.1 remain exactly 0.3, matching TS safe math.
Native crate: 67 tests pass; clippy all-targets with warnings denied passes.

### Harness gates before latest landing merge

- Captured a real 20-second public session, both books; committed compressed
  first-20-second fixture has 16,154 frames, 692 markets/tickers, two full books.
  Trimmer `--http-limit 0` retains all cached snapshots needed for gap recovery.
- Three-run public medians TS/Rust: CPU 907.3/722.1 ms, steady CPU 663.0/487.9 ms,
  retained 6.2/2.4 MB. Correctness/protocol/request gates PASS.
- Private fixture: 76 records, 17 commands including native bracket/stops,
  cancel/replace/both cancel books, leverage/margin, blocked mode switch, candles,
  all history surfaces and dedicated funding. Independent JSON oracles only
  replace the four missing TS query methods; no mutation result overrides.
- Independent phase-5 review passes. HMAC/passphrase, token URLs/private topics,
  subscription caps and detail channels are independently audited. Node 24
  harness 13/13; wasm integration 17/17. Root typecheck (44 tasks), lint (28),
  tests (27 tasks, safe-cex 6,495 tests per run) pass. Bans/wasm clippy pass.
- Binance public/private, Bybit public/private, Crypto.com public/private and
  OKX public regression gates PASS before merging the newly landed WEEX lane.
- Fixed a flaky existing memory unit oracle: a retained ArrayBuffer can coexist
  with a negative heap delta after GC. Deterministic negative-heap coverage keeps
  the runtime Wasm counting test; production metric and gate remain unchanged.

Next: merge latest landing (WEEX), rebuild and run all landing replay gates,
then PR, adversarial reviews posted on PR, green CI, merge and follow-up branch.

### WEEX landing integration checkpoint

Merged landing 6cd219ce4c with both venue registrations, broker/upgrade options,
header validation and timed replay templates intact. Merge review found the new
unconditional `SetTickerSymbols` startup hint reached KuCoin's trading fallback;
added a no-op (its complete snapshot pool is already subscribed) and native
before/after-start regression. Crate 68 tests and wasm integration 18 tests pass;
workspace tests/clippy and package typecheck/lint pass. Combined six-venue wasm
was measured at 2,457,294 bytes before the tiny hint handler; budget 2.5 MiB.

Funding tape wiring was corrected from `kind` to the wasm command's `incomeType`
field; the dedicated funding command now matches its independent expected row.
KuCoin private gate PASS at 32 frames/17 commands. The public replay exposed a
remaining TS startup/book race after the landing harness change; investigation is
active and that gate is not counted passing at this checkpoint. The landing
branch has since advanced through BingX and a startup-subscription harness fix;
merge that next, then rerun the complete gate set before PR publication.
