# Orderly family Rust port

Branch: `feat/rust-orderly`, created from fetched `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md` from landing.
Full auto; no master, stash, rebase or force-push. PR targets `feat/rust-rewrite`.

## Scope and design

One `safe-cex-venue-orderly` crate under `sdk-rs/venues/orderly` with a typed
broker option. Registered aliases: kodiakfi, mode, salsadex, tealstreet-dex,
what, woofi. All share the existing EVM REST/WS hosts and Tealstreet broker
UUID; regular order tags respectively TEALKODIAK, TEALMODE, TEALSALSA,
TEALSTREET, TEALWHAT, TEALSTREET. Testnet selects its own hosts and broker UUID.
No TS method transcription or generic JSON values. Borrowed typed payloads,
streamed large REST arrays, sans-I/O state machines, ED25519 from crypto.

## Phases

1. Public data: crate, borrowed wire types, market/ticker seeds, stream routing,
   socket watchdog/reconnect, refcounted candles/books and snapshot buffering,
   FetchCandles, native documented-payload tests. Audit official timestamp and
   book continuity contract rather than invent sequence numbers.
2. Private data: exact base58-seed ED25519 signing, startup validation,
   margin defaults/account leverage, positions + balance, normal/algo orders,
   authenticated WS, read reconciliation, stale guards and per-execution fills.
   Preserve simultaneous Cross/Isolated identity through optional compatible
   frame data; ensure failed reads never clear last-good state.
3. Trading: full-batch preflight, caller IDs verbatim on one native order,
   broker attribution, normal and positional TP/SL/brackets with sibling repair,
   strict stable-ID native amend, normal/algo bulk cancellation without repair,
   per-symbol margin/leverage, fixed OneWay. No automatic write retries.
4. Queries: typed documented history/trade/candle endpoints in engine JSON
   result shapes, bounded paging/ranges and honest unsupported surfaces.
5. Integration and gates: wasm and SDK aliases/options, recording/replay config,
   strict signature/body matching private tapes with all broker/testnet variants,
   live read-only 20-second public recording for two books, fixture, correctness
   and CPU/memory gates; Binance and all existing venue regressions; bans, fmt,
   clippy, native/wasm tests, SDK typecheck/lint/tests. Remove provably unused
   replaced helpers and resolve TS removal vs replay oracle without losing gates.

Every phase: plan audit, implement, meaningful checks, independent sub-agent
review, fix and recheck, commit this progress. Deep planning only for actual
Orderly risks: dual-margin identity, REST-proven amend, positional sibling jobs,
and timestamp-only books. Genuine TS bugs are declared in gate KNOWN_TS_FIXES,
commit messages and PR, with independent expected results.

After all gates: fetch/merge landing, revalidate, push, PR, local adversarial
sub-agent reviews posted to PR, fix/review until clear or diminishing returns,
wait every required CI green, merge to landing, cut clean follow-up branch.
Report PR, exact correctness/performance numbers, TS fixes, unfinished work.

## Progress

- Planning: brief/spec and nearest docs read; six aliases and tags confirmed.
- Independent account, trading and harness plan audits in progress.
- Early audit: store keys positions by ID, but frame currently reconstructs IDs;
  optional identity extension required for simultaneous margin modes.

### Public phase

Implemented public crate and typed broker options, streamed market/ticker
seeds, four aggregate streams, refcounted early subscriptions, candle reads
and stream, heartbeat and reconnect, full-depth snapshot buffering with exact
integer predecessor timestamps and generation-correlated snapshot requests.

Official current docs omit the incremental topic; a read-only live probe
confirmed `@orderbookupdate` contains `data.prevTs`. Snapshot `data.ts` is the
same book clock. First buffered update bridges `prevTs <= snapshot.ts < ts`,
then predecessor must equal the last applied timestamp. Gaps/malformed frames
hide the stale book and resnapshot. No floating-point sequence conversion.

Public live sockets reject invented identifiers with normal close1000. The
official js-sdk `packages/net/src/ws/ws.ts` uses COMMON_ID
`OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY`; this identifier works keylessly.
Documented WS `event:request` orderbook snapshots returned full depth (372
bids/356 asks in probe), avoiding TS's private signed REST snapshot in public
mode. Source: https://github.com/OrderlyNetwork/js-sdk/blob/main/packages/net/src/ws/ws.ts
and https://github.com/OrderlyNetwork/documentation-public/blob/main/build-on-omnichain/websocket-api/public/request-orderbook.mdx .

Independent phase review found and fixed startup subscription loss, quiet-book
snapshot starvation, malformed update retaining a stale book, missing candle
timestamps/OHLC bounds, unsupported intervals, and snapshot symbol mismatch.
Re-review found no phase blockers; also tightened null timestamps and missing
depth data. Native tests: 11 passing; fmt/clippy/bans passed. SDK wiring and
record/replay performance checks remain in the integration phase. Genuine TS
book continuity/public snapshot fixes will be declared in gate/PR.

### Private phase

