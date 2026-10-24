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

## BloFin public managed-read boundary

Unsigned BloFin discovery/candles adopt engine-owned preparation, release,
deadline and Context completion. Typed setup errors survive the venue callback;
public preflight validates market before interval and reports explicit user-error
kinds. Background retirement covers successful, malformed, missing-list, native,
transport and preparation-failure terminals. Native row mapping, discovery retry
policy and private behavior remain independent. Shared test transport helpers
execute preparation/claim at fixture event time; raw-action regressions retain
the unreleased-response and parked-preparation boundaries. One pre-Context
numeric refusal awaits the shared command-entry seam; no new legacy fingerprint
or checker exemption is introduced.

## Bitrue FIFO read lifecycle regressions

Native controls reject late and exact-deadline trade/candle success, prevent an
expired FIFO head from dispatching queued work, and preserve delayed timeout
completion across unrelated admissions and duplicate numeric IDs. They retain
live-peer socket-wide failure, explicit/empty correlation compatibility, capacity,
malformed/rejected-head continuation and command-ID reuse. Repeated native
preflight refusal at one clock proves bounded entry ownership is released without
opaque I/O or loss of the original diagnostic. The original three assertion RED
controls precede the production move in a separate test commit.

## MEXC candle Context regression controls

The candle lifecycle regression suite exercises public preparation/binding/final
claim, original-budget expiry before and after release, typed route and transport
failures, late/duplicate callbacks and numeric owner reuse. Native controls retain
query bounds, tail/zero/no-limit results, atomic malformed-column rejection,
HTTP/native envelope errors, and live versus stale HTTP429 depth/account pauses.
The original 1000-row no-limit fixture retains its payload/result assertions and
uses only the shared prepared-actions executor for this command.

## KuCoin Spot discovery lifecycle regressions

Managed discovery regressions cover keyed/keyless unsigned header preparation,
unreleased identity rejection, original deadline expiry, ticker-only recovery,
unchanged native envelopes and duplicate suppression. A fixed-clock capacity probe
drives 270 terminal attempts separately through each of six response/failure routes,
so 20-second expiry cannot conceal missing retirement. Native startup/valuation,
broker, secondary identity and socket-policy assertions retain their original
payloads, using the shared prepared transport fixture. The real WASM boundary
fixture explicitly supplies that same existing test transport preparation contract.

## MEXC initial candle admission regressions

Two separately committed original-source RED controls require initial read
rejection to have no write outcome, no transport/signing or auth evidence, and
exactly one typed terminal. They prove valid ID reuse after invalid time and
continued prepare/claim/completion of an existing read after the256-owner limit.
The nine prior candle lifecycle controls and native fixture assertions remain.

### Bounded account reporter fan-out: MEXC/Phemex/Polymarket

The three venues delegate persistent skip memory to existing engine bounded Once
reporting. Thin adapters preserve native label formatting and native recovery
boundaries; there is no policy extension or per-venue replacement implementation.
Default constructors select the same 4096-entry clear-before-lookup behavior.
Shared engine tests cover rollover and Once severity suppression; nine added
native compatibility controls cover the consumers' recovery/admission differences
and pass on unchanged production with all116 account tests. Separate permanent
unlisted memory and direct notices remain outside this bounded slice.
Architecture gate: YES, cross-cutting consumer fan-out through the existing engine
choke point. No signing, transport, history, snapshot authority or financial
arithmetic change. Current-head CI/raw proof and queue ordering still apply;
this entry does not close the remaining reporter inventory.


## Lighter managed candle area

Lighter and Robinhood candle requests share the engine preparation/claim,
original-deadline and generation-bearing completion paths. Typed preparation
failures are passed intact; failed streamed rows never become a successful
result. Background retirement precedes stale subscription/revision returns and
preserves later fallback polling. Existing candle source-order assertions stay
intact while their fixture uses the shared prepared-action helper. An additional
within-deadline resubscription control distinguishes the venue generation fence
from expiry. No signer, nonce, paging, row-admission or retry policy is copied.

The exact request ratchet removes four Lighter lib occurrences only. The
pre-authority issuance refusal remains explicitly counted until its shared
engine prerequisite; this does not claim all Lighter or cross-venue closure.

## Paradex managed startup GET proof

Four real-venue assertion-RED controls require managed public catalog/ticker
preparation, exact live/testnet empty-header GET bytes, preclaim callback silence,
original deadline before/after claim, typed preparation denial, stale-attempt
suppression, native publication and failed-stream-prefix atomicity. Existing
startup resilience and shared fixture drivers perform the shared readiness/claim
handshake while retaining original payloads, clocks, timers and assertions.
Independent source/adversarial, caller and full-map reviews accompany the slice;
no per-venue lifecycle, retry implementation or transport workaround is added.

## Hyperliquid perps candle lifecycle controls

Ten native controls retain the original three failing-first assertions and cover
public exact-header release, preclaim/stale/reused identities, preparation refusal,
parked/released original expiry, failed incremental prefixes, progress at the
deadline, capacity-before-validation and peer isolation. Native zero/default/capped
limits, ignored from-time geometry, HIP-3 spelling, unclassified response errors
and merged spot/perps separation are pinned. The checkpoint compiled with nine
legacy-dispatch assertion failures and one native compatibility pass before the
adapter move; all ten pass after it. Existing public candle assertions retain
their exact payload checks through the shared prepared-actions test executor.

## BloFin candle command-entry regression proof

Two original-source assertion failures pin typed invalid-clock and registry
capacity rejection before Context issuance. The capacity fixture fills genuine
background roots in an earlier no-ID callback, repeats270 refusals at one clock,
explicitly retires the roots and successfully reuses the command ID without
expiry. Duplicate parked entry preserves the original preparation and one
completion. Another fixed-clock control runs270 market refusals,270 interval
refusals and270 successes with stale previous-response probes. Existing native
candle mapping, validation precedence and preparation/expiry assertions remain.
This closes the earlier pre-Context numeric refusal gap using the shared seam.


### Profile capability truthfulness

Aster V1 STP, Bybit Spot/Hyro income and Coinbase Spot/INTX/CFM income no longer
inherit an advertised command they refuse for that profile. Six original
assertion REDs cover those profiles; supported Aster V3 and Bybit
linear/inverse/demo remain positive peers. Native refusals and V3 STP route
contracts stay separate from capability assertions. Only the three capability
fields change in production; no handler, signing, history or PnL policy changes.

### Polymarket unsupported history advertisements

The valid-state capability audit found three inherited history claims whose
native handlers unconditionally refuse. Six original-source assertion REDs
cover public/credentialed fills, orders and income across all three signer
modes, independently checking exact unsupported failure and zero HTTP before
the capability assertion. Set those three bits false, preserve supported peers,
and align the host cfg(test) record fixture. Future native history adoption must
restore the corresponding bit atomically with valid-state dispatch. The SDK
branded-empty fallback and coverage obligations are recorded in CALLERS.


## TxFlow indexed history owner controls

Original ten native controls pass. Added original-source controls produce three
assertion RED groups (typed first/filtered-only failure, accepted-page recovery,
and mixed-income stage recovery), plus a terminal-offset/cap compatibility pass.
The corrected suite also checks numeric fill-ID ties, strict empty/repeated
continuation after accepted output, and a terminal-cap income stage followed by
fresh funding offsets. Existing exact request sequence, full pagination before
final output limit, signed funding, raw-millisecond filters, native ID dedupe,
delisted/relisted instrument mapping and native order quantities remain covered.
No local release WASM/full replay timing sweep substitutes for final CI/artifacts.


## Orderly shared history controls

