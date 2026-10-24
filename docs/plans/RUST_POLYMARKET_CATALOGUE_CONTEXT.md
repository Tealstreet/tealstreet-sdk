# Polymarket Gamma catalogue page custody

Plan and original controls begin on actual `0ee8c544a22f32e0861a0976da9da0c5a3b93c38`.
Owner: hcmq3f. One unsigned streamed sender in `venues/polymarket/src/lib.rs`,
fingerprint `3c2518cd3efa9ec044158c6dd5008cc36722ff550877663df294c8c8c2d55621`,
count one. Actual family inventory is 14 occurrences; this slice yields 13,
or 12 after the independent midpoint #2057 prune actually lands.

## Native boundary

Keep GET `https://gamma-api.polymarket.com/markets/keyset` with active=true,
closed=false, limit=100, exact repeated encoded slug filters, optional encoded
after_cursor, empty headers/body, and streamed `markets` array. Retain native
100-row page validation, opaque cursor cycle/empty/missing-cursor checks and
10,000-cursor ceiling. No new logical traversal deadline or page limit.

Rows and prediction metadata accumulate until the final page succeeds. Preserve
full token IDs, duplicate ID/symbol suppression, malformed-row diagnostics and
skipped-slug tracking, native precision/limits/active flags, and atomic catalogue
publication. Failure on a later page discards the prefix. Preserve existing
Backoff<Option<usize>> catalogue retries from the first page; clear native walk
dedup/cursor/skipped sets only on that retry path, as today. Midpoint issuance,
private startup, live ticker overlays, accounts, history, trading, signing,
credential choice and all held engine/host/5b files remain outside this slice.

## Existing authority composition

Each physical catalogue page obtains a genuine Background Read Context through
the existing `http_context(Scope::Background, public_read::policy(Read))`, then
`request_managed` with the native ArrayAt::Field("markets") stream declaration.
Retain that exact Context in Pending::Markets and the current catalogue-page
owner. Do not invent a command, root, Session, Plan, local issuer, credential,
generation counter or raw-request wrapper. No engine or Cargo change is needed.

The 20,000 ms admission-to-expiry policy, including stalled Prepare/Bind, is a
NEW caller-visible bound; native raw discovery declares no request timeout.
It applies independently to each page and must be recorded in CALLERS on
adoption. There is no aggregate 20s catalogue-walk budget. Retry is Never at
the physical Context; native discovery Backoff owns recovery with a fresh page.

Every streamed item and final response must check current exact Context and
Background DATA authority BEFORE row/envelope decoding, diagnostics, walk-set
mutation or publication. Retire the old physical Context before issuing the
next page or entering native failure/backoff. Typed preparation/transport/timeout
cleanup is failure-only, not gated on DATA authority. Stale callbacks cannot
clear a newer current page or schedule its retry. Exact current cleanup on all
malformed/envelope/cursor branches must release capacity once.

Unissued admission/queue refusal retires any admitted Context and schedules the
same native first-page retry, with no recursive page admission and no publication.
This pressure boundary adds no private/auth evidence or CommandNotice. Preserve
native retry messages and severity where existing paths supply them.

## Plan, original controls and source gates

Commit this plan before a new isolated tests/catalogue_context.rs target. Keep
production, old fixtures, inventories and selection maps byte-exact to the base.
Original RED candidates: eager HTTP before Prepare; final or streamed DATA at
the new page deadline still mutates/publishes/advances; no managed expiry timer
while unprepared; and shared-owner pressure bypass. These are adoption-bound
controls on a legacy path, not an existing admitted-root vulnerability claim.
Do not count compile failures, fixture errors or assertions after the first
failing assertion as executed evidence.

Working controls pin exact wire/filters; opaque two-page atomic accumulation;
failure/cycle/full-page/malformed backoff and no prefix publication; malformed
row neighbour retention; final metadata and uint256 identity; and no successful
empty publication. Private-startup independence remains a separate existing-
fixture/source-stage check; all new original fixtures are public-only. Deadline failure must
preserve retry rather than prohibit all post-expiry work. Streamed expiry must
also prevent expired rows from poisoning dedup state on a subsequent retry.

