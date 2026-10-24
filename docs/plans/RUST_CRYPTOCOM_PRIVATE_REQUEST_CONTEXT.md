# Crypto.com complete private dispatch and RequestContext cutover

Plan first, codex-ih35js, base actual f40d64831ee. The overseer approved the
remaining eight occurrences after #2061, with full custody and no staging dual
path. #2061 remains independently frozen/running CI; venue merges are frozen
until combo-5b fully lands. This checkpoint adds a plan and original controls
only. Shared setup/checker source changes wait for actual 5b and owner review.

## Exact remaining map

Current tip has 11 fingerprints/12 occurrences in Crypto.com. The public tail
in #2061 removes 3F/4O, leaving the following eight singletons. This full funnel
must remove all eight and leave zero, without checker exemptions or invented
legacy rows. The actual scanner, not a method-count estimate, verifies the end.

| File | Fingerprint | Native witness | Managed owner |
| --- | --- | --- | --- |
| account.rs | 83c6b712d19379913a85bf575a1a1834358d1b5f84d4c7a04a2f241487399a9c | shared signed POST cx.request | Read Background/command or Mutation Plan Context, request_signed_managed |
| account.rs | a77d14858a6b097716fd945cddafa2c827e671194bbe1122e95c60b698c56006 | Orders-kind macro over legacy request | typed account-cycle request and exact delivery admission |
| identity.rs | c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548 | numeric done | original caller Read root/ReadCompletion |
| identity.rs | c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | numeric fail | same exact root, paired command diagnostic before failure |
| history.rs | dfb17bdf89f16befd8bb7566ac05d0bddf2980c6627b642f20e1f1b5a4859508 | private page-exhaustion boolean witness | SelectedRead root/ReadCursorHistory admission |
| trading.rs | c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | numeric fail | engine write journal/Plan terminal |
| trading.rs | f32eb32f491028d71654d8ec05bbdf592328cd7f7156d848a17d430553a77638 | numeric done | same journal/Plan, retain accepted IDs and failed legs |
| trading.rs | 8c1d78bb768fb392694ad91c49f963ab64ba87a71d151646bff0298b85a6b734 | leverage-account preflight macro | explicit owned preflight Read and following Mutation |

Macros disappear with the actual typed owner/admission conversion. Do not rename
unmanaged bindings solely to shrink counts or retain a dead legacy authority.
Private Account/Identity/History/Trade all share account.rs::signed today; WS
public/auth also uses raw Credentials. The cutover must cover both together.

## Ownership and cutover boundary

No competing Crypto source PR exists beyond own #2061 (history/lib). Future
production stacks on its reviewed head/actual landing, preserving its public
callbacks. HMAC review: claude-4yjlw0. Selection: claude-m26ps4. Core Plan/issuer
fit: codex-uufub4. Shared registry/hosts/protocol/checker source is held until 5b;
coordinate actual file owners before editing. No merge before an explicit GO.

Keep native socket transport/supervisor/subscription, account snapshot reducers,
revisions/skip coverage, public discovery/candles/books/trades and local trading
refusals intact. WS changes are limited to dispatch signing at the native auth
handoff. This is not managed-socket adoption. Existing account, history, identity,
trading, socket and signer financial assertions must survive with fixture glue.

## Signing parity and full custody