The original seventeen integration controls establish the native baseline.
Failing-first controls pin typed first failures, accepted-page continuation
recovery and regular-to-algo failure recovery. The corrected controls assert
failed-prefix exclusion, filtered-only refusal, strict native metadata/progress
checks, the total fifty-page budget with a terminal fiftieth page, separate stage
identity/limits, concurrent command attribution and strict signing refusal.
Existing funding spacing and delayed-timer, nested algo, canonical ID, native
identity, empty/malformed page and final-format tests remain in the focused
Orderly suite. Two old whole-read continuation/algo-error assertions are explicitly
superseded by accepted-only partial output plus the shared incomplete warning.


## MEXC history owner controls

Original-source tests pin the native ten-control history baseline. New failing-
first assertions cover typed first/filtered-only failures, accepted-page loss,
funding page-prefix rollback and older-window failure. The original-source page-
1000 compatibility control passes separately. Corrected controls additionally
check identity retention across windows and native encounter order, concurrent
command markers/empty success, single-request order fills and exact global
limits. One old whole-read continuation refusal is explicitly superseded by
accepted-only partial output plus the engine warning. Existing tests retain
exact native window request sequence, units/IDs, funding and public-trade shapes,
deduplication, and limited newest-first reads. No local release WASM/replay timing
run substitutes for the current-head CI and final artifact proof.

## Phemex bounded non-income shared owner

Fills, orders and public trades reuse HistoryRows/Completion with one page and
strict response admission. Four original-source assertion-RED groups pin typed
Internal terminal failure; all 28 prior controls and three new compatibility
controls pass on the original source. Compatibility covers valid empty/full
native caps with no continuation, stable equal-time ordering, peer isolation
and unchanged lookup classification. Corrected controls exercise transport,
HTTP, invalid-prefix and oversized failures in direct/streamed modes, same-ID
slot reuse and zero partial/gap publication. Endpoint codecs, requests, signing,
account and income source remain outside this change.

## WOO shared owner across native history routes

All WOO history axes adopt existing CursorHistory without an engine API change.
A continuous owner counts every physical native continuation, including empty/
failed alternative transitions; no empty/no-new-row stall flags are installed.
Eight original-source assertion-RED groups cover typed failures, accepted-only
later recovery, filtered-only refusal, mixed-page coverage and strict native
refusal classification. Three additional compatibility controls pass on the
original source, including the terminal-1000 versus required-1001 boundary,
combined alternative/legacy budget and streamed failed-prefix identity reuse.
Native builder/codec precision, symbol aliases, row isolation, ledger pacing and
zero Store publication remain pinned. Caller controls distinguish the unchanged
immediate row warning from the owned terminal coverage marker.

## Explicit non-pageable history results

The three non-pageable adapters use existing HistoryRows/Completion without new
engine API or result schema. Six original-source assertion-RED groups prove the
previous missing owner coverage and untyped public-trade refusal; all 110 prior
selected controls plus three new lookup compatibility controls pass on original
source. Corrected controls require exactly one command-owned Warn before Done,
including Error log level, [] shape despite live cached orders, zero I/O or Store
mutation, distinct command IDs and explicit pre-I/O Unsupported/NotSent. Native
unsupported capabilities remain unsupported and no numeric managed fallback is
introduced. Caller polling guidance retains coverage while deduplicating UI
alerts.


## Paradex managed REST book area

The independent Paradex public snapshot query adopts the existing engine
preparation/claim, original read bound, exact delivery check and Context
completion. Public wire/codec behavior and the existing eight snapshot contract
tests remain; fixture adaptation uses shared prepared-actions and explicitly
checks the one new original-deadline timer before preserving outbound-only
assertions. Original-RED controls cover premature delivery, typed denial,
expiry, reused/concurrent owners, duplicates and capacity refusal/recovery.
No private credential, Starknet, candle fallback or live-book mechanism moves.
The venue-only inventory loses five occurrences; pre-authority refusal remains
counted for the shared engine entry seam rather than hidden in another API.

The existing public-SBE and private-JWT snapshot isolation fixtures also use the
shared prepared-action helper for this query only. Both keep their original
clocks, request bodies, native-state and protocol assertions; the public fixture
checks the new original-deadline timer explicitly before its no-extra-actions
control. Held account/trading source and tests/account are untouched.

## Lighter command-entry regression proof

The separately committed candle controls reproduce two native admission errors:
capacity returned a generic adapter message, and invalid clock admission reported
Internal instead of the engine's InvalidInput. Both are assertion-RED on the
original implementation. The live-duplicate control allows the pre-existing
COMMAND_DUPLICATE diagnostic and proves no request/terminal replacement; it is a
compatibility control, not an original failure. All seven earlier candle controls
remain. New controls exercise both aliases/networks, denial before invalid market
validation, one terminal/no resource allocation, peer completion, freed-slot
reuse and NaN/positive-infinity/negative-infinity admission followed by a finite
command. The adapter uses shared command entry only for its genuine command
caller; background generation cleanup and native query/stream behavior remain.

## KuCoin Spot candle managed-read controls

Tests-first controls distinguish unreleased, bound-but-unclaimed and claimed
responses, timer-first and response-first exact-deadline expiry, typed preparation
and transport failures, initial admission refusal and immediate physical queue
refusal. Genuine saturation fixtures assert256 separate background roots or256
queued physical requests under one background root in a prior no-ID callback, then explicitly retire them and recover at
one clock. Fixed-clock repetition pins preflight and success retirement, duplicate
command preservation and stale-generation rejection.

Native compatibility controls preserve complete candle fields, both timestamp
units, duplicate timestamps, oversized responses, volume column5 with extra columns, permissive
unknown symbols, exact headers/query calculations and separate malformed-row
versus envelope error timelines. Corrected native fixtures ran unchanged source
with eleven RED and three compatibility GREEN controls before the managed source
made all fourteen GREEN.

## MEXC initial Plan refusal regressions

Separate original-source tests reproduce two failures: no typed initial Plan
startup terminal, and resumed HTTP through late pagination/asset refresh. Two
negative controls already pass on original production, preserving transient and
post-startup retries. Coverage includes AuthInvalid/AuthExpired/AuthPermission,
native code/message, deactivation policy, exactly one startup terminal, stale
poll delivery, late sibling failure/login, a 100-row late page and asset timers.
Both normal empty-list error envelopes and errors missing data are covered;
successful missing/null lists retain retry behavior. Existing account assertions remain
intact, and the legacy request inventory has no venue delta.

Verification on the integrated startup fix passes456 MEXC+Binance tests across
40 targets, including all44 account tests. Scoped fmt/strict all-target Clippy
and both ratchets (14 controls) pass. Three independent reviews are clean;
architecture No. Source/test files held by history #1793 remain untouched.

## Phemex unsigned candle request regressions

Committed plan05988416603 precedes separate original-source tests2a1a008bc7c:
six lifecycle regressions fail and primary/secondary/dual signing compatibility
already passes. Controls cover preparation/binding/one release, parked and
released expiry, stale callbacks with ID reuse, typed setup/network failures,
genuine invalid-time admission and native sorted output. Additional native
error/malformed-row/missing-list comparisons match the credentialed path, and
an expired429 reply cannot pause another read. Existing public assertions are
retained with the shared prepared fixture; only the daily response clock moves
forward to its command's timestamp. No scanner/manifest change or credentialed
signer migration is included.

## MEXC managed book regressions

