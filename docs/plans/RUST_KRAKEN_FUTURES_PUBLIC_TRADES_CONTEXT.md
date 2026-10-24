# Kraken Futures unsigned public-trade history custody

Plan-first base: exact actual9a757d406b8. Branch:
feat/rust-kraken-futures-public-trades-context. Scope approved by claude-ddi2il:
ONE request occurrence. No combo5b source or key-selection changes. No production
in this checkpoint; plan and compiling original assertion controls only.

## Inventory and collision boundary

Kraken baseline27 occurrences/23 fingerprints includes four candle occurrences
owned by2058. This slice adopts only history.rs:527 request_streamed for
Read::Trades, fingerprint
6c7a880f942512575a92c8c8bd1fd04a85df37e7e7bb0a4e0b134b338cdd542e (1).
Baseline27->26; after2058 lands23->22. history.rs6->5, with every other
fingerprint/count retained. Four history entries are conservative format-macro
fingerprints involving a variable named request; they are not extra HTTP sends.
Never rename those bindings or rehash unrelated expressions to claim a prune.

Open file map checked:1876 owns the managed-write plan, spot_write_adoption_red
and trading tests;1798 owns Spot book/CRC32;2058 owns Futures candle lib hooks and
its secondary probe compatibility carry. Hcmq3f released history/public-trade
regions with no unpublished claim. Preserve all those paths. This branch keeps
2058/2033 heads untouched. Later production callback wiring must plain-compose
with actual2058 after its landing, preserving candle handling exactly.

Private Fills/Income/Orders and signing funnels stay legacy until their separate
signer/authority adoption. No credentials, Selection, Plan, hosted-signing,
provider, key material, account cycle, trading or Spot changes. Existing plain
caller Read-root and ReadCursorHistory APIs are present on this base; no pending
engine PR is required for the public walk.

## Native wire, rows and traversal

Query::PublicTrades is unsigned GET on futures.kraken.com or demo-futures.kraken.com:
/derivatives/api/v3/history?symbol={lowercase caller symbol}; later pages retain
native sorted form order: ?lastTime={ISO}&symbol={lowercase caller symbol}.
Keep native form escaping, empty headers, absent body and native egress. No market
lookup or symbol normalization is added; output retains the caller's symbol.
Stream ArrayAt::Field("history"). There is no native request_timeout declaration.

Native limit is100 when omitted OR zero; positive limits are not newly clamped.
Only output is truncated. Parse exact numeric/string trade_id without f64 loss;
nonempty uid wins, otherwise trade_id. Strict ISO timestamp remains milliseconds
in output; it is not candle seconds. Preserve native side, positive price/size,
finite quoteQty=price*size, duplicate identity winner, descending timestamp sort
and row encoding. Public rows are not filtered by the implicit history range.

Current page geometry observes every decoded identity/time, raw count, fresh
count and oldest time. A page with raw count<100 OR accepted+staged>=limit stops.
Otherwise resume at ISO(oldest.floor()+1ms). CursorWalk has50 pages maximum,
empty/no-new-row stall checks and repeated-boundary protection. Full pages can
hold equal timestamps; preserve existing progress checks rather than guessing a
new cursor. Unwalkable coverage is an explicit incomplete error, never invented
complete data. No retry/backoff is added to this read.

Retain envelope success(status2xx, native result/status/error rules and required
history array), native failure messages and strict codec validation. A malformed
page's staged prefix is discarded. Existing HistoryRows behavior recovers only
NONEMPTY accepted rows after a later actual transport/native/codec failure,
issuing the owned HISTORY_INCOMPLETE marker before the one successful accepted-
prefix result. First-page failures remain failures. Empty/filtered accepted
vectors cannot authorize partial recovery; identity counts alone are not rows.

## Managed public authority and changed bounds

Keep existing local history-range validation and OrderByClientId refusal outside
this public remote path. At genuine PublicTrades entry admit exactly one
admit_caller_read_root(None,None), before remote effects. Map Rejected unchanged
(one refusal already emitted); Revoked uses the existing expired-entry helper.
No guessed timeout, numeric completion fallback, re-admission or renewed root.
Bind ReadCompletion::from_context to that original command/root once and retain
ReadCursorHistory across every page.

At each eligible page derive one Read/Never/credentialNone/no-pacing physical
child, timeout20000ms, from the ORIGINAL root. Managed unsigned HTTP plus
stream_response(Field("history")) preserves the wire. Validate exact root/leg/
Attempt delivery before row parsing, diagnostics, fresh identity mutation,
staging or envelope acceptance. Use ReadCursorHistory::admit for row/identity/
page progression; codec success remains separate from authority. Retire the
physical child only after accepted-page admission and before the next page.
Only the original ReadCompletion owns result/coverage marker and terminal.

