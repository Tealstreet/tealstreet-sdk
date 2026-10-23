# Rust port quality standards

Read-only Opus audits of `feat/rust-rewrite` (2026-10-20) for patterns we can
obviously improve without regression. The client contract outside safe-cex may
change where argued; each item names the tealstreet-next callers it touches.
Strike items as PRs merge. Trading-semantic changes are marked **risky**.

## Bugs found (fix now, ahead of the standards)

- ~~Command rejections on Rust venues never emit `'error'`
  (`rust.exchange.ts:857-868`), so the app shows no toast unless the venue also
  logs. Emit once per rejection, de-duplicated against a same-frame `Log Error`.~~
  Each rejection emits one `'error'` carrying its `ExchangeError`.
- ~~Lost-send recovery never runs for Rust: `isUnknownOutcome`
  (`utils/unknownSendOutcome.ts:3`) does not match the Rust "outcome unknown"
  texts (lighter, coinbase, txflow, orderly, bybit). **Risky** (enables lookups
  and chaser decisions).~~ Venues classify `outcome`; the host builds an
  `ExchangeError` from it and `isUnknownOutcome` honours it before text matching.
- ~~A bad key can hang `start()`: Bybit retries startup forever
  (`bybit/src/lib.rs:734-745`) and `untilLoaded` has no failure path
  (`rust.exchange.ts:795`). Needs a terminal startup failure.~~ Engine
  `StartupFailed{error}` rejects `start()`; Bybit fails startup on definitive
  key rejections (10003/10004 after resend/10010/33004) of its signed fee read.
- Secrets in logs: ~~53 log sites interpolate raw bodies (broader sweep remains).~~
  Swept: direct body interpolation is gone. Non-2xx `HTTP {status}: {body}`
  failures keep the venue's error envelope as an actionable message. Every
  private replay now fails if any log line or command result quotes a replay
  credential or a signed query (`harness/notice-leaks.mjs`).
  ~~Bitrue logs whole private frames (`bitrue/src/account.rs:700`), Bybit logs
  the signed request body (`bybit/src/lib.rs:550`); native transport errors
  include signed URLs (`hosts/native/src/transport.rs:390,404`).~~ Fixed at
  private diagnostic/signing and native connection-error boundaries, with
  all-notice credential/URL regressions. Current reqwest body-stream errors
  already omit URLs; that path is regression coverage, not a reproduced fix.
  Ordinary business rejection messages remain actionable.
- ~~Native and Durable Object hosts drop logs with no client attached.~~ Fixed:
  Warn+ goes to stderr/console with no client, and log notices are cut to fit
  before broadcast. ~~An oversized native broadcast disconnects every peer
  (`server.rs:386`).~~ An oversized delta frame is sent as ordered segments
  that share its string table; other oversized notices are replaced by a bounded
  warning, and an oversized completion settles its command with an error. Only
  a single section larger than the limit still makes peers resnapshot, and that
  is logged.

## Error handling

Today: every engine error is a bare string; venue codes are discarded; ~40
hand-written formats; ~95 parse failures collapse to `malformed = true`;
partial-batch detail is JSON inside the error string (`host.mjs:306-317`);
consumers reclassify with regexes (`health/errorPatterns.ts`, app toasts,
`ChaserOrderManager.ts:550`).

Standard (`engine/src/error.rs`):

```rust
pub struct VenueError {
  pub kind: ErrorKind,  // Rejected | InvalidInput | AuthInvalid | AuthExpired | AuthPermission
                        // | RateLimited | Network | Timeout | VenueUnavailable | Unsupported | Internal
  pub outcome: Outcome, // NotSent | Rejected | Unknown
  pub venue_code: Option<String>, pub venue_message: Option<String>,
  pub message: String,  // user-facing; contains venue_message verbatim
  pub retry_after_ms: Option<f64>,
  pub partial: Option<Partial>, // placed ids, failed index, total
}
```