Plan531195b3a8c precedes separate original-source RED0213bd8e13a: all7 lifecycle
controls fail on original production and pass with Context custody. They cover
prepared binding/one release, parked and released expiry, setup metadata, claim
at deadline, stale callbacks with command-ID reuse, genuine invalid-clock entry
and expired429 non-interference with both depth and account cooldowns.
Native result/auth assertions retain payloads and expected values while using
the shared prepared fixture and forward response/WS clocks. Successful dispatch
asserts exactly one engine20s deadline and no retry timer/socket send; immediate
input/cooldown refusals still forbid timers and managed dispatch. Native decoder,
large IDs, scaling, overflow beyond limit, code510 and live cooldown controls
remain independently exercised. Inventory shrinks by exactly8 occurrences and
7 entries, with no unowned map or scanner change.

### Shared order-snapshot admission: Backpack/Paradex

Both families replace whole-order snapshot counters with the existing opaque
Store token. Backpack preserves newer-read supersession by invalidating before
capture. Paradex keeps single-read busy ownership and all lagging-replica,
terminal and amend-evidence semantics. Native cache changes and Store rows now
publish under one admission boundary; repair HTTP actions remain outside the
callback and run only after acceptance. Other domain and per-ID clocks stay
native. Permanent exhaustion performs no order GET and does not strand either
family's account continuation.

The original four-crate baseline passes1157 tests; final native controls include
four original Backpack assertion REDs across two proven defects, native malformed
Pair and Paradex ACK compatibility positives, and distinct token API/exhaustion
controls. The inventory ratchet keeps1253 sites: only the existing Paradex order
request's pending-field hash rebinds10-to-10, with exact whole-map substitution,
source/hash pins and growth/unrelated/pruned-parent negative controls. No rule,
path or site-kind allowlist is added. Architecture gate: YES, consumers adopt the
existing engine choke point. No all-venue completion or private parity claim.

### Lighter synchronous cached Read Context controls

Plan2a093966518 precedes original RED commit ffef6eaf580. Against actual732cf8
production, five assertions fail: capacity for present/missing leverage and tier,
plus invalid-clock leverage/tier; three compatibility controls pass. After the
shared admission/terminal move all eight pass, retaining duplicate-owner isolation,
all aliases/networks, finite recovery, no-I/O/no-Store publication, slot reuse and
300 immediate completions without owner accumulation. Existing account_limits
controls remain unchanged, including cached standard tier and SetAccountTier.
Independent plan source/adversarial, caller and full-map reviews are CLEAN;
architecture gate No: no new local capacity, clock, retry or completion mechanism.

Cached-slice verification passes504 Lighter+Binance native tests (48 result groups),
strict all-target/all-feature Lighter Clippy, scoped fmt, bans and both request
checks/14 controls. Actual e233cb05dfb is genuinely integrated; its incoming
KuCoin/MEXC source and prunes are preserved. CI owns WASM/replay/size/timing;
native success alone does not claim these broad gates or whole-venue closure.

## WOO family compiler throughput controls

A plan and baseline named profile preceded the WOO-only compiler change.
Five interleaved baseline/candidate Rust pairs alternate execution order:
medians23.105/18.957ms, four of five pairs improve. Every run serves77 frames
without protocol errors or unserved requests. One correctness replay per
artifact has identical final snapshots and all five checkpoints; every
nonmetric output field also matches. The normal candidate grows61,376 bytes
(1,142,442→1,203,818), within the unchanged1,343,488-byte ceiling. Inventory,
tape, clocks, budgets, comparison rules and runtime/shared paths are unchanged.
Local medians and sparse overlapping profiles do not establish the TS/Rust1.10x
CI target or private runtime proof. The exact-head CI build and replay gates
remain required. [Reproduction and evidence](RUST_WOO_PUBLIC_CPU.md).

Production WOO CI at f564c7b50f1 (run 39977265913, job 120495031043) passes the
nine-pair public gate at reported TS 15.8 ms / Rust 8.9 ms steady CPU, 1.78x.
Its actual CI WASM is 1,197,465 bytes within the unchanged 1,343,488-byte ceiling;
manifest hashes and the CI checkout tree match the tested head. Strict
verification records one public PASS, one existing declared known-private FAIL
and zero unexpected failures. This does not establish private authenticated
lifecycle parity. The tested head has 43 successful checks and one browser skip;
source-equivalent current integration, final guard and merge GO remain separate.
The fixed gates and comparison policies are unchanged.

## SDK master venue parity controls

Separate original-RED commit637fe5832d6 pins seven assertion failures: three
Hyperliquid fee-subject/startup controls, Lighter timestamp candidate and48-bit
exhaustion, Robinhood approval preflight and signed attribution. The zero-candidate
compatibility control remains positive1,2. Native private master/vault fee rates,
empty-vault fallback, missing/empty applicationId agent discovery, public/default
fee retention, both Robinhood networks and explicit builder overrides are covered.
Existing Lighter split/bracket/batch, receipt and failed-send controls remain.
Shared ProbeVenue tests seed boundary state privately without a production test
hook. Master TS sources, prepared tape/harness and all incoming inventories remain
unchanged. Native local checks and independent reviews precede the #1869 handoff;
its owner supplies the full CI replay evidence.

## Robinhood frozen CI fixture receipt

CI39953255157/1845 supplies the frozen LightRobinhood engine and three reports.
The original boundary control reproduces one NotSent builder-approval failure
with21 other cases passing. The corrected fixture preserves every existing
native assertion and adds exact signed attribution. Independent and executable
provenance checks reproduce both old and new37-record tape hashes: only three
approved account indices and seven order-attribute objects differ per tape.
Existing assessGateReport finds eleven unknown diagnostics per private report
before the two-hash refresh, and zero errors/eleven known afterward; the public
report remains186 known. Every diagnostic path/rationale and all other manifest
entries are byte-preserved; the original reports remain strict FAIL. No replay,
local release build, new declaration, threshold or comparison-policy change.

## Shared ticker index compatibility and measurement

Tests and generic benchmark were committed before the representation change. All
29 Store controls pass before and after, including duplicate-winner dirty masks,
append-only recovery and repeated large/small/empty replacements releasing obsolete
capacity. The unchanged-source nine-pair KuCoin Spot gate miss supplies performance
RED; functional compatibility is intentionally GREEN on the baseline.

The same965-symbol sequence and2,500,000 native lookups retain checksum1236723300:
99.489 to23.099ns per lookup in elapsed wall time. Native index-only allocation
measurement reports3860 to59603 retained bytes and7720 to119206 replacement-peak
bytes; both release all index capacity at empty replacement. This is a measured
memory tradeoff, not whole-venue or WASM allocation accounting. Full CI checks
all venue memory and WASM size budgets, including Aster, and paired CPU medians.

## Draft Engine socket lifecycle references

OKX adopt one Engine registry/driver for epoch admission, typed auth,
shared Supervisor deadlines/backoff, transactional complete interests, replay,
retirement and required Store invalidation. Native codecs retain venue payloads
and role policies. Original runtime controls pin retired open/close rejection
and expiry-before-late-pong; shared controls cover bounded queues, health,
retry revocation, signed-output retirement and transactional same-interest book
repair. This is a OKX-only preparation, using the landed physical-ID seam; final CI evidence is separate; other families remain open.

The option-1 variant leaves Bitunix incoming-exact on the legacy path; the
temporary socket inventory records that deferred family. No source or gate
claim is inherited from the historical two-family experiment.

### Lighter depth Background Read controls