The20s physical admission-to-expiry bound is a NEW caller contract, including
stuck route/Prepare/Bind, distinct from native timeout_ms=None. CALLERS must say
so before source/freeze. The exact original caller receipt caps every child; no
new logical50*20s timeout, native retry or deadline renewal. Root expiry is
engine-owned strict Timeout: drop all pending/staged/accepted state, no partial
marker/result or next page. No venue timer/stop flag/root-retirement registry.

Typed pre-release preparation/provider/queue failures are STRICT, including
when earlier pages were accepted: ReadCursorHistory::reject, no partial marker,
no HTTP or retry. Preserve shared failure kind/code/retry metadata and local
NotSent instead of string collapse. This new preparation branch had no native
raw equivalent; CALLERS must distinguish it from existing later actual failure
recovery. Released Network/physical Timeout under a LIVE root may use the native
accepted-only recovery layer. First released failures retain the shared typed
cause; native HTTP/codec failures retain their native error projection. Exact
constructor/failure settlement fit gets bounded core review before production.

Root and child each consume a registry slot. Initial256-peer Capacity refusal
is one shared error with no effects; freeing one slot admits a root whose child
can still refuse Capacity/localNotSent. That failure must release the root slot;
free exactly one more peer and success proves no leak. Validate all255 then254
survivors using genuine contexts, never fabricate permits or inspect next IDs.

## Planned source boundary after originals/review

Prefer a separate Futures public_trades module and explicit PublicTrades pending
carrier, removing only the old unsigned branch. Private history keeps its native
CursorHistory, signed requests, error projection and original semantics. Remove
only newly orphaned public-only helpers; do not duplicate legacy completion or
retain an alternate raw public path. Native wire/row helpers may be shared only
with byte-equivalent codec behavior. No generic private funnel or inventory name
changes. The sole intended prune is the public request_streamed fingerprint.

New typed preparation/HTTP failure hooks route PublicTrades while retaining
2058's Candles branches and each foreign default exactly. Actual2058 composition
is a venue source-order prerequisite, not a new engine capability. Original
public/private history tests and existing candle assertions remain intact.

## Original assertion controls

Use a thin forwarding Venue probe with actual startup catalogue/ticker replies.
Keep source/Cargo/maps/existing fixtures byte-exact9a757. New isolated target only.
Real engine admits256 held Read peers; retirement validates every survivor.
No fabricated Context, Attempt, witness, signing success or numeric fallback.
The shared native/managed packet harness drives actual Prepare/Bind/Claim so
working wire/codec controls survive adoption. Disclose first-assertion REDs and
any gated suffixes, excluding fixture setup/compiler failures from evidence.

Pin both hosts/unsigned exact query/escaping, default and zero limit, output
identity/uid/ms timestamp/economics/sort/dedup, short/empty/full pages, two-page
cursor, later native/transport/codec accepted-only recovery with owned marker,
first/partial-staged failures, repeated boundary and50-page cap. Custody controls
pin Prepare before HTTP, genuine Capacity and exact two-slot recovery, duplicate
command ID, NaN clock, fractional raw receipt and root expiry after accepted rows,
new child per page, strict typed preparation refusal on first AND later page,
stuck Prepare/Bind20s under live60s root, late data/claim/preparation inertness,
and live-root physical Timeout versus strict root expiry. No production until
original/native/core API review and source authorization.

Local cargo: changed Kraken crate/new target only, CARGO_BUILD_JOBS=4, one build
at a time. CI owns workspace/all-feature/WASM/replay gates. Plan+RED work does not
change or merge frozen2058/2033, and no PR/READY/production claim is implied.

## Reviewed recovery and physical-timeout binding

Fe7's existing-API fit confirms original-Context ReadCursorHistory authority,
strict reject versus accepted-only finish_with, and strict root expiry. The
review's two initial pins were corrected against actual source and native tests:
- claude-ddi2il explicitly ruled preserve native accepted-only recovery after
  later HTTP503 AND malformed DATA. A stricter DATA policy needs a separate
  explicitly scoped PR. Fe7 withdrew the strict-DATA suggestion independently.
- validate_read_http_delivery correctly rejects a physically expired child, so
  it is NOT called to bind physical Timeout. Engine on_http_failed dispatch is
  already exact released-attempt custody; finishing a child removes its managed
  entries. The callback Pending carries the current page child, which must match
  the history's retained current child. Validate the ORIGINAL root with
  validate_read_root_context; expired/revoked root emits nothing. Then finish the
  child, discard_staged_page and finish_with(Err) for accepted-only recovery (or
  strict failure if accepted Vec is empty). No failure-validator/authority seam.