Implemented normalized base58 ED25519 credentials with zeroized seeds and
URL-safe padded signatures over exact REST/WS bytes; validation before private
bootstrap, margin defaults, per-mode leverage caches, positions/collateral and
paged normal/algo orders; authenticated subscriptions, heartbeat, reconnect,
periodic 2s positions and every-fourth-tick orders. Failed reads preserve state.
Private sequences/IDs remain exact integers; executions deduplicate independently
from order-state updates. Fills invalidate pending positions/collateral reads.

Optional appended position-ID references preserve simultaneous Cross/Isolated
identity across frames without changing legacy venue payloads. Reader, types,
FrameApplier and store/spec docs updated; transferred/legacy frame tests pass.

Independent review found/fixed dual status/algoStatus decoding, documented
root lifecycle and average-price aliases, REST freshness stamps, fill-vs-held
positions, root terminal-before-any-child-vs-held-algo-snapshot, and leverage
cache mutation before snapshot validation. Parent tombstones filter subsequent
REST/WS children. Re-review found no remaining phase blockers.

Verification: 22 venue tests (2 signing, 9 account, 11 public); engine44;
host26; crate fmt/Clippy and bans; safe-cex typecheck/lint all pass. SDK constructor
wiring, end-to-end WASM account tests, strict independent signing tape audit,
and account deactivation signal integration remain for the gates phase.

### Trading plan audit

Use sequenced typed jobs with complete batch preflight before writes. Preserve
caller IDs on exactly one regular native order; reject conditional/bracket/split
IDs and duplicate IDs as a batch. All writes carry explicit mode and correct
broker UUID/order tag; plain TIF maps to native LIMIT/POST_ONLY/IOC/FOK.
Native amendments require complete REST proof and one precision-aligned real
change, with EDIT_SENT success and stable ID. Positional replacement cancels
its parent directly and posts one pair preserving only the opposite sibling;
child cancellation groups selected siblings by root before rebuilding survivors.
Bulk normal+algo cancellation attempts both independently and never repairs.
Margin selector writes affect defaults only; leverage targets selected mode;
position mode is fixed OneWay. No write retries; retain partial accepted IDs
if later sequenced steps fail. Review after native command/error/race tests.

### Trading phase

Implemented fully preflighted typed sequenced jobs, exact bounded quantity
splits, verbatim ASCII client IDs on one regular order, broker attribution,
native TIFs, strict REST-proven stable-ID amend, dual-mode settings and
per-market leverage bounds. Positional replacement preserves only the opposite
sibling and waits for old-parent terminal state or a fresh complete snapshot.
New pairs wait for all authoritative children before another protective job.
Uncertain writes retain scoped fences until a fresh successful snapshot;
pre-write snapshots cannot release them. Bulk cancellation preempts queued
placements and pair waits, attempts both families, and never repairs siblings.
No write retries. Partial acceptance reports exact accepted IDs.

Independent review rounds found and fixed terminal-before-ACK resurrection,
unconfirmed-pair duplication, duplicate bracket scopes, global Cross leverage
clamping and Isolated fallback mutation, uncertain Pair response loss, and
bulk cancellation arriving before a Pair ACK. Final review has no blockers.
Tests also cover stale snapshots failing to release uncertain-write fences and
fresh reads restoring progress. Verification: 38 venue tests (16 trading),
crate fmt/Clippy and bans pass. TS differences (TIF, split precision, child/root
response shapes, sibling repair, cumulative fills and reconciliation) will be
explicit KNOWN_TS_FIXES with independent tape oracles in the integration phase.

### Queries plan audit

Implement documented rare reads for fills, ordinary+algo completed orders,
settled funding, and public market trades. Typed streamed rows, bounded page
continuations, exact IDs, strict range/symbol/order filters and deduplication.
Failed or malformed pages fail the whole command without changing account state.
Use direct order-trades endpoint for order-specific fills so latest500 account
trade retention does not hide an older order. Funding exposes settled Pay as
negative and Receive as positive; other income types fail explicitly. Match
Binance JSON shapes, including seconds execution/income/public timestamps and
millisecond historical createdTime/updatedTime. Public rows without native IDs
use a deterministic content key plus identical-row occurrence discriminator.

### Queries phase

Implemented streamed fills (including direct order-trades lookup), ordinary and
nested algo terminal history, settled funding and public trades. Order history
omits the API's COMPLETED filter so rejected orders remain visible; Open rows
are filtered locally. Output units follow Binance. Bounds: 50 pages, 500 rows
per page, 1,000 results, algo depth8 and 1,000 nodes per root. Malformed/missing
rows, repeated pages and failed later reads fail the complete command. Public
trade IDs preserve identical same-time executions using occurrence suffixes.
Funding starts are paced across concurrent commands and rechecked when delayed
host timers fire; Pay is negative and Accrued rows are excluded.

Independent review found/fixed delayed-timer funding bursts, nested bracket
traversal, root status overwriting explicit child status, and opening bracket
entries accidentally defaulting reduce-only. Final re-review has no blockers.
Verification: all17 query tests and crate Clippy pass. SDK/harness integration
is underway; its first public gate exposed broker-suffixed symbols and the
obsolete aggregate funding topic, to be corrected before counting gates green.