`CommandFailed` carries it (`Cx::fail` is the only failure path; `Cx::done`
is success-only), `Deactivate{error}` and a terminal
`StartupFailed{error}` replace the reasonless deactivate and the BreakoutProp
prefix match. The wire carries the object. TS exposes `ExchangeError extends
Error` with `kind`, `category` (1:1 onto `ErrorCategory`), `outcome`,
`venueCode`, `venueMessage`, `retryAfterMs`, top-level `partialOrderIds`,
`failedOrderIndex`, redacted `debug`. Regex classification stays only as the
TS-venue fallback.

PRs: ~~(1) types and plumbing, byte-identical messages, URL redaction~~
(landed: `engine/src/error.rs`, `Cx::fail`, host `detail` beside the unchanged
`error` text, partial batches as fields, TS `ExchangeError`); ~~(2) one
shared parser per venue family, replacing `malformed` flags~~ (every venue
classifies `kind`/`outcome`/`venueCode`; the later Bitunix port now also uses a
strict command-response parser, conservative write outcomes and partial batch
detail, rather than logging item failures followed by success);
~~(3) TS toast on rejection~~;
~~(4) `outcome: Unknown` at unknown-outcome sites (**risky**)~~; ~~(5)
`StartupFailed`, per-venue auth codes (**risky**: ends infinite retries)~~
(validateAccount carries the startup `kind`; the `'error'` event carries
`classified` and Sentry tags `error_kind`/`error_category`/`error_outcome`/
`venue_code`); (6) app and chaser read `kind`/`outcome` (tealstreet-next);
~~(7) wasm panic disposes the engine and surfaces `Internal`~~
(`EngineDeadError` carries `kind: 'internal'`).

tealstreet-next: `exchangeAccountController.ts`, `createLimitOrder.ts`,
`createMarketOrder.ts`, `createScaledOrder.ts` (reads `partialOrderIds`),
`errorToast.tsx`, GRVT builder handler, CLI and exchange-mcp printing.

## Logging and observability

Today: one path, `cx.log(level, String)`, ~460 venue calls (Error 286, Warning
169, Info 4), no Debug, no context, no counters, no redaction. Retried
transients log at Error (toast, Sentry, `console.trace`); some child-leg
failures log at Warning (no toast). `ExchangeHealthMonitor` and the app match
message wording, which makes Rust message text an unversioned API.

Standard: `Notice::Event { level: Debug|Info|Warn|Error, code: &'static str,
message, fields }` with stable dotted codes (`ws.reconnect`, `auth.rejected`,
`snapshot.discarded`, `order.rejected`, `rate_limited`, `child_leg.failed`), a
`Redacted` field type and a 512-byte cap in `Cx`, never raw bodies. Host adds
venue and account. Hand-rolled JSON, no `tracing` crate (wasm size). Host-set
minimum level so Debug costs nothing. Engine `Health` counters (ws opens and
closes, reconnects, auth failures, HTTP failures, rate limited, snapshots
discarded, rows skipped, last private message) exposed via `getHealth()`.
Hosts forward events and write Warn+ to stderr/console with no client.

PRs: ~~(1) facade, wire, `Health`, `cx.log` shim, host events on ws close and
HTTP failure, no-client fallback~~ (`engine/src/log.rs`: `Cx::event`,
`logLevel` engine option, `EngineHost.health()`/`onEvent`, native `event` notices,
DO coded event notices); ~~(2) redaction sweep with a test that fails on
secrets in any notice~~ (every private replay checks every notice); ~~(3) engine-level ws and auth counting~~ (reconnects per socket role, engine-initiated closes, `deactivate` and auth-kind failures, last private frame); ~~(4) venue batches assigning codes and fixing levels (keep message text stable)~~ (every venue emits coded events; retried reads use `Cx::retried_failure`: a repeat of the same failure is a warning, re-raised as an Error every 90 s and whenever the text changes; optional child-leg failures are Errors); (5) ~~SDK health monitor on codes and getHealth()~~ (structured event/legacy
metadata bridge; authoritative counters separate from observed reports);
tealstreet-next callers remain cutover items in RUST_CUTOVER_CALLERS.md; ~~(6) delete the shim~~ (engine/startup codes; one Level/Event path across hosts).

