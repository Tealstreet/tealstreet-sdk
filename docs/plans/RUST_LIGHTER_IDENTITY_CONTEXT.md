# Lighter unsigned identity Read custody

Plan-first base: actual `be710b3f2d7692244144b3f8f10810dc52790796`.
This checkpoint is plan plus compiling original runtime assertions only. No
production, existing fixture, Cargo, checker or manifest edit is authorized.

## Slice and ownership

Migrate the complete unsigned FetchAccount identity candidate chain. The checker
counts exactly three singleton fingerprints in venues/lighter/src/identity.rs:

| Fingerprint | Native site |
| --- | --- |
| 7b3812a7db557b003fb7e0703f4a783dbb56807d3a225c32d012c38243013485 | cx.request physical candidate |
| c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548 | numeric success |
| c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | numeric failure |

Expected identity3->0 and family33->30, with no new fingerprint and no rename
prune. The checked command_error companion must remain, behind the same live
root validation and immediately before its single failure terminal.

Auditor8x and signer ownerm26 confirmed against train/combo-5b-be71 at5506b0d90bc
that #1967 changes only Lighter Cargo.toml, sign.rs, trading.rs and
 tests/dispatch_recipe.rs. Neither identity.rs nor lib.rs is in5b. #1831 adds no
venue path. This checkpoint adds only this new plan and a dedicated new test
file, so it touches none of5b's existing files, including shared plan indexes.

Future source allowance: identity.rs and only lib.rs Identity-specific command
entry, response, typed HTTP/preparation/sign-error and stream-dispatch branches.
Keep account.rs candidate construction and private lifecycle, every other Pending
arm, catalogs, candles, books, history, limits, signing, sockets and financial
reducers exact. No Cargo/http-plans opt-in or new engine seam. Bearer's later
mechanical request_managed None argument is a composition detail, not a current
dependency. Hosted/local/remote signing share Sam's one model; this Read is
unsigned and consumes no signer or Session.

## Native candidate geometry and outcomes

Keep account::identity_candidates exact: a valid application index first; then
unique L1 addresses from application_id and key in that order through account
by=l1_address; then each address through accountsByL1Address. Never use the
API-key index as an account identity. A numeric index plus one address makes
three candidates. Two DISTINCT addresses make FOUR, not three. Repeated
addresses are deduplicated. No endpoint is retried in place.

Keep GET URL/parameter order, no body, empty headers and public routing on all
four production/testnet Lighter/Robinhood deployments. Preserve code/success
handling, current and legacy response fields, first eligible row, exact integer
indices through i64::MAX, own-row referrer selection and omitted absent referrer.
Preserve HTTP/status/parse/missing-row candidate fallback order and final error
text, one command-scoped companion diagnostic before failure, no Store or frame
mutation and no private startup/signer/socket work.

## Existing authority and original deadline

At genuine command entry capture one unsigned tracked caller Read root using
admit_caller_read_root(None,None), before selector refusal or candidate issue.
Keep the exact caller receipt across all candidates; no fresh command, fabricated
credential, timeout default or whole-query20s surrogate. Missing/expired receipt
uses existing shared entry refusal/reject_expired_command semantics. New shared
Read admission errors have no financial outcome. Native final failures preserve
their original unclassified metadata and diagnostic ordering.

Each candidate gets its own unsigned20s/Never child from http_leg_context under
that original root, then request_managed. Validate exact root/current physical
delivery before status, parsing, candidate consumption, diagnostic or terminal.
Retire the completed candidate before issuing the next, preserving the root.
Validate root completion and publish synchronously before terminal retirement.
No response-time Context, local timer/queue, Plan, Session or retry grant.

Typed preparation/provider failures are strict and retain their shared fields;
they do not become native lookup evidence or silently consume another candidate.
A real released transport failure or physical timeout may try the next native
candidate only while the original root remains live; root expiry is strict and
cancels retained work through the engine. Parse/native HTTP refusals still use
native fallback, with no successful-empty identity substitution. No candidate,
late response or duplicate failure may renew the caller cap or publish DATA.

Capacity/invalid-clock/duplicate admission owns one terminal before native
selector diagnostics, spends no outbound and preserves retained peer ownership.
Original receipt expiry retires pending candidates even without a venue event.
A retired-root cleanup callback, if needed, is Identity-only and grants no work.

## Original runtime controls before production

Use a dedicated test target and real Engine command receipts, Prepare/Bind/Claim
and genuine retained peer roots. Production and maps remain byte-exact to base.
Split deployment Prepare REDs so one failing loop cannot hide another network.
Keep working wire/rows/referrer/selector and native four-candidate controls.
Pin local no-selector and remote shared-capacity/clock/duplicate controls,
unclaimed DATA, prepared/claimed original caller expiry, native candidate fallback
without root renewal, strict typed preparation error and late/duplicate callbacks.
Include a delayed first candidate followed by a valid later physical20s window
under a longer caller root: no guessed whole-query20s contraction.

Report actual first assertion failures and gated suffix limits. No missing API,
compile error, ignored assertion, fake Context or synthetic signing success is
RED evidence. Use CARGO_BUILD_JOBS=4 and one scoped local build at a time; CI owns
workspace/all-feature/WASM/performance checks. Independent plan/native/API review
and unchanged original controls precede any production authorization. This task
stops at committed plan and original REDs, with no production cutover or PR claim.