After independent plan/original review CLEAR, implement the bounded source,
preserve all original assertions, and add real Prepare/Bind/Claim, late-host,
stale-old-owner and capacity/recovery/credit-release proofs as needed. Existing
fixture collectors may use prepared_actions, with all wire/financial assertions
unchanged. Obtain exact source/API and native/caller/map reviews. Push draft
and dispatch fresh CI in parallel, per standing authorization. Actual midpoint
composition must preserve both exact Context lifecycles and native progression;
no inventory from a pending PR is assumed landed.

Local Cargo is limited to changed Polymarket tests/crate with
CARGO_BUILD_JOBS=4, one build at a time. Full workspace, WASM/size/performance
checks run in CI; do not change budgets/profiles or waive enabled-adopter limits.
READY requires source reviews CLEAR plus green CI, followed by auditor notice.

## Original checkpoint receipt

Plan aef0f018659 precedes original controls 3b1651f907e. Against unchanged
actual0ee8 production, the isolated target compiles and executes twelve tests:
five intended assertion RED, seven GREEN, zero ignored. Eager raw HTTP escapes;
first- and second-page DATA at their exact new deadlines still publish;
expired streamed rows reach native malformed-row diagnostics; and the
unprepared page has no admission expiry timer. These are adoption boundaries.
Late-host/timeout/backoff suffixes after each failing assertion remain unexecuted
until adoption and must not be counted as original executed evidence.

Working controls preserve exact repeated-slug GET wire/no private intent,
opaque cursor/dedup and atomic metadata geometry, a successful 38s total walk
with each page under 20s, actual later transport failure with prefix discard and
first-page retry/recovered dedup state, cycle/envelope/full/oversized-page
refusal, malformed-neighbour survival and empty-catalogue retry. No production,
old fixture, Cargo, inventory, selection or shared-file change is present.
An initial extra delimiter in the new fixture stopped rustfmt/compilation;
it was corrected before the committed executable original checkpoint and is
excluded from all assertion-RED counts. No native assertion was relaxed.

## Managed source checkpoint

Both original reviews CLEAR at e91f5d6548d (uufub4 core/API, 5c57jl native/caller).
Docs-only 4a82 narrows private-startup evidence to existing native fixtures.
Source artifact SHA256
2855e4062c7f31fdc5f4404c2616109e6a6b06b1a5502274571fa2be676bd806
is independently core/API CLEAR with the unchanged twelve controls passing.
The applied lib.rs is byte-exact to that reviewed artifact result.

Own execution passes all twelve unchanged originals plus four additional real
admission controls: 256-peer pressure backs off without recursive admission,
releases peers and recovers; stale failed/late host callbacks cannot clear a
new current preparation; an expired interrupted stream cannot poison retry
dedup; malformed/empty/error cleanup can re-saturate all 256 credits before and
after successful recovery. These use genuine engine Contexts and public
Prepare/Bind/Claim; no fabricated witness or production seam is added.

Scoped Polymarket tests pass 160/160, zero ignored, including native account,
trading, history, public/startup, security and the doc test. Strict all-target
Clippy and changed-file formatting pass. Existing account/trading collectors
change only to prepared_actions at their native self.now; startup fixtures
collect managed releases at their unchanged explicit cause clocks. All native
wire, financial, readiness, timer and failure assertions remain unchanged.
Private startup independent of midpoint success is covered by that existing
startup fixture, separately from the public-only new target. The manifest
removes only the named count-one Gamma sender; all foreign and selection
entries remain exact. Committed source preservation, native/caller review,
actual landing integration, CI/family size/performance and READY remain gates.

Plain merge dce2b925b12 integrates actual ae108f958c6 (#1896 after #2051/#2031).
Only README/CALLERS require manual resolution: full incoming then the own row
or suffix exactly once. Runtime and original controls remain byte-exact to the
source checkpoint. Ten own paths differ from incoming; all 5,688 unowned entries
and every foreign/selection inventory remain exact. Original fixture assertions
are token-exact, including the existing private-startup proof. Fresh integrated
160/160 scoped tests, strict all-target Clippy, changed-file fmt and normal full
bans (including request/signing/socket inventory controls) pass. CI owns full
feature/WASM/size/performance evidence; exact native/caller and committed core
reviews are requested in parallel with the draft publication.