Reuse the actual Envelope recipe (#1840), hardened through owner review, with a
single production recipe under src/sign/. No new sign_http/send_signed HTTP
bootstrap, raw-secret fallback, partial dispatchSigning flag or legacy dual
funnel. The final engine Options/venue contain only opaque identities and public
configuration. Trusted provider setup alone receives signing material. Hosted
mode sends only material required for this venue's actual operations, never an
unused secondary secret. Erase raw inputs through existing cleanup paths and
pin the host/TS envelope regression on the same PR after source ownership opens.

Native Params is BTreeMap<Text, Number(u64), Strings>. It has no nested object,
bool, null, float or depth-cap input; do not expand the wire grammar. Sort keys,
concatenate string-list elements without separators, and render u64 exactly in
decimal (including MAX), never float/exponent. Preimage is method+id+api_key+
paramsText+nonce, with id=nonce=floor(protocol now) as u64 and lowercase HMAC hex.
Two reissues inside one millisecond share the native stamp; add no counter.
Fresh reissues in later milliseconds reprepare from unsigned data and fresh
protocol time. WS public/auth uses the same nonce rules and omits params.

Pin independent Python hmac literals for a REST Strings/u64 case and WS auth.
Keep recipe parity and security evidence separate from legacy oracle deletion.
At both prepare and apply, attest the exact method/host/path/headers/body params
and receipt. HTTP headers are exactly Content-Type: application/json, with empty
signer_headers; prepare requires body None (WS empty text). Apply requires its
attested prepare, exactly two values, a32-byte MAC, matching receipt and no
provider headers/CRLF. Replace body/text wholesale with id/nonce taken from the
receipt stamp; do not trust mutable self.nonce as separate authority. No partial
mutation on refusal. Use shared validation primitives rather than another
result-policy implementation. Never retain provider key/MAC results in
errors, Debug or unrelated continuations. Enumerate signing-map/check-signing
family-flip exposure before changing it; no staging exemption is authorized.

## Selection and secondary validation

Native signed REST is POST/Primary even when a valid secondary is configured;
posted_read_keeps_primary_identity is a protected positive. There is no read-key
fallback. The native route is None and it emits no route_request/tag_account_read.
Use engine Slots<Identity>(primary,None,default); role Read has scope None,
Mutation for writes and SessionAuth for WS. Pin actual route/evidence, not just
HMAC bytes. Do not invent a primary reason or fake credential to force routing.

M26 confirmed existing composition suffices: trusted setup computes
Option<Result<(), &'static str>> once via ReadCredentials::validate, then erases
raw read material. None means no secondary. Only that bounded result is used
later; drop all later raw validate calls and any dual path. No Crypto-specific
identity/validation type or algorithm. Invalid secondary blocks BOTH start()
(CONFIG_INVALID before discovery) and fetch_identity (validation failure before
public-data-only/missing-primary failure). Keep exact messages and precedence.
At the reviewed5b-final inventory, Crypto lib.rs has ReadCredentials1 and
read_credentials3; identity.rs has read_credentials1. Remove all five mentions
through trusted setup validation metadata, with no relocation into another
venue file. Shared credential inventory is regenerated after actual #1831.

## Signing inventory and vector literals

Move the hardened recipe to src/sign/dispatch.rs and delete eager Credentials
and its raw-material oracle. Keep Crypto outside the migrated-family scanner
set, matching the owner's explicit WOO option: this checker scans sign/ even
after a migrated flip, so a move plus flip alone adds primitive witness rows.
No new baseline row or exclusion is requested. Existing Crypto baseline sites
shrink: lib.rs11 -> at most2 required EraseSecrets trait/method witnesses,
secondary_tests.rs3 ->0 through trusted fixture setup. Remove orphan Sensitive
storage and raw-field handling; independently review the entire dispatch module
for complete custody. The final scanner decides exact removal-only counts.
This checker state is interim, not a permanent exemption: once the remaining
two lib.rs EraseSecrets witnesses leave, a named follow-up flips Crypto into the
migrated set and lists any newly exposed canonical primitive rows explicitly
for owner review. Full dispatch/provider review and independent vectors remain
mandatory before this cutover can be READY.

Independent Python hmac literals use key fixture-primary-key, secret
fixture-primary-material and nonce1710000000000. REST private/get-trades has
exec_inst=[REDUCE_ONLY,POST_ONLY], instrument_name=BTCUSD-PERP and limit=u64::MAX:
ad5455a0093aba156f0ad8f96ec18c599add648c4de5dbe3b2059ea1fbbc43aa.
WS public/auth with no params:
0e9d20117ab7df8097634d022b6241c57cc2eba952e8685cbb2240e43efa48fb.
Both are committed as literals in the original target, separate from the native
recipe's own calculation. Native same-ms and MAX-u64 rendering are also pinned.

## Request owners and policy

Account polling: actual Background Read Contexts and selected dispatch helper,
20s per physical operation. Each stage shares one retry counter: signature-text
40101 allows one immediate retry; other transport errors allow three immediate
retries. A transport retry consumes the signature retry allowance and vice
versa. Do not add a retry timer. Reset rows/skips for the retried stage and
reset its counter only when advancing to the next read. Preserve the native
raw/decoded text compatibility exactly, with dedicated mixed-error controls.
Validate exact delivery before stream codecs,
coverage/skips, snapshot tokens, stage/publication and next-read transitions.
Typed failure cleanup consumes the exact Pending, keeps native retry/reset,
and never implies DATA authority. Preserve positions -> balance -> regular
orders -> advanced orders on initial/every fourth successful cycle; intervening
10s polls fetch only positions -> balance. Pin regular-order rows/token retention
across an advanced-order retry, partial publication and revision fencing.
Provider refusal must schedule native recovery once rather than leave busy set.

Identity: genuine caller root before POST, immutable caller wait; fresh20s
children under it. Native identity retries immediately: code40101 once, ordinary
transport up to three times, using one shared counter. Transport then40101 adds
no signature retry; signature then transport has only the remaining counter
budget. Reprepare each fresh20s child under the original root, without a new
retry delay or renewed root. Preserve UUID
text/integer/zero rules, primary signature and no financial Store publication.
Local refusals and success/error diagnostic ordering use the same owned root.
Duplicate IDs, capacity, missing receipt, preparation/provider refusal, equality
and stale callbacks have one typed terminal and no numeric fallback.

Private history: one selected caller root and ReadCursorHistory, fresh20s legs
at eligible dequeue. Preserve nanosecond cursors, duplicate identity rollback,
accepted-only recovery, requested limits/income range behavior, one-second page
waits and per-method eligibility. Keep native transport retries/reset and40101
compatibility counts; if minimum legacy text compatibility is necessary, reuse
the approved native error projection and disclose it, never auth inference.
Root expiry remains strict with no finalizer/prefix/INCOMPLETE; exact Owner
retirement drops only its held timers. Local child/preparation refusal is strict.
No Plan solely for paging, no local root timer or renewed receipt.

Trading: opt into actual http-plans for preflight/whole-plan admission and
physical windows; use genuine entry custody before progress/queue/I/O. Preserve
native validation and order identity receipts, 20ms inter-step eligibility and
required physical pacing through shared Context policy. Account-leverage Read
preflight remains Read before the final setting Mutation, with no write right
until Plan admission. Use shared classify::CodeTable and journal facts, exact
leg intents, terminal aggregation and accepted IDs. No local outcome demotion.
Known mandated audit corrections belong in CALLERS and REDs: all-failed
best-effort batches cannot report done; code40101 on408/5xx cannot trigger a
write resend; malformed ACK/unproven status is unknown; accepted partial IDs
survive a later failure. Keep native read retries separate from write resend.
Exact Plan source/physical-attempt/window geometry must be reviewed by uufub4
before implementation; do not guess around unsupported constructors.
This is a pre-source gate: declare two physical attempts for a write with the
native single40101 resend; retain serial20ms step eligibility, the checked
whole-batch credit ceiling, and genuine leverage Read-preflight -> Mutation-Plan
custody. The exact constructor/credit/window table remains pending core review.
Never admit a normal Read root and attempt fresh command-entry Mutation later.
Empty keyed PlaceOrders/CancelOrders use done_empty_plan on genuine Preflight
authority, with no synthetic physical call or accepted fact. Keyless empty
batches still refuse credentials first; pin both plus duplicate/capacity gates.
Native40101 write resends are immediate and bypass the inter-step20ms timer.
Any added physical pacing fence must be explicitly reviewed and recorded in
CALLERS, with delayed-provider/final-Claim/resend controls, not claimed as parity.

New preparation/provider refusals have no eager-native equivalent: identity and
history fail strictly through their exact root; writes record NotSent through
the journal; account cleanup clears its busy state and schedules native poll
recovery once; WS follows its existing failure/reconnect lifecycle. Pin each
separately. A late WS signing result after close/reconnect is inert; retain the
native one-second auth handoff and no financial or subscription work before auth.

## Original controls and gates

Commit this plan, then compiling actual-Engine controls before any production.
Pin parked/missing/refused provider for identity/history/startup/WS/write; genuine
root/capacity/duplicate/deadline/queue behavior; best-effort all-failed journal
terminal and write500+40101 resend denial. Record initial assertion REDs apart
from later adoption assertions. Missing API compile failures are not REDs.
Working controls pin exact native primary POST with secondary/no route/evidence,
secondary validation at startup and identity, UUID/no-Store behavior, native
signing vectors/u64/same-ms stamp, native retries/queues/financial snapshot
ordering. Existing suites cover broader recovery; add actual delayed provider,
route, receipt, final claim, late expiry and provider-field negative controls
before claiming dispatch custody.

Original test factory may later switch to trusted opaque setup as one explicit
fixture change; preserve every original assertion. No new public production
flag, test-only authority, ignored RED, checker growth or generated receipt is
committed. Scoped Crypto tests/Clippy only, CARGO_BUILD_JOBS=4 and one local build
at a time. Full size/CPU/replay are CI. Independent source/caller/map plus HMAC,
selection and core API reviews precede READY; draft CI runs alongside review.
Architecture gate: No when this consumes existing engine composition; any shared
API gap is handed to its owner before venue code, never solved by a local copy.

Original checkpoint on unchanged f40d64831ee runtime: dedicated target executes
26 tests, 13 assertion RED /13 GREEN /0 ignored. REDs expose provider bypass in
identity/startup/WS, missing provider fallback, root deadline/duplicate/capacity,
all-failed and partial write terminals, 500+40101 write resend, and unprepared
recipe application. Positives pin exact vectors/stamps, selection/validation,
secret erasure, empty-batch precedence, mixed retry counters, fourth-cycle orders
and regular-order snapshot retention across advanced-order retry. The first WS
fixture attempted to install the provider after startup; that setup error was
corrected before counting its assertion RED. No runtime/map/Cargo edits.
