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
row neighbour retention; final metadata and uint256 identity; private startup
independence and no successful empty publication. Deadline failure must
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