### Integration plan audit

Ship all six aliases through production registries and a default WASM loader;
retain the former implementation only in __tests__/reference for independent
replay. Conditional package exports must keep Node filesystem glue outside the
browser graph. Preserve applicationId, tick cadence, broker UUID/tags, account
identity and startup rejection/disposal lifecycle. Honor selected REST routing
at the host boundary; never silently send routed credentials directly.
Use strict independent ED25519 verification and exact ordered bodies in tapes;
correlate public snapshot responses by taped semantic request, symbol, socket
generation and opaque-ID occurrence. Run actual live two-book20s recording,
all alias/testnet private gates, existing venue regressions, and full checks.

### Integration checkpoint before landing merge

WIP: production six-alias registry cutover, test-only legacy replay reference,
conditional WASM loader, explicit deactivation event, scoped host routing,
strict signed private tapes and guarded independent TS-fix oracles are present.
Broker suffixes now preserve native identity. Funding moved to dedicated
90-topic sockets after live evidence of the venue's100-topic/socket limit.
Public shard tests16 and query tests17 pass; harness96 and transport10 pass.
SDK alias/private/lifecycle WASM tests15 pass; typecheck/lint pass. Full SDK
suite had6567passes and six expected registry-characterization failures;
those six snapshots were updated and targeted49tests now pass.

No final gate has passed yet. Remaining: verify new funding topology on a fresh
20s recording, finish native/main/private handshake watchdog and main-topic
capacity checks, propagate request abort through the actual host so deferred
proxy authorization cannot send after disposal, preflight unsupported socket
routes, rerun private gate with current query build/oracles, fit combined WASM
within existing3MiB budget, run all gates/regressions/CI and adversarial reviews.
Current public fixture is an intermediate recording and must be replaced.
Unused hmac_sha512 helper removed; spec-required P5a signing primitives retained.
Legacy built-in HOW tunnel cannot preserve signed bytes and currently fails
explicitly; raw custom transport/proxy routes preserve bytes. Document/review
this limitation before shipping.

### Landing refresh (c5bc2c4adf)

Merged the landing branch after the quota interruption, preserving Bitget and
Hyperliquid, strict harness request/write accounting, and optional per-venue
WASM builds. Orderly now participates in those feature builds; the landing
per-venue budget replaces the obsolete aggregate-size workaround above.
The new shared client-order lookup query fails explicitly for Orderly.
Locked native checks of the Orderly crate and complete WASM host pass.
Remaining integration and lifecycle gates above remain open.

### Lifecycle review checkpoint

Independent reviews cleared initial/reconnect handshake deadlines on main and
private sockets, duplicate-open guards, the 97-topic main socket capacity,
and host cancellation across authorization/fetch/stream consumption. Unsupported
socket routes now fail synchronously during SDK startup, including testnet.
Verification: 65 native Orderly tests, Orderly Clippy, full-workspace format,
WASM host Clippy, 19 host tests and 23 SDK transport tests pass. The complete
host suite needs the combined build (its Binance Workers test correctly rejects
an Orderly-only binary). The single-venue WASM is770,916bytes below1,258,291.
Fresh20s public tape captured637socket frames and two full books without topic
errors; first gate has only legacy zero-opening-price NaN percentage diffs.
Strict guarded private oracles are being updated for landing optional fields
and newly enforced request-set accounting; no replay gate counted green yet.

### Public and private replay checkpoint

Fresh20s fixture now passes all three public replay runs:639inputs,144markets,
144tickers,two full books. Median main CPU TS179.1ms/Rust117.6ms, steady
TS140.3ms/Rust82.0ms, retained TS2.0MB/Rust1.4MB (Rust linear0.7MB).
Private base replay passes with33inputs,two markets,14commands; all broker
mainnet/testnet variants are running before final completion.175harness tests
pass. Added declared TS fixes for zero-opening-price NaN percentages and
explicit open-order pagination, plus independent exact read accounting for
legacy query stubs. Account comparison now canonicalizes object keys while
preserving every row/value and array order.

Full SDK suite:6644tests/459files passed. SDK real Orderly WASM:15passed.
Review fixes: shared startup promise prevents duplicate/orphaned hosts;
disposal while loading rejects; unsupported Orderly history ID/status filters
reject before reads.48focused SDK tests, typecheck/lint and87metadata/account
construction tests pass. Built-in HOW capability is no longer advertised.
Test drivers for Binance/Bybit/Bitget explicitly fail on unexpected Deactivate
notices, retaining exhaustive checks after the shared lifecycle addition.
Remaining: combined host/SDK WASM tests, workspace checks, all broker variants,
mandatory Binance replay regressions, final landing refresh, PR reviews and CI.

All14broker/network private gates now pass. Combined host32tests and SDK
WASM40tests pass. Follow-up independent review closed prompt disposal of a
never-settling WASM loader (49focused SDK tests plus typecheck/lint pass).
Landing advanced tofc18c55874 with shared transport routing and Bitrue;
merge that structure before final workspace/Binance regression checks.