Plan8b700d2ffc8/caf6e4cf1b3/c7a1d352046 precedes RED362e8edb77f (seven compiled
assertion failures on native dispatch). Fixture correction6311e52cfe0 preserves
the existing testnet REST host; all original admission assertions remain. All
seven pass after managed adoption: exact aliases/networks, no preclaim publish,
original deadline, strict transport/malformed cleanup, stale generation/revision,
capacity isolation/recovery and300 retirements. All33 focused public/depth tests
pass, with six original supervision tests and held candle fixtures unchanged.
Unheld public drivers use shared prepared_actions at their existing event clocks,
retaining every old data/error/continuity assertion. No local transport helper,
retry policy or outcome mechanism is added; broad CI remains a separate gate.

Depth frozen source81ac295d25a passes511 Lighter+Binance native tests (49 result
groups), strict all-target/all-feature Lighter Clippy, scoped fmt, bans and both
request checks/15 controls. Held request_context_candles.rs and trading.rs remain
byte-exact to actualdb32; ticker/catalog runtime remains unchanged. Independent
implementation review and CI do not derive readiness solely from native tests.

## KuCoin local history authority controls

Tests precede production. The fixture delegates the native mutation guard so
Query tracking follows real KuCoin, then exercises genuine Engine callbacks.
Corrected original source produces five assertion REDs and six compatibility
GREENs: missing typed local metadata, capacity bypass, invalid-clock bypass and
fixed-clock metadata/reuse assertions. Saturation genuinely allocates all256
Background roots in a separate no-ID callback, repeatedly denies every reachable
local path, explicitly retires those roots and recovers at one clock. No fake
command-origin or opaque request is used to manufacture admission pressure.
Native precedence, numeric boundaries, both zero-limit results, Store stability,
signed order/funding wire requests and clipped long-range paging are preserved.
Three defensive branches remain explicitly outside native witness claims.
## Bitunix local history admission controls

Tests-first native controls reproduced five assertion RED and three compatibility
GREEN before all eight passed with the authority adoption. Both credential modes,
all five local terminal expressions, native branch precedence, typed refusals and
companion-free timelines are covered. Initial capacity fixtures assert256 genuine
background roots before270 rounds of command refusals, explicitly retire them,
and recover at one clock. Invalid-clock denial and more than256 fixed-clock local
completions cover both empty-result paths as well as refusals. An unchanged valid
signed private history request and empty response remain a compatibility control.

## Bitrue local history command admission controls

Tests-first genuine Engine callbacks delegate both native mutation guards and
cover all five local refusals, including missing/unknown symbols. Original source
produces five assertion REDs and three compatibility GREENs. Capacity setup uses
a prior no-ID callback with256 asserted Background roots; repeated denials retain
peers, explicit retirement recovers at one clock and fixed-clock reuse exercises
terminal cleanup. Invalid raw clocks and typed local metadata are distinguished
from native text/precedence, Store stability and signed-fill preservation.
Aliases, fractional-time flooring, default/clamped limits, native headers and
empty results remain covered; no fake HTTP or command origin supplies authority.

## Bitrue local leverage admission controls

Tests-first fixtures delegate both native guards and use genuine commands for
all four local refusals. Original production gives four assertion REDs and three
compatibility GREENs. The bounded owner probe allocates256 Background roots in
a separate no-ID callback, repeatedly denies work while preserving peers, then
explicitly retires roots and recovers at one clock. Fixed-clock reuse and invalid
raw clocks distinguish shared admission from native messages and Store stability.
Valid bracket/account requests retain their signed dispatch; existing account
suite assertions continue to own successful caller completion/group behavior.

## Phemex local history admission controls

A separate original-RED commit adds nine integration controls on unchanged
661427d10b3 production: four assertion failures and five passing compatibility
controls. The same REDs reproduce after actual 1e8f600da75 integration. All nine
pass with Context-owned local terminals. Real unsigned candle preparations fill
all 256 managed owners; terminal preparation failure frees one slot and local
results recover while the other owners remain pending. NaN and both infinities
reject, then finite command-ID reuse succeeds. All eleven local branches retain
their finite payload/error contract, with active-capacity, duplicate-ID,
precedence, resource reuse and valid signed/public continuation controls.
No existing assertion, fixture, engine, host, signer or shared policy changes.
Both request-context ratchets verify only the exact eleven-entry Phemex prune.
Local Phemex and Binance verification passes 491 tests across 39 targets with
zero failed or ignored, strict all-target Clippy, scoped formatting, and both
request-context checks (fifteen ratchet controls). CI supplies replay/size gates.

## MEXC local read refusal controls

The committed original tests plus fixture correction compile against untouched
c4e02c production with 9 assertion REDs and 6 passing native controls in an
isolated target. Eight independent capacity REDs cover every selected site;
nonfinite command-clock rejection is the ninth. Duplicate-ID protection is a
native control, preserving the shared COMMAND_DUPLICATE diagnostic and pending
original owner. The from-only bound stays invalid through recovery. Excluded
evidence: an initial duplicate-diagnostic expectation mismatch and an anomalous
archive replay sharing the edited tree's build target. No compilation failure
counts as a RED, no old assertion changed, and no test is ignored.

All 15 controls pass after adoption, including real 256-book-peer capacity and
physical peer settlement/recovery, a real account 429 pause established after
book peers park, invalid clocks/finite reuse, repeated local settlement, full
native error fields/precedence and valid signed/public continuations. Scoped
MEXC/Binance tests pass 487 across 42 targets with zero failed/ignored; strict
all-target Clippy and formatting pass. Both request-context checks pass, with
15 ratchet controls and exact unowned-map equality; 8 entries/8 occurrences are
removed only from MEXC. Full CI owns WASM/replay/size and performance evidence.

### Paradex second native Read-history consumer controls

Original plan dec36cfa468 and seven original REDs efafc26dedc retain their assertions
on reviewed #1848 f471 plus actual e233. All seven fail on legacy dispatch before
implementation and pass through real managed release afterward. Controls preserve
unsigned query/body/headers/native codec, limit geometry, encounter order/dedupe,
forced single page, strict malformed/failed prefixes, typed preparation/transport,
original deadline and stale token/ID reuse without marker or Store leakage.
Only one public sender fingerprint is pruned; private authority and shared bridge
blobs remain unchanged. Full native/strict/CI and independent review are separate
readiness gates; this native receipt alone does not claim bridge landing.

The public-trades frozen source38836aad176 passes501 Paradex+Binance native tests
(41 result groups), strict all-target/all-feature Paradex Clippy and scoped fmt;
all three frozen implementation reviews are CLEAN/architectureNo. Incoming
actual2badd1cb0e3 preserves tested dependency blobs and all foreign prunes. Both
request checks/14 controls and bans pass on the integrated head; bridge blobs
remain exact to authorized f471. This is executed native consumer evidence, not
an actual bridge landing or complete WASM/replay/size proof.

## Hyperliquid local lookup entry controls

Six original REDs require clock/capacity refusal before cached or non-definitive
local lookup success. Companion controls preserve the entire cached Order wire,
cache precedence in perps/spot and public/private deployments, symbol/client-ID
matching, private REST and public-only/spot fallthrough, active-owner isolation,
finite ID reuse and 900 synchronous retirements followed by 256 real parked
managed candle owners. No marker or Store mutation is introduced. Exact inventory
removes only two local done sites; native/strict checks and independent reviews
are required on the committed source before merge readiness.

## KuCoin local Read original-source controls

Tests committed before production on frozen661427d10b3 produce ten assertion
failures and six passing compatibility controls. Standalone failures show
capacity/invalid-clock leverage mutation and saturated cached publication before
adoption. Genuine256-owner saturation, fixed-clock recovery/reuse and duplicate
command protection use real callbacks, not fabricated Context or HTTP authority.