tealstreet-next: `exchangeAccountController.ts:180-383`,
`apps/cli/src/exchange-manager.ts:203`, `apps/cli/src/error-classifier.ts`,
`safeCexTelemetrySink.ts`, `exchange-pretty-error-messages` bybit and bingx.

## Shared engine helpers

Low risk, do first:
1. ~~`engine::de` numeric decoders: 77 hand-written helpers, copies already
   diverge (Binance malformed to 0, Hyperliquid trims, Kraken/WEEX/Coinbase
   reject blanks). Named strict and lenient variants; pin current behaviour
   per venue, then migrate in batches.~~ Done: `engine::de` and
   `num::parse_decimal`; every venue migrated.
2. ~~`engine::url`: one RFC 3986 encoder copied in 7+ venues, plus
   `encodeURIComponent` and axios dialects; 5 `Query` types. Signing vectors
   settle it.~~ Done: `engine::url` dialects and `Query`; Kraken's form
   spelling and AsterDEX's raw-value query stay venue-specific.
3. `engine::backoff::Backoff`: ~~~40 inline exponential formulas, no jitter.
   Migrate with constants unchanged, then jitter~~ (done: one schedule shared
   with `engine::socket::Supervisor`, reconnects jittered); ~~then one
   startup-retry policy (fixes startup fail-closed on OKX, BloFin, Bitget)~~.
   Public discovery already retries; `startup::PublicRetries` now reports one
   typed-cause `startup.degraded` warning after six failures, keeps capped
   retries running, and emits `startup.recovered` once markets and tickers load.
   Aggregate empty OKX catalogues retry too. Private reads, Bybit and
   post-loaded recovery retain their policy. See [the plan](RUST_STARTUP_RETRY.md).
4. ~~`engine::time`: ISO/civil-date math in 9 venues.~~ Done: civil math,
   month arithmetic and ISO formatting shared; venue text grammars stay local.