Controls pin late failure from an already completed child, page2 physical
Timeout recovering page1, strict original-root expiry before a later failure,
and streamed-but-unaccepted page2 prefix discard. The initial native cursor
fixture mistakenly expected lastTime after symbol; corrected to actual sorted
form order lastTime then symbol. That fixture failure is excluded from RED
receipts. Source remains unchanged; no stricter DATA assertion replaces native
working controls. Production still waits for original reviews and actual2058
callback composition.

## Original executed checkpoint

New target compiles on unchanged9a757 and executes29 tests:16 intended assertion
REDs,13 working GREENs, zero ignored. REDs expose raw HTTP before public Prepare
(first and later page), capacity/duplicate/invalid-clock bypass, absent raw
fractional receipt/physical20s timers, late successful or accepted-prefix result
past the original root, and absent physical timeout boundaries. Physical20s
assertions are NEW-CONTRACT policy controls, not existing native timeout bugs.

The first Prepare assertion gates deeper Bind/Claim, typed pre-release fields,
late preparation/permit, later strict refusal, exact capacity two-slot recovery
and255/254-peer survival suffixes. No executed proof is claimed for those gated
suffixes. The three root-expiry controls genuinely receive native success/partial
success instead of strict Timeout. No missing API/compilation failure, fixture
panic or setup error is counted as RED. The initial cursor query-order fixture
failure was corrected to native sorted form before this receipt.

Working controls pin both hosts and unsigned wire, empty/default/zero/positive
limit, unknown mixed-case symbol/form escaping, exact ID/uid/millisecond/side/
quoteQty encoding, cursor oldest.floor()+1ms and final sort/dedupe, native later
transport/HTTP503/codec accepted-only recovery and marker-before-result,
first/staged-page failure without false partial success, repeated-boundary and
50-page failure, stale completed-child failure silence, interrupted streamed
page2 prefix discard, and zero-wait external rejection. Native DATA recovery
controls stay unchanged under the overseer's explicit parity ruling.

Scoped target strict Clippy and rustfmt/diff checks pass, jobs4 sequential. All221
existing Kraken/engine/crypto/registry/checker/Cargo paths remain byte-exact9a757.
Only this new plan and new target differ; no source, inventory, selection or
existing fixture change. Original/native review remains separate from these
owner execution receipts. Actual2058 composition is still required before
production callback wiring; do not merge its frozen head into this checkpoint.

## Production composition and executed-control adjustments

Actual #2058 is now landed at 883c5c8deb8 and plain-merged into this branch.
Production separates only unsigned PublicTrades into public_trades.rs, with an
explicit pending carrying the current child beside the owned history. The
private history codec helpers Atom/side/ISO/stall text are shared without a codec
change; the public-only Trade/TradeWire definitions move unchanged. Existing
candle functions remain unchanged; typed callbacks add only PublicTrades arms.

Released Network/Timeout callbacks bind the pending's exact current child and
validate the original root without applying the DATA validator to an expired
physical child. DATA and integrity failures validate delivery before processing.
Both discard the unaccepted page before native accepted-prefix recovery. Typed
preparation and queue refusal remain strict through ReadCursorHistory::reject.
Root-only history authority owns markers and completion; no numeric fallback.

After composition, the first scoped run passed 28/29 controls. The NaN control
stopped in its helper because the landed host handle_command now rejects a
non-finite receipt before dispatch. It now uses the existing external Event
entry as the landed candle control does, preserving no-next/typed-terminal
assertions. An additional control pins handle_command -> Err(Invalid) with no
engine actions. No compiler or fixture panic is counted as adoption proof.

The existing mixed public/private history failure control expected Internal for
all transport failures. The reviewed shared-cause contract now preserves Network
for PublicTrades only. That one case gains an exact Network/native-message
assertion; every private transport, HTTP and malformed-DATA Internal assertion
is retained. The initial original 29-test checkpoint and its 16 RED/13 GREEN
receipt remain historical evidence, distinct from these disclosed adaptations.

The complete changed Kraken crate now passes 320 tests (zero failed/ignored),
including 30 PublicTrades controls, 29 history controls, 28 candle controls and
all in-crate carrier matches. The first rerun hit disk exhaustion writing Cargo
incremental cache; only this worktree's disposable incremental cache was cleared,
then the same scoped crate run passed. No source/test failure was masked.