Compatibility controls preserve cache aliases/sentinels, full order results,
constant leverage fields and unknown-symbol handling, successful setting no-ops
and capability declarations. Private/pager requests preserve native URLs, headers
and completion while all256 background roots can coexist without an added owner.
No steady WS/ticker/book path changes; current post-#1888 CI must pass the
shared speed gate and report the fresh KCS ratio. The1.20x figure remains a
headroom target, not a merge gate, per the overseer clarification.

### Lighter ticker managed-entry controls

Committed plan and six compiled assertion REDs precede managed ticker source.
The previously held candle deadline driver was corrected first: every captured
same-deadline timer fires at its original raw timestamp; all ten candle controls
pass on legacy ticker source with every original assertion retained. Ticker
controls preserve native fields/aliases/partial-row semantics and symbol-specific
WS precedence through actual prepare/bind/claim, reject preclaim publication,
discard failed prefixes, and prove failure/capacity recovery and300 Background
retirements without completing unrelated commands. Existing fixture drains use
shared preparation at original event times only where the new ticker requires it.
No runtime timer/retry policy or production ordering workaround is introduced.



## BloFin PublicTrades managed history regressions

The native test-first controls exercise preparation/binding/claim, unreleased
responses, original-deadline expiry, typed preparation and released transport
failures, duplicate commands and stale responses. Fixed-clock repetition beyond
the 256-owner bound covers preflight, success and immediate managed-queue
refusal, so elapsed time cannot conceal missing terminal retirement.

Compatibility controls assert every Trade field, native row order, cross-market
rows, identity consumption before unknown-market skipping, timestamp precedence
and milliseconds, numeric defaults, signed quantities and the wire-only limit.
Malformed or refused first pages discard their streamed prefix without an
incomplete marker. Six original-source assertion failures preceded production;
five native compatibility controls already passed that original source.

## BloFin PublicTrades actual bridge receipt

The genuine actual-#1848/current-landing integration retains incoming candle
entry and all six shared history/authority blobs. The capacity message mismatch
is reproduced on the actual engine, then its expected text is obtained from
Error::Capacity without weakening kind/outcome, no-I/O or retirement assertions.
The 32 focused BloFin controls and three shared bridge controls pass; scoped
strict Clippy/fmt and request-context checks with 15 regressions pass. The historical
run of 501 native tests is distinct; fresh CI and a venue merge window remain gates.

## WOO local history custody controls

Real Engine<Woo> controls cover each original local refusal at invalid engine
time, finite native error metadata/precedence, repeated settlement and same-ID
reuse, zero I/O/Store effects, Fills aliases, unchanged private credential refusal
and unchanged unsigned public GET dispatch. Three original assertion REDs plus
four compatibility passes become seven passes without assertion changes. Shared
capacity and revoked-entry mechanics retain the existing engine-suite coverage.

### Paradex local history admission controls

Original RED checkpoint `45c743903b8` adds one focused fixture without editing
production, shared support or inventory: four local branches emit their old
result/error instead of a shared capacity refusal; the clock control likewise
exposes unowned completion. Three native compatibility controls pass alongside
those five REDs. After adoption all eight original tests pass unchanged.
The test uses real Engine callbacks,256 unclaimed managed book peers and exact
terminal metadata, never a manufactured Context. Recovery reuses the freed slot
and individually retires every remaining peer. Fixed-clock300-cycle reuse and
refill exercise owner retirement without deadline expiry hiding a leak.
The original managed public-trades and private history suite stays unchanged.
Exact inventory target is Paradex55->51/history13->9; broader native, strict,
ratchet, independent review and CI evidence will be recorded separately.

Local history verification on reviewed integration `56dacc36562` genuinely
includes actual landing `23d0d490009` (#1892 background executions and #1817
credential selection). All513 Paradex/Binance tests pass across42 targets,
none ignored, including the same eight original local-history controls.
Strict all-target/all-feature Paradex Clippy, scoped formatting, bans, the
request-context scanner and all15 ratchet tests pass. Source/adversarial,
caller and full-map reviews are CLEAN on this actual integration; architecture
No. The seven-path diff preserves every unowned incoming blob and all foreign
manifest entries, with only the four planned Paradex history occurrences pruned.
CI and an authorized train slot remain separate gates.


## Subscription handoff ordering controls

The assertion-only real OKX private reconnect test fails on the original
managed adapter (HTTP index 2 before subscription index 5); the identical native
pre-#1771 control passes in an isolated source/target. The corrected control
compares the complete concatenated action batches and fires the actual queued
wakeup, preserving the subscription-before-refresh assertion.

Seventeen engine controls cover actual managed Prepare/Bind/Claim, final-batch
fencing, callback retirement/replacement, required-interest removal and wire
changes, refusal, additive supersession, pacing, repair/no-op, auth and delayed
signing, bounded atomic admission, last-safe ID/rearm, duplicate/early/expired
wakeups, HTTP-delivery isolation, command-entry refusal and caught fatal state.
Existing OKX account, product and reconciliation fixtures drive genuine wakeups
at their native reconnect times; their financial assertions stay unchanged.
The focused engine/OKX all-target/all-feature run passes 1,241 tests.

Three actual EngineHost bookkeeping controls fail against the original host
and pass with inline/cleared/superseded timer ownership repaired. The full scoped
host-unit command passes 194 tests. These local correctness results do not make
a CPU, memory, size or replay-parity claim; those measurements belong to CI.

Engine feature modes pass: feature-off 718, managed-sockets 820, http-plans 806,
and combined 908 tests. Strict focused Clippy is clean. The initial bans run
fails because the newer remote landing has already pruned unrelated Paradex
and Phemex history inventory; this branch must integrate those actual changes
before a current-landing ratchet pass is claimed.

Runtime review added an eighteenth control: an early host one-shot must return
an explicit rearm at the unchanged handoff time, run no early callback, run
exactly one later callback and release its credit. This assertion fails on the
initial implementation. A fourth actual-host control covers inline early
rearming without losing the subsequent handle. The integrated predecessor run
passes 1,269 focused tests, strict Clippy, formatting and bans; its initial stale
inventory failure is resolved by genuine actual-landing integration.

The refreshed complete host-unit command passes 195 tests, including all four
handoff controls. Uufub4 independently executes the archived handoff file 4/4;
ih35js's source/control clearance carries no independent build claim.

The nineteenth handoff control covers #1932 composition: both initial arming and
a previously armed wake remain blocked during the exact expiry barrier, retain
the pending credit and barrier, and resume once afterward with a fresh timer.
The unguarded integrated source is assertion RED at aa6613f7677.


### Bitunix public REST managed Read proof

Plan756a precedes original tests fda4818/c2aa (10 then14 assertion REDs, five
compatibility controls GREEN); public source follows. Native authority tests
cover Prepare/Bind/Claim, stale/duplicate/reused command IDs, invalid clock,
entry capacity, typed provider refusal, expiry, parser versus transport failure,
public auth isolation and both seeds' repeated fixed-clock retirement. Existing
private/socket native assertions are preserved through genuine preparation
fixtures. Immediate managed queue-capacity is reviewed defense/shared-engine
coverage, explicitly not a native execution receipt (owner ruling in plan).

### WEEX local history original REDs and typed Read completion

The new dedicated local-history target compiled against unchanged actual
`a828ede7ebb`:3native controls passed and8intended assertion REDs exposed seven
capacity bypasses plus invalid-clock publication. Test-only commit
`d5746d2d490` and the committed plan received three independent CLEAN reviews.
After the seven scoped terminal replacements, the same11controls pass without
changing their assertions. A genuine retained-root probe covers the keyless
case without credential fabrication; duplicate/recovery/300-cycle refill and
full-capacity native remote wire controls keep custody and fallthrough explicit.
No new per-venue lifecycle, retry, signer, history finalizer or engine seam.