Medium:
5. ~~`SnapshotBuilder<T>` with `push`, `reject(id, err)`, `commit`: a rejected
   row keeps the store's copy. Structurally ends the one-bad-row-discards-the-
   snapshot class (~50 hand-rolled `invalid` flags).~~ Done:
   `engine::coverage::SnapshotBuilder` (#1459, #1493, #1517, #1524) isolates
   private snapshot rows per identity across venues. A rejected row keeps its
   stored copy and logs once (Error when none is kept); snapshots still hold
   while discovery or the catalogue is incomplete, and structural failures stay
   fatal. Skips are coded `row.skipped` notices.
   Financial freshness now also has one engine boundary:
   [RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md](RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md).
   Issuer-bound checked epochs and linear Store tokens admit native cache effects
   and rows together for Binance/Bitunix; native semantic fences and Bitunix WS
   healthy-peer progress remain. This first core does not claim metadata or all
   venue adoption, nor safe RequestContext late-ACK patches.
   Standardization continues in [RUST_ROW_ADMISSION.md](RUST_ROW_ADMISSION.md):
   `RowAdmission` now owns bounded coverage, unknown/overflow conflict policy,
   silent page/stream merge and coded reporting; `SnapshotCoverage` remains its
   compatibility alias. Bybit and Bitunix use the shared collector without
   changing their scoped freshness/lineage rules. Existing helpers' presence
   does not prove every keyed REST/WS call site is migrated: Bitget existing
   REST/WS and Orderly REST/WS now use shared admission/reporting. Orderly WS
   validates complete containers, prepares each native row without mutation and
   admits it before root/fill/store effects, so bad rows preserve their copy while
   healthy peers commit. Remaining keyed consumers still need inventory/fan-out;
   validated cumulative-fill preparation now starts with `engine::execution`
   and Hyro/Bitget Classic reference consumers, repairing five original-RED
   financial defects before progress/dedup commit. See
   [RUST_CUMULATIVE_FILLS.md](RUST_CUMULATIVE_FILLS.md). The engine now owns
   observed/delivered/position suffix separation and bounded priced intervals;
   WOO/Bitrue adopt it with five more original-RED/core-GREEN controls. An
   exclusive borrow keeps prepared interval decisions current until consuming
   commit, which precedes Fill emission; aggregate VWAP cannot price a clipped
   subset. Bitunix and Coinbase(+spot) now share checked average/direct-suffix
   quote preparation; emitted rounding preserves unrounded anchors. Native
   compatibility controls cover equal-endpoint quote refresh, invalid-value retry
   and Spot/INTX/CFM units. BloFin now shares prefix admission, capped-decimal
   arithmetic and scaled public execution preparation, with two original-RED/
   core-GREEN controls preventing infinite-price delivery and lost correction.
   Historical observations still bypass unused live-price preparation. Kraken
   Spot now uses the existing direct/cumulative preparation path before row
   state effects; four original-RED/core-GREEN controls preserve corrected
   delivery and isolate rejected final rows without a separate ledger.
   Zero/no-growth, snapshots, lifecycle observations and native ID policies stay
   explicit. Remaining cumulative consumers need fan-out before reporting
   and snapshot freshness, rather than independent venue repairs. Snapshot commits retain the
   current protected rows; venue freshness checks precede accepted-row writes.
6. ~~Per-venue `from_options` in a `safe-cex-venues` crate shared by hosts;
   delete ~130 lines of TS credential checks (`rust.exchange.ts:431-645`);
   route `tickInterval` to every venue.~~ Done: `sdk-rs/registry` builds every
   venue for the wasm and native hosts and owns the credential rules (#1467,
   #1469, #1476); TS sends one uniform options blob (#1519); the virtual-clock
   venue list became the `hostClock` capability (#1520), now retired by the
   [shared engine clock ingress](RUST_ENGINE_CLOCK.md); polling is the runtime
   policy (#1392). AsterDEX version inference is shared by native construction
   and registry metadata; the bridge forwards only explicit overrides and reads
   the native version after WASM loads. Its synchronous pre-host fallback stays
   under a compiled TS/Rust parity matrix. Public-mode/signing rules now come
   from one JSON table embedded by Rust and synchronously imported by TS;
   raw `publicMode` intent lets the registry own Paradex's explicit-private
   rule. CI checks every registered name. The registry also selects HyroTrader
   endpoints from raw config and refuses private non-Paradex payload signers;
   the WASM constructor bridge preserves typed error classification. Factory
   routing exceptions and native/harness polling inputs remain supported.
7. ~~Typed `Interval` parsed once; venues reject unsupported intervals instead of
   silently falling back (Hyperliquid 1m, TxFlow 30d, AsterDEX 1h).~~ Done:
   `model::Interval` decoded at the command boundary; AsterDEX, Kraken and
   Kraken Spot fallbacks removed.

Bridge (contract change):
8. ~~Engine-reported capabilities replace 76 venue-name branches in
   `rust.exchange.ts` and `FrameApplier` positional booleans.~~ The remaining
   36 explicit predicates (21 comparisons, 11 array memberships, four set
   memberships) now use runtime capability policies or the shared synchronous
   construction rules. CI bans their reintroduction. FrameApplier's venue
   booleans were already removed; its sizing computations remain unchanged. Capability
   record: done (`Venue::capabilities()`, `exchange.capabilities`); market
   quantity units in the frame: done (MARKETS flag bits, snapshot
   `CAPABILITIES` section for frame consumers). tealstreet-next:
   `collectAccountAxes.ts`, `ClosedPositions.tsx`, `symbolScopedFills.ts`.
   `contractsNative`/`remainingNative` names and computation stay unchanged
   (**risky** if their computation moves). Calls during pending startup wait
   that lifecycle; unavailable/failed startup rejects with a typed method-specific
   error. Caller readiness and error handling are documented with this contract.

Risky, after live-smoke captures:
9. `num::whole` for leverage (five serialisations today, incl. truncation and
   `20.0`) and one Rust owner for client-id rules.

Lower: ~~shared test-support `ProbeVenue` (10 copied probes in `secondary_tests.rs`)~~
(done: `engine::test_support`), ~~one `SocketSpec`~~ (done: `Cx::connect` takes
a `SocketSpec`; one `Action::WsConnect` carries headers).
~~`RangeWalk` for the 16 hand-rolled history paginators.~~ Shared `CursorWalk`,
`windows` and `OffsetWalk` now page Lighter, GRVT, Coinbase, Paradex, Bybit,
Bitget, MEXC, KuCoin, TxFlow, Backpack, BloFin and Binance fills; OKX, Kraken,
Crypto.com, Hyperliquid, Orderly and WOO stay hand-rolled with reasons in
[RUST_HISTORY_PAGING.md](RUST_HISTORY_PAGING.md).


Owned history completion follow-up: the first reference now uses
`engine::history::{Completion, CursorHistory}` for BloFin closed positions.
Command ownership, marker-before-result ordering and retained-rows completion
live together in the engine. It stages each page until validation: failures before
accepted rows reject, later failures retain accepted rows with owned incompleteness;
encoding and lifecycle invariant failures still reject. Shared cursor policy owns its budget/dedupe/end
checks. Bybit and OKX closed positions now use this owner too, with shared stage reset
and initial cursor support. Bybit general histories now use shared contextual
finalization for accepted rows, sorting/dedupe/caps and strict policy refusal;
actual later-page failures recover with owned incompleteness. Missing list data
never proves empty complete coverage. Binance general histories now use shared
accepted-page completion and `RangeHistory` owns income bounds, dedupe selection,
coverage and finalization; failed prefixes are not accepted, and signing failures
stay strict. Other general histories remain to migrate;
this does not close the cross-venue enforcement work. See [the plan](RUST_OWNED_HISTORY.md).


BloFin general histories also use the landed cursor/completion owner for typed
staged/accepted rows and native-ID dedupe. Whole-response admission excludes
failed prefixes; actual later-page failures recover only accepted rows with owned
incompleteness. Missing/repeated cursor, page budget and header-preparation
refusal remain strict. Raw paging counts, request pins and native financial
shaping are retained; no engine extension or universal enforcement claim.

Bitunix fills/orders/income now reuse the same CursorHistory owner instead of
local row vectors, dedupe, paging budgets and marker emission. Native time/offset
facts feed the existing shared walker: raw progress before mapping filters,
single-page fills, 50/100 page caps and final sorting/limits remain. First-page
failure rejects typed; later failure returns accepted pages with owned
incompleteness, excluding staged failed-page prefixes. Signing preparation
refusal stays strict notSent. No engine API extension or universal fan-out claim.


BingX's native observed-cumulative/last-quantity cap now uses the shared
`ObservedProgress::prepare_capped` path. The engine prepares and validates a
native-priced suffix before the venue commits execution identity, observations
or terminal state; its existing authoritative Last policy remains distinct.
Four original-source RED financial regressions and two original compatibility
controls cover correction recovery, stored-state isolation, fallback and quiet
identity admission. KuCoin Spot fallback and Orderly cumulative adoption remain
open. The owner prioritized the shared financial snapshot freshness core
immediately after AsterDEX, before remaining fan-out and reporting. AsterDEX
now uses the same capped
preparation and validated observation type for its native no-ID path and
one-time REST seed. Negative observations cannot consume retention slots and
evict healthy fill anchors; native direct-ID delivery, duplicate observations,
accepted quiet rows and10,000-entry bounds remain. One original assertion RED
and ten original compatibility controls are all GREEN through the existing
engine path; no second local subtraction/minimum helper was introduced.





Paradex histories also reuse CursorHistory for staged/accepted rows, command
ownership and completeness instead of local row vectors, walker ownership and
marker emission. Native lookup proof, strict cursor/unbounded-budget refusals
and fills fallback remain. The shared probe-stage operation preserves cycle and
lifecycle checks while preventing result-row admission from hiding coverage.
Queued auth/preparation rejection stays distinct from actual-page recovery.
[Scope and proof](RUST_OWNED_HISTORY.md); other families remain unconverted.






Lighter general histories now adopt CursorHistory for staged/accepted rows,
command-owned completion and native cursor facts. One owner spans trade/funding
income with a stage output baseline; actual later failures recover accepted
pages only. Native saturation/cycle/budget and synchronous auth preparation
refusals remain strict. No new engine API or universal migration claim.








Coinbase Advanced Trade history owner adoption now routes fills, orders and
public trades through existing CursorHistory, with one completion across CFM
and Spot stages. Only complete validated response pages affect shared identity
membership/admission; native fingerprints remain conflict-integrity proof before
filters. The engine owns paging progress, accepted-only later failure and one
command-bound coverage marker. Preparation/native policy refusals remain strict.
No new core API or venue pager/marker emitter; other history adapters and NX B11
coverage remain open. [Scope and evidence](RUST_OWNED_HISTORY.md).








WEEX income now adopts RangeHistory for bounded reads and CursorHistory for
single-page/equal-bound reads. The shared core admits validated native coverage
gaps and enforces an optional lifetime request budget using the existing walker
count. Five local income marker emitters are gone; only successful finalization
can publish one owned gap. First/zero-accepted failure rejects; later actual
failure stops and retains accepted pages. Fills/order/public-trade lists and their
remaining native completeness fact still need conversion; this is not universal
history enforcement or B11/deployment closure.


WEEX fills/orders/public-trade lists now adopt the existing CursorHistory owner
across native windows and paired normal/algo reads. Shared owner dedupe and
window generation replace native accepted-row/dedupe/window-loop state. Coverage
facts publish only after whole-page admission and final success; no native
HISTORY_INCOMPLETE emitters remain in WEEX histories. Explicit algo continuation
is an owned gap, with no new paging algorithm. Income remains the prior slice;
other families and caller/deployment proof remain open.

Phemex income uses one engine range/completion owner across both currency stages.
Original-source RED controls pin typed first failure, eligible-only partial
recovery, later-currency loss and failed streamed-page prefixes. Core controls
cover first gaps, stage lifecycle, cumulative budgets, filtering after coverage
and actual native-bound restrictions; concurrent owners and Error-only logging
stay isolated. Saturation intentionally continues USDC with an owned gap; no
per-venue completeness emitter or positive deployed-runtime claim is introduced.

## KuCoin Spot cumulative fallback adoption

The native Spot fallback now uses the engine execution helper rather than
emitting a cumulative amount directly. Original-source fixtures reproduce prefix
overdelivery; the staged-seed and native-identity controls preserve direct last
quantities, selected-field presence, binary precision, late/terminal delivery,
partial healthy peers and failed/stale/lookup nonpublication. Financial progress
is separate from Store display, terminal IDs and execution-ID retention. Native
bounded retention and missing-endpoint ambiguity remain documented; this is not
new live/provenance evidence or closure of Orderly/reporting fan-out.


GRVT general histories now use one shared CursorHistory/Completion for output
admission, cursor progress and command-owned coverage. A reusable engine page
discard clears failed retry prefixes without rewinding accepted output, budget,
identity progress or first gaps. Order-filtered complete-fill proofs and queued
session/preparation refusal stay strict, while later actual general-query loss
retains accepted pages only. No venue marker emitter or copied pager/terminal
policy is introduced; other families and B11 runtime coverage remain open.


Crypto.com histories now adopt the existing CursorHistory/Completion and staged
page reset APIs. The engine owns accepted-only completion and one command-owned
coverage marker; the adapter supplies exact u64 native cursor/identity facts and
retains native strict proof checks. No new engine API, copied marker emitter or
paging/retention implementation; other adapters and caller runtime coverage
remain open. [Scope](RUST_OWNED_HISTORY.md).

## Orderly observed-average execution adoption

Orderly's selected-last/average derivation now uses the additive engine
`prepare_observed_average` operation. A separate observation may commit before
native dedup; inferred quote may enrich only a live same-endpoint cursor after
unique positive Fill delivery. Original-source public price assertions expose
the missing/zero-average quote-poisoning defect; compatibility controls preserve
native arbitrary-last quotient pricing, gaps, quiet/duplicate observations,
field presence, overflow, REST non-seeding, late identity and bounded eviction.
Whole-growth inference is Binary-only and best-effort, with supplied evidence
validated before effects. Shared row-reporting policy/fan-out remains separate;
this does not claim new live captures or universal financial contract coverage.


## Persistent row-reporting compatibility

`coverage::RowReports` centralizes persistent repeat/re-arm memory for the first
Coinbase, Crypto.com and WOO consumers. The engine API returns whether repeat
suppression admitted `row_skipped`, independently of host filtering; it does not
mean a row or snapshot was admitted. Native event level/code/text/order, Error
upgrade policy, namespace boundaries and early decoded-key re-arm remain.
Original account controls are compatibility GREEN, not defect RED evidence.
New engine controls cover all three policies, capacity reset before duplicate
lookup, independent unbounded mode, borrowed lookup and host-filtered health.
Grouped/deferred reporting and the remaining persistent consumer fan-out stay
explicit follow-ups; see [RUST_ROW_ADMISSION.md](RUST_ROW_ADMISSION.md).

Hyperliquid perp histories now adopt the existing engine CursorHistory owner for
staged admission, page budget, stage restart, retention gaps and terminal marker
ownership. No history-local HISTORY_INCOMPLETE emitter remains. Actual first or
zero-eligible failure rejects typed; later actual loss excludes failed prefixes
and retains only accepted output. Inclusive timestamp/native-identity progress
and final order-transition folding preserve native behavior; mixed income keeps
its first gap through funding. Spot and lookup remain separate legacy paths;
other families, managed completion and B11/deployment proof remain open.


## Grouped and deferred row-reporting compatibility

Shared `RowCount` and `RowKeys` keep the two native grouping contracts explicit.
Crypto.com counts every rejection; WOO preserves ordered unique known IDs and
every anonymous rejection, including its move-based order-pagination progress.
An emitted summary increments health exactly once at the original site,
regardless of group size or host filtering; absent or repeat-suppressed summaries
do not. The shared optional-summary reporter
forgets only its key on None; Some(empty text) remains an observable report.
Bybit deferred Once memory is unbounded, separate per product, never re-armed by
clean rows, and flushed only at accepted joined-read publication after native
cached-row restoration. Partial admitted joins still report unsupported rows.
Original native compatibility controls cover failed/incomplete/stale joins,
terminal-ACK invalidation, retained exposure and account swapping; no defect RED
or complete persistent-reporter fan-out claim. See RUST_ROW_ADMISSION.md.


### BloFin/Kraken native reporting boundaries (2026-10-23)

Five reporter collections now consume existing engine reporting policy across
BloFin and both Kraken modes. Original-GREEN controls pin all three BloFin order
aliases, decoded per-key forgetting before later failures while unrelated
anonymous memory remains, held position snapshots, and Once remaining quiet
when a kept position disappears. Kraken controls pin unlisted symbols across
feeds/reconnect (with a fresh-symbol positive), silent zero quantity, reporting
before failed/stale REST admission, rejected-identity duplicate protection,
stale WS no-rearm followed by current-clean positive re-arm, coin/order/empty
identity isolation, rejected versus admitted stale balance reads, and Spot's
native unfiltered REST versus filtered WS duplicate re-arm. All126 original
account tests pass before migration. No new engine API, financial/admission
policy change, local replay/timing run, threshold, budget or manifest change.
Full green CI, current artifacts and independent source/caller/data reviews
remain required; this slice does not close the remaining reporter inventory.

Migrated focused suites pass1253 native tests; strict all-target Clippy, scoped
fmt, bans and root checks pass. Both independent implementation reviews confirm
that only reporter memory/adapters changed in the three production files.

Kraken futures histories now adopt existing CursorHistory for staged admission,
identity/cursor progress, fifty-page bounds and query-owned completion/coverage.
No venue-local HISTORY_INCOMPLETE emitter or accepted-row owner remains. Actual
first/zero-eligible failure rejects typed; later actual loss returns admitted
rows only. Missing/conflicting continuation proof, repeated/empty/no-fresh
continuations and cap exhaustion stay strict; a terminal fiftieth page completes.
Closed-order folds cannot admit a failed newer prefix. Ledger since-only gap
precedence, eligible limit suppression, exact native units/IDs/request sequences
and strict consuming setup are preserved. Other adapters, B11/runtime proof and
managed Context completion remain open. [Scope](RUST_OWNED_HISTORY.md).

KuCoin owner-adoption regression plan: original-source controls pin
initial typed rejects, accepted-only later recovery, failed-page/retry identity
isolation, strict ledger/funding/census proofs, concurrent owner attribution and
complete-empty reads; spot single-response fills never publishes a malformed
prefix. Existing unit/request-sequence/funding/spot checks remain compatibility
coverage. The engine owns completion and marker policy; no new venue pager or
signing implementation is introduced.

## Bitrue shared single-page owner adoption

Bitrue fills reuse engine HistoryRows/Completion without a fake cursor walk or a
new engine API. Four original-source assertion-RED groups pin the shared typed
Internal fallback on terminal native/malformed/exhausted-transport failures and
strict endpoint coverage refusal. Compatibility controls pin per-attempt row
rollback, same-ID economics after retries, independent signature/GET budgets,
strict timestamp increments, 1000/1001 admission, native duplicate/filter order,
missing-array versus empty success, concurrent owners and Store isolation.
The adapter retains endpoint facts, not another completion or pager policy.

Backpack fills/orders/funding/public trades now adopt the existing CursorHistory
owner for staged admission, typed first failure, accepted-only later recovery and
command-owned repeat/cap/skipped-fill coverage. The existing engine offset helper
only computes raw displacement after owner More; no second pagination authority
or venue marker emitter is introduced. Original-source assertion RED controls
cover the changed contracts; native shape/filter/units/request order, eager
non-fill diagnostics and strict setup remain. NX coverage/runtime proof stay open.

## ApeX bounded history owner adoption

ApeX bounded execution reads use existing engine HistoryRows/Completion for the
selected terminal result. Four original-source assertion-RED groups prove that
all-native, missing-list/malformed and exhausted transport failures must reject
rather than return background-cache/empty success. Controls preserve successful
cache selection (including mixed alternatives and filtered nonempty primary),
bounded duplicate replacement after eviction, two/four candidate requests,
signature reset, concurrent owners and persistent live cache. Native endpoint
selection/upsert facts remain local; no new owner/pager/marker policy or shared
signing-provenance implementation is copied into the venue.


### AsterDEX/GRVT/TxFlow native reporter boundaries (2026-10-23)

Three persistent Once memories consume the engine reporter. Nine original-GREEN
compatibility controls pin native zero-size versus clean-clear behavior, early
REST forgetting despite a stale snapshot, Once suppression after severity
changes, composite versus exact-leg labels, anonymous/unlisted-key isolation,
GRVT accepted/rejected duplicate coins versus filtered order/position rearm,
non-array retention versus null clean-empty coins, and current-read rearm
before per-row timestamp rejection. All125 original account tests pass before
production migration. Hash-bound source/test/log evidence and the remaining
reporter inventory are documented in RUST_ROW_ADMISSION.md. No new engine API,
financial reducer, admission/freshness, signing/request/session or history
policy is changed. Current full CI, artifact proof and separate guard/merge
remain required for adoption.

Migrated focused engine/AsterDEX/GRVT/TxFlow/Binance suites pass1294 native tests
across66 result groups; strict all-target Clippy, scoped fmt, bans (13 controls)
and root44 typecheck/28 lint pass. Both independent source/test/caller reviews
are CLEAN. Fresh full CI, current artifact proof and separate guard/merge remain
required; no all-venue reporter-completion claim.

## Paradex managed startup GET proof

Four real-venue assertion-RED controls require managed public catalog/ticker
preparation, exact live/testnet empty-header GET bytes, preclaim callback silence,
original deadline before/after claim, typed preparation denial, stale-attempt
suppression, native publication and failed-stream-prefix atomicity. Existing
startup resilience and shared fixture drivers perform the shared readiness/claim
handshake while retaining original payloads, clocks, timers and assertions.
Independent source/adversarial, caller and full-map reviews accompany the slice;
no per-venue lifecycle, retry implementation or transport workaround is added.