Scoped WEEX/Binance native verification passes506tests,0failed/ignored;
strict WEEX all-target/all-feature Clippy, scoped formatting, source scan and
all15request-context checker tests pass. Implementation review/CI remain separate.

### WEEX FetchAccount local and remote Read custody

Original identity controls on actual `a828ede7ebb` expose five full-capacity
entry failures and invalid-clock numeric publication. Three native controls
PASS beside six intentional assertion REDs; test-only `902335f2b6f` is the
reviewed strict-Clippy refinement of original `a55a561b921`. The same nine
controls now PASS, with genuine engine-root capacity/recovery/peer isolation,
300fixed-clock local completions/refill, exact native diagnostics and metadata,
independent primary/secondary HMAC wire, primary stableId attribution and Store
preservation. Plan, original REDs and the refinement each have three independent
CLEAN reviews. No new provider key, fabricated Context, engine API or venue
lifecycle/helper was introduced. Broader native/strict/ratchet/final review and
CI evidence are separate gates.

Pre-integration native504WEEX/Binance tests and strict WEEX Clippy/fmt pass.
Whole-map provenance is blocked by newer Aster inventory shrink in origin;
actual integration and fresh gates follow, with no foreign manifest edits.

Actual integration e336f67b7de genuinely includes15bd7821b52; all unowned incoming
blobs and foreign prunes are preserved. Fresh504native tests, strict Clippy/fmt,
full bans and both request-context checks (15tests) PASS. Three independent
implementation reviews are CLEAN; architecture No. Exact own identity2F/2O
prune is verified; the parallel #1951 history prune is not credited here.

### Binance identity preflight Read admission

Plan `3a0598f1892` precedes original-control commit `84e327e2c8f`.
With unchanged production, four real full-registry controls are assertion RED:
missing/disabled/incomplete-primary/incomplete-secondary identity preflights
publish local messages rather than the engine's capacity refusal. The existing
valid-command capacity control and all six identity compatibility controls are
GREEN, including 300 same-clock/same-command local refusals and exact selected
signer request bytes. The same eleven tests pass after conversion, with no
transport, extra diagnostic or Store mutation on initial admission refusal.
No manufactured Context or fixture bypass is used.

## Phemex unsigned public-trades custody controls

Original checkpoint fc1b87973fa compiles13 controls on unchanged15bd production:
8 behavioral assertion REDs and5 native passes, zero ignored. The dedicated
public target retains those selected assertions; held signed REDs remain on
feat/rust-phemex-bounded-history-context at321992b680c. Additional controls pin
invalid-entry refusal, typed route metadata without auth evidence and cleanup
after a pause exceeds the original20s root. Existing native history fixtures
use the shared prepared-action driver and monotonic response clocks; only the
unsigned transport-kind expectation changes to Network, while strict financial
rows, errors, no-prefix/no-gap and signed-path assertions remain. Rebuilt CI
continues to own replay, CPU, size and retained-memory certification.

## Selected managed signed Read custody

One non-Clone engine wrapper retains selected opaque identity, egress/evidence
and original Context through queue and retry, with exact typed refusal and
pending return. Missing-selection command diagnostics precede a single typed
terminal under genuine local authority. Provider/prepare metadata and ordered
recipe headers remain unchanged. Genuine engine controls cover capacity,
duplicate entry, phase custody, expiry, retry, streaming and account evidence;
native compatibility controls do not substitute for later venue adoption.

## BingX identity original Read owner

One AdmittedRead couples selection, credential and terminal authority without a
venue admission or numeric fallback. Exact managed delivery is checked before
native response decoding/publication. Local config precedence and 20s/Never
remain native; typed transport/refusal errors keep checked diagnostic ordering.
Only identity.rs loses two legacy terminal occurrences; other inventory is intact.


## Shared CPU remeasurement controls

Policy controls cover mean passing and failing, missing/failed second samples,
5% boundaries, all three objectives, category changes, output/report tampering
and unchanged strict failures. Runtime controls cover archive/path/type/link
validation, prevalidated restoration, exact Node/dependency/module identities
and initial/final loader cache snapshots. Coordinator controls reject foreign
same-source run/job payloads, substituted physical runners and missing runtime
authority, duplicate/retried second jobs and incomplete suite/case rosters.
Workflow controls pin all 32 final required names and 110 original workloads,
allow only an eligible projection to schedule a second runner, and require final
certification even after missing or failed upstream work. Declared public FAILs
remain declared results without CPU rescue. Measurements in CI report first and
second samples separately; this methodology makes no runtime speedup claim.

The first real second-runner archive exposed a mismatched transport bound. The
shared size validator preserves the original bound in an extraction before
regressions are committed: 5 RED with 16 working controls. The observed
743,167,768-byte runtime case is checked without allocating a giant buffer;
metadata/log/ordinary bounds, 4 GiB overflow, empty response recovery/exhaustion,
mixed socket/empty budgets and CDN credential stripping remain explicit controls.
Routing tests cover every registered suite and reject unknown runtime names.

## Selected multi-page Read contract controls

New-contract Engine controls cover exact receipt bounds, distinct page legs,
immutable selection, wrong-root/foreign-engine delivery, queue/provider refusal,
discard/requeue, streamed order, stale staged rows, typed root retirement and
physical-timeout recovery. Existing single-leg controls remain unchanged. API
privacy doctests distinguish original roots from children without claiming that
borrowed Contexts lose generic core authority. Native consumers remain separate.


### Replay audit finalization controls

The behavior-preserving extraction has four working controls and two assertion
REDs matching the historical WOO false protocol diagnostics. After final-only
aggregation, all 31 focused audit/finalization/checkpoint/WOO protocol controls
pass. Genuine late private violations remain visible; audit exceptions propagate.
Both real replay callers and forgotten-final-flag mutations are checked.

Native accepted-write snapshot races are separately reproduced by four REDs
and two working controls; the identical six controls pass against queued #1788.
Those venue changes and their tests are outside this standalone harness scope.

## Hyperliquid perps history custody regressions

Original test-only04e6ee356ad ran16 real-engine controls on unchanged venue
production:11 assertion REDs,5 working native positives,0 ignored. All16 now
pass. Four adoption controls add real root/page timer ordering, strict forced
dequeue-capacity refusal with zero reservation charge, exact-owner cleanup with
Account/History FIFO in both orders, and typed public Auth* metadata without
private health evidence. Existing22 native history and11 cached-lookup fixtures
retain financial assertions through shared managed preparation; long page-cap
fixtures request the maximum caller wait and drive the actual native budget wake.
Focused Hyperliquid+Binance suites pass568 tests/45 targets; the request-context
checker unit suite passes15. Full inventory provenance awaits the replacement
#1976 composition carrying current incoming Aster/OKX prunes. Independent source, caller
and full-map reviews and fresh CI remain required before freezing this consumer.

## OKX history caller-root controls

Revised test-only916a3e65f5a runs15 controls on unchanged OKX production:9 genuine
assertion REDs and6 native positives, zero ignored. This retains the original
fourteen-control checkpoint and updates two whole-root expiry expectations to
the newer strict engine contract; the added parked-root timer assertion pins
the missing shared tracked-root behavior. Subsequent review counts all correlated
terminals and forbids owned incompleteness on expiry. The implementation target
now passes20 controls, adding Primary preparation intent, pre-release versus
released20s child expiry, typed preparation metadata (query/lookup/public), real
registry-capacity refusal at paced dequeue with peer preservation, and retired
pacing/late-data isolation after command-ID reuse.

Existing17 history,8 lookup and5 dispatch-signer controls pass with financial,
identity, signature and absence assertions retained. Fixtures perform actual
managed preparation; deferred-provider refusal drives the route-then-sign seam
explicitly, since the immediate fixture deliberately rejects parked signing.
History response clocks use their original relative offsets on the command's
absolute epoch instead of running backward. Primary physical routeNone is checked
as the host's default primary route, and the dedicated intent control proves the
same primary egress before signing. Fresh complete native, strict/fmt and inventory
provenance results are recorded in the plan checkpoint; CI and replacement#1976
composition remain separate readiness gates.

## Crypto.com managed candles and local history controls

Original regression commit cc2916a8037 precedes source edits. Fixture correction
b89ca9aaa52 permits only engine Cancel cleanup and preserves native streamed
malformed-body behavior; its independent original-source rerun proves nine
assertion REDs and seven native controls, with no compile failure counted. All
sixteen pass after the cutover. The discarded Internal expectation on ordinary
malformed text is explicitly excluded from RED evidence.

Actual managed book peers fill256 owners; local and candle admissions refuse
without dispatch, then recover after one peer settles. Controls cover invalid
clocks/finite ID reuse, single preparation/bind/claim, parked/released expiry,
stale callbacks, typed setup/network errors, native full error fields/auth health,
raw candle order/timestamps/numeric coercion, all timeframes/live-UAT roots and
credential combinations. All five local branches, native validation precedence,
explicitly ranged Income limit0, and valid private/public history continuations
remain controlled. Exact scanner prune removes only Crypto lib/history entries;
no existing assertion is removed. Full native/replay/size verification follows.
Local Crypto.com/Binance verification passes432 tests across41 targets, strict
Clippy/fmt and fifteen ratchet controls. A later independent-review callback
control exposes the engine's credential-free public typed-error auth-health
accounting defect (health1 versus0); shared-owner closure remains required.
The adapter preserves kind/metadata and adds no local demotion or exemption.

### Crypto.com refreshed original and managed controls

The test-only original branch is genuinely merged onto actual6a6845c4491; its
expanded checkpoint1d576566cb4 runs both targets with --no-fail-fast and proves
16 behavioral assertion REDs plus seven native positives, zero ignored. Seven
added controls pin private range/nanosecond and unsupported-interval admission
at bad clock/full capacity, retained genuine peers and real settlement recovery,
and stale bind/item/failure/timer isolation with a distinct exact replacement row.
The original public-auth-health regression from27b051a8907 is retained verbatim
and passes against the actual shared fix. Managed targets pass24 controls.

One original working expectation is intentionally updated: malformed delivered
text at200 keeps its exact message but now carries shared Internal/NotSent. The
unchanged native baseline is preserved in the original checkpoint; this change
is declared in CALLERS rather than hidden by stripping the engine's typed cause.
HTTP500 still returns its native string-only error. Existing financial assertions
remain; tests/public.rs only drives the genuine prepared executor at its old clocks.

### Lighter local history Read evidence

Genuine256peer roots exercise seven local terminal capacity refusals, slot/ID
reuse and invalid-clock recovery. Native result/error precedence, all four
Lighter/Robinhood deployments, duplicate custody, repeated immediate completion
and saturated remote byte/schema controls remain covered. All19 focused tests
PASS after original4GREEN/15RED. No shared policy, timer, wrapper or scanner change.

### GRVT local history Read evidence

Seven capacity controls use256 genuine retained Read peers. Invalid-clock,
retirement/reuse, duplicate-owner and2800 immediate-terminal controls accompany
native precision/error-precedence checks on both deployments. Saturated public
trades retains exact legacy POST route, headers, body and nonempty output schema.
All19 focused tests pass after15 original assertion failures. Existing source
outside the seven branches, held fixtures and foreign inventory stay unchanged.

## KuCoin refusal admission controls

Ten original assertion REDs cover eight lexical local refusals, including all
three Spot unsupported variants. They pass after admission conversion, alongside
native error/credential ordering, UUID entropy, Store preservation and public
wire controls. Duplicate ownership and more than 256 fixed-clock reuses pin
retirement; saturated-registry valid private wire controls pin unchanged signed
fallthrough. The isolated suite contains 18 executed controls, with no ignores.

## Bitrue initial refusal regression controls

Three original compiling capacity assertion REDs pass after shared entry
adoption. Seven isolated controls also pin keyed/keyless/public-only ordering,
Store preservation, capacity recovery, successful native one-way mode, saturated
signed wire and genuine duplicate-owner protection with 270 fixed-clock reuses.
No original test is ignored or rewritten to claim a legacy removal.

## GRVT final-coverage replay controls

Healthy final completion, pre-ACK checkpoint observation, genuine earlier
private-data faults, late selector faults and incomplete replacement generations
are controlled through the real shared audit/capture APIs. The scoped GRVT and
declaration-policy run passes 32 tests. Verification of the authenticated reports
with the narrowed manifest retains one public PASS and one declared private FAIL,
zero unexpected failures and zero unobserved declarations. Five-pair repeatability
and fresh composed CI remain separate qualification work; no native parity is
implied by protocol coverage.

### Crypto.com terminal audit controls

Twelve real-audit controls cover healthy authentication and heartbeat exchange,
pre-ACK checkpoint observation, persistent invalid HMAC and early subscription
faults, a final missing heartbeat before memory collection, and the distinct
TS four-channel/Rust five-channel requirements. The original capture has two
assertion REDs with ten working GREEN controls; final-only capture passes all
twelve. Full raw comparison excludes only top-level metrics and protocol errors,
retaining all other receiver fields and the sixty ordered financial/identity
failures. The nine removed protocol declarations each require exact current-tape
absence; every other manifest byte is preserved.

### Binance Spot acknowledgement coverage controls

Sixteen actual-audit controls preserve independently signed subscription bytes,
exact typed ACK correlation, pre-ACK event rejection, explicit closed/connected
generations and final unanswered requests captured before memory settling.
The original capture has two final-assertion REDs and fourteen GREEN controls;
terminal-only capture passes all sixteen. Full raw comparison omits only
metrics and protocolErrors, retains every financial field and ordered fixer,
and proves each of the four removed current-tape protocol keys absent. Other
public/private Binance cases keep their exact declarations and verifier status.

## MEXC local write admission controls

Original test-only checkpoint2c0755b5607 executes13 actual-Engine controls on
unchanged production: nine behavioral assertion REDs plus four working native
positives, zero ignored. A diagnostic-fixture mismatch was corrected before
qualifying REDs. The same13 controls pass after source conversion.

Both reachable branches are exercised at invalid clock and full256-peer capacity;
additional placement shapes prove UUID entropy ran before validation/key refusal.
Controls retain every real peer preparation, unchanged Store snapshot and exact
recovery after one real peer failure, duplicate-ID isolation,300 ID/lock reuses,
exact diagnostic/terminal order and validation-before-key precedence. A supported
private Mode command still emits its native legacy POST/body. Credential-free
AuthInvalid keeps its fields but supplies zero auth-health evidence.
508 MEXC/Binance native tests across43 targets, strict all-target/all-feature
Clippy, scoped fmt, full bans and both provenance checks (15 checker tests) pass.
No existing financial/identity/signature assertion changes. CI remains the
WASM/replay/CPU/size/memory authority; no performance/readiness claim here.

## KuCoin Spot fixed-setting admission evidence

Ten native LocalSetting controls cover six original capacity/expired-wait REDs,
constant projection, unknown symbols, configuration precedence, Background-only
capacity and duplicate original-owner release. Tests use genuine command
receipts; no missing-method compile failure or fabricated Context is evidence.
Exact Spot trading inventory removes only the two-occurrence success row.

## Bitrue local one-way setting controls

Seven native controls retain two original assertion REDs plus five working
controls: command-owner capacity/recovery, expired/sub-ms receipt, duplicate
original owner/diagnostic, Background Context independence, all credential
configurations, true-mode refusal and absent symbol behavior. No fabricated
Context or missing-method compile failure is evidence.

## Flat-position unavailable PnL projection

Original financial-evidence controls reproduce two assertion failures: native
NaN creates a null-PnL row, and native/TS unavailable evidence differs. Five
working controls retain real finite PnL, numeric strings, non-flat positions and
genuine PnL mismatches. Admission now uses the same finite conversion as the
projection, preserving zero and losses while excluding unavailable sentinels.
Declaration pruning requires fresh inverse replay evidence for each removed key.

### Public refusal and catalog custody controls

Paradex uses genuine retained peers to pin local candle and book-entry capacity,
clock/reuse and duplicate custody, with native GET/schema and saturated candle
fallthrough controls. Lighter pins exact public catalog wire across four hosts,
spot filtering/precision, endpoint fallback/backoff, real Prepare/Bind/Claim,
preclaim/released expiry, failed streamed prefixes and inert stale/duplicate work.
A255-peer registry-pressure control forces each catalog attempt to retire before
fallback/retry and successful slot reuse; expiry cannot conceal a root leak.
Existing startup fixtures retain their assertions while using shared preparation.

## Joined order-read authority: WOO and Crypto.com

WOO's regular/algo pages and Crypto.com's regular/advanced reads carry one owned
Store token across the logical census. All native classification/membership and
initial execution-baseline publication is inside the admitted callback; later
metadata/repair HTTP and timers stay outside. ACK invalidation follows the
existing validated order-only success boundaries, including absent rows and
partial success. Per-ID WS progress, terminal fences and rejection retention
remain unchanged. Capture/late exhaustion, accepted/refused/malformed ACKs,
both WOO arrival orders, Crypto auth/transport retry after ACK, WS peer progress
and later WOO amend/fill behavior have bounded native controls. Native original
RED receipts distinguish the two accepted-write crossings from API-adoption
controls; no live/private or full-parity claim follows from local tests.

## BloFin local leverage Read controls

Five native tests retain two compiling original capacity assertion REDs plus
three working controls: exact validation/credential precedence, Store/empty
financial state preservation, duplicate original-owner release, and 300 sequential
failures with 255 Background peers and one free Context slot. Capacity recovery
releases exact Background roots. No fabricated Context, numeric fallback or
missing-method compile failure is regression evidence.

## OKX closed-position managed controls

Ten original compiling assertion REDs and six native working controls now pass
through the managed adapter. Five additional managed controls pin dequeue-time
child bounds, release-aware typed timeout fields, provider/preparation metadata
and accepted-prefix recovery, reversed equal-deadline ingress order, and an
explicit180s caller receipt completing after the default120s boundary. All21
controls and all9 existing closed-position tests pass without ignored tests or
removed financial assertions. Existing fixtures only drive managed preparation
and advance each native100ms page clock. Capacity uses genuine retained peers;
reuse controls now reach and pass their stale-timer/DATA assertions. Family
WASM/replay/CPU/memory certification remains CI's responsibility.

## MEXC unsigned Background Read verification

Plan3690a27236f precedes original controlsc58c123d68c:18 executed controls,
11 assertion RED/7 working GREEN/zero ignored against unchanged production.
The same18 assertions pass after managed adoption. They cover parked dispatch,
preparation refusal, deadline equality, expired stream end, native startup retry,
exact500ms depth recovery, old-response isolation, a concurrent live symbol,
malformed-envelope429 precedence, wire values and buffered version replay.
All221 MEXC native tests pass across18 targets including the empty doc target;
strict all-target/all-feature Clippy, scoped fmt, full bans and both request
ratchets (15 checker tests) pass. Existing native test expectations stay intact;
public fixture collectors now drive the shared prepared-action executor at their
explicit event times. Signed secondary probes use an inert Identity continuation
instead of a public Markets continuation.

Two independent runtime findings received their own assertion-RED commit221bfbfc2ff
and fixbec93dd562a: expired cleanup metadata must not suppress native failure
recovery; accepted catalogue custody must retire before successor admission. All
20 controls now pass; refreshed223 MEXC tests across19 targets and305 Binance
reference tests across26 targets pass (target counts include empty doc tests).
Strict Clippy/fmt, bans and both inventory checks pass after the fix. Independent
source/adversarial, caller and full-map delta reviews are CLEAR atbec93dd562a;
architecture gate No. CI family size/CPU and other CI gates remain pending.


### Hyperliquid spot candle custody controls

Eleven dedicated controls cover standalone/merged spot on both networks, exact
native wire and rows, validation order, real Prepare/Bind/Claim, early delivery,
original deadline, preparation failure, clock/capacity recovery and duplicate
owner/delivery. All pass with zero ignored. Existing candle and spot fixtures
retain native assertions while driving managed preparation. The new probe opens
no socket, so it makes no standalone socket-composition claim.


## Crypto.com immediate trading managed controls

Plan and original controls were committed before production. Re-executed on
actual db6fa45bbb7 with unchanged source, the dedicated target produced 18
assertion REDs and 8 working GREEN controls. After adoption all 26 pass, including
all three non-finite clocks, full genuine-owner capacity and recovery, duplicate
live owners, native validation/key precedence, missing-key auth-health isolation,
fixed-mode Store preservation from either initial value, warning order, repeated
slot reuse, private placement bytes/progress and legacy empty jobs. No ignored
tests, removed financial assertion or fabricated custody; broad replay and
WASM/CPU/memory checks belong to CI.

The scoped Crypto.com suite passes 174 tests across 16 targets; strict all-target
Clippy, formatting, bans and the 15 request inventory tests pass. Read-only
credential scanning confirms no key-selection term change, and all production
outside Crypto.com trading.rs stays base-exact. Original controls remain
byte-identical to their RED checkpoint. CI is still required before readiness.


## Crypto.com public discovery managed controls

Plan preceded original compiling controls: 8 assertion RED/7 GREEN, then a
caller-review live-prefix/expired-stream-end RED produced 9 RED/7 GREEN.
All 16 now pass, covering Public preparation, parked expiry and late readiness,
exact deadline in both ingress orders, cached quote preservation, genuine owner
capacity/recovery, catalogue-to-ticker slot handoff, native wire/values/backoff,
private startup ordering and expired-metadata sibling recovery.

Two no-I/O assertions were corrected to permit only shared ManagedHttp::Cancel
cleanup, after a diagnostic proved Cancel was their sole unexpected action;
Http/Prepare/Bind/WsConnect remain forbidden, and Store assertions stay exact.
Existing financial and signing assertions remain. Fixture preparation uses
existing event times; cancelled deadlines are removed from the fixture's active
timer list as a host would. Signer probes gain a genuine unused carrier Context,
with no new managed-signing claim. Broad CI gates remain pending.


Discovery closure adds a seventeenth managed control: raw ticker preparation
remains parked while native private startup proceeds, then real prepare/bind/
claim releases the ticker and publishes quotes. The overseer accepted this
independence; CALLERS names the physical-order consequence. All three bounded
reviews are CLEAR at a9282a7e787. Refreshed scoped suite: 165 tests/16 targets,
zero ignored; strict all-target Clippy passes. Actual landing integration retains
all incoming foreign changes and only the discovery singleton inventory prune.
