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

The overseer assigned the shared ValidateOnly prerequisite separately to m26.
Its plan pin is00231507a4c, RUST_READ_KEY_POLICY.md. The construction declaration
drives native and JS setup once: validate, erase/drop the unused secondary, and
carry EngineOptions.read_credentials_valid (JSON readCredentialsValid). The
venue maps that Option<bool> through ReadCredentials::status into its stored
Option<Result<(), &'static str>>. None means no secondary. Shared setup rejects
injected secondary identities, metadata on an undeclared family and simultaneous
raw read credentials plus metadata. Consume the reviewed prerequisite pin; do
not add these shared surfaces to this PR. Only the bounded status is used later;
drop all later raw validate calls and any dual path. Registry construction
already refuses an invalid read key before entering the Crypto venue arm:
InvalidInput, the exact validation text, and no command outcome. ValidateOnly
must preserve that same refusal for a false flag. Only direct venue construction
can reach the venue's own CONFIG_INVALID startup notice before discovery and
identity validation before public-data-only/missing-primary failure. Pin both
boundaries separately; do not claim a registry-built bad-key engine starts.
The registry consumer target cryptocom_validation pins rejected construction
and a valid/absent-key working control; the direct-venue target keeps startup
and identity precedence. These assertions must survive metadata activation.
On archived actual a8c0403bd08, this registry target executes two working
controls and one assertion RED: false readCredentialsValid is ignored and
construction succeeds. That RED is held for the reviewed prerequisite plus
atomic Crypto declaration activation. The direct-venue target separately pins
the literal CONFIG_INVALID message across public-only/Primary combinations.
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
calculation. They and the same-ms check were first pinned on Credentials; before
deleting that signer, the controls are ported to real Engine/LocalSigner recipe
prepare/apply, including the actual DispatchContext stamp. MAX-u64 is pinned too.

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
native validation and order identity receipts and20ms inter-step eligibility.
Do not introduce a physical20ms fence on immediate native resends. Account-leverage Read
preflight remains Read before the final setting Mutation, with no write right
until Plan admission. Use shared classify::CodeTable and journal facts, exact
leg intents, terminal aggregation and accepted IDs. No local outcome demotion.
Known mandated audit corrections belong in CALLERS and REDs: all-failed
best-effort batches cannot report done; code40101 on408/5xx cannot trigger a
write resend; malformed ACK/unproven status is unknown; accepted partial IDs
survive a later failure. Keep native read retries separate from write resend.
The concrete constructor/custody table below has bounded core CLEAR at613b39e022a;
production source and its executed controls still require review.
Declare two physical attempts for a write with the
native single40101 resend; retain serial20ms step eligibility, the checked
whole-batch credit ceiling, and genuine leverage Read-preflight -> Mutation-Plan
custody. Use the reviewed constructor/credit/window table below.
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
native one-second auth handoff and post-auth subscription release. This signing
change does not gate native REST snapshots or WS reducers on auth: private_start
starts reconciliation immediately, and the existing pre-auth financial ingress
policy remains intact. The negative controls fence the auth signing action only.

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

Actual5b219d4bc6725 is plain-merged at c9084cef977. Request/credential ratchets
and both checker test targets (15/8) plus banned-pattern checks pass. The original
26 controls retain13RED/13GREEN on that engine. Two added account same-stage
mixed-error controls pass, bringing the current target to13RED/15GREEN/0ignored;
at16a376f668c the26 earlier bodies remained unchanged. Shared setup/checker hold is lifted;
the explicit core Plan-constructor review was still pending at that checkpoint.

The subsequent review closure ports only the two vector/stamp oracle factories
to real recipe dispatch while preserving their literal/output assertions, and
strengthens the capacity control to check all256 exact pending preparations
before/after identity entry. There is no Credentials import in the new target.
Both recipe controls remain GREEN; the full target remains13RED/15GREEN.

## Reviewed Plan constructors and terminal timing

codex-uufub4 reviewed the actual219d engine at613b39e022a: bounded constructor
and custody CLEAR, read-only, with no independent execution. This table replaces
the earlier pending API question; it is not production-source or CI clearance.

| Native operation | Shared authority and declaration | Settlement |
| --- | --- | --- |
| Entry and FIFO wait | begin_preflight(WriteAfterReads), selected Primary for both read_credential and write_credential; park_preflight in genuine initial custody | Retain handle with queued job; request_preflight_ready grants nothing until on_http_preflight_ready |
| Direct keyed write | admit_plan from initial or ready Preflight custody | Serial final-write schedule below |
| Leverage account-id prerequisite | preflight_read_context, Primary Read, timeout20000, CallTerms wait0/attempts2/FreshAttempt | validate_preflight_delivery before decoding account_id; admit_plan in that same successful callback atomically retires the prerequisite |
| Native leverage40101 retry | preflight_retry_context with the exact ReadFailure, once | Fresh prerequisite attempt; no account-poll transport retry policy; all other prerequisite failures fail_preflight |
| Empty keyed batch | done_empty_plan after native credential precedence | No dummy physical call, accepted fact or replayed command |
| Final writes | PlanPolicy Mutation, same trusted Primary, DefinitiveRefusal; one CallBound per serial StepBound, timeout20000/attempts2/FreshAttempt/no fallback | validate_plan_delivery, classify_write and http_write_fact; exact refused-resend grant before settlement, otherwise finish_plan_call |
| Failed physical call | Exact returned failed Pending and finish_plan_failed_call when native best-effort continuation applies | Later calls issue only in Plan-ready custody; failure cleanup is not DATA validation; strict preparation/queue refusal may seal through shared failure |

For N final writes, declare2*N physical credits, peak1 and span40000*N plus
the sum of step waits, all checked before admission. N<=128 follows the256-credit
ceiling; all other shared reservations may also refuse atomically. CALLERS and
controls must identify this whole-batch admission limit rather than silently
truncate or split a batch into renewed roots. The prerequisite consumes its own
time and credits; the final Plan root starts only at final admission. This API
has no WriteAfterReads caller-wait binding, so do not claim a receipt-capped
Mutation root or include prerequisite time in the declared final Plan span.

The first step waits0 for ordinary jobs or20ms after the leverage prerequisite;
later steps wait20ms after prior settlement. Step.wait does not delay
plan_resend_context: native eligible40101 write resend remains immediate. No
physical20ms pacing fence is added. Network,408,5xx and malformed/unproven replies
never grant a write resend. A source-stage table must distinguish these write
rules from native Read retries and pin fresh attempt windows independently.

After the last finish_plan_call, retain the genuine Plan and result through the
existing final20ms native timer. done_plan is permitted there only while the same
Plan is live, structurally complete and isolated from another ready owner;
validate_plan_seal_origin accepts delivery=None with owner.deliveries==0. The
timer is eligibility only: it issues no leg and never replays trade_command.
Then request_preflight_ready for the next native parked owner, which still waits
for the engine callback before doing work. Do not add a dummy StepBound, a new
terminal-custody hook or a renewed root to represent the tail wait.

PlanBound has no tail-wait field. The immutable root remains40000*N plus step
waits; if the last response leaves less than20ms, strict root expiry wins before
the final timer. Exact-owner cleanup makes that old timer inert. This new
whole-plan-bound consequence must be disclosed in CALLERS and pinned at equality
in both event orders, alongside a live-root timer that completes once and wakes
the next FIFO owner once. An unconditional extra20ms beyond that root would need
a separately approved shared bound extension; this cutover does not request it.

done_plan delegates all-failed, partial and unknown aggregation and accepted IDs
to the journal. Native ids/first_error/effective state cannot override it. The
actual PlanPolicy has no independent_calls field: no copied flag or readiness
claim for forthcoming independent-call behavior is part of this plan.

## Production sequence and review boundaries

Ship one complete cutover; these are local implementation checkpoints, not
separately shipped staging paths. Plain-merge the reviewed #2061 head or its
actual landing first, preserving its exclusive managed public-history authority.

1. Trusted setup and custody: stack on m26's separate ValidateOnly prerequisite,
   then update only the Crypto registry/provider construction and consumer
   activation together with Options. Keep only opaque Primary identity,
   public settings and the once-computed secondary validation result. The
   prerequisite owns shared construction declarations and native/JS transport;
   this consumer uses its metadata alone at start and identity. Private
   POST selection stays Primary with Read scope=None, no route/tag evidence.
   Hosted setup sends only the required Primary signing material. Coordinate
   shared-file regions before editing: registry/lib.rs is also touched by #2033,
   #2029, #1990 and #1891; construction-rules/localSignerSetup by #2033/#1868,
   with #2029 also touching construction rules. No foreign hunk resolution is
   implied by this plan.
2. Canonical recipe: move Envelope to sign/dispatch.rs and harden prepare/apply
   attestation. Before deleting Credentials, port dispatch_recipe.rs comparisons
   and sign.rs literal unit vectors to independent literal-only recipe controls,
   including the replay-key WS vector57a5207e795bd876080625ee876fa750cd0134c3916bcdc346d4efe1b6f0218f.
   Keep same-ms/no-counter and u64 decimal controls. Delete eager/raw helpers in
   the same atomic cutover; retain no fallback or alternate dispatch flag.
3. Read consumers: account.rs keeps independent Background owners and unchanged
   native stage/counter/reducer behavior; identity.rs uses a selected caller root
   with20s children; history.rs retains one selected caller root across eligible
   dequeue/retry and exact history-bridge admission. Preparation/provider and
   root expiry remain strict for command Reads; native live-root released
   transport failures keep their documented recovery. WS auth signs only at the
   existing one-second handoff, with late-close/reconnect result fences, without
   changing private REST startup or reducer authentication policy.
4. Trading: implement the reviewed Preflight/Plan table in trading.rs and exact
   Pending callback arms. Add one const CodeTable and record accepted IDs/effects
   through journal facts. Preserve validation, native FIFO, setting effects and
   reconciliation identities; expose no numeric terminal fallback. Declare all
   physical credits before any final-write progress or I/O.
5. Cutover audit: remove account.rs::signed and all remaining raw Credentials
   callers only after every private consumer and WS auth has moved. Regenerate
   the exact eight-entry request prune, signing shrink and credential inventory
   shrink; leave foreign rows/checker behavior unchanged. Crypto remains outside
   the migrated-family signing set under the explicit interim choice above.
   Review the whole dispatch/provider path, not only scanner output.

Preserve all28 original assertion bodies except the explicit opaque-fixture
construction change. Source-stage additions cover the constructor table and
N128/N129 atomic admission, parked provider and delayed route/final Claim, fresh
resend windows, leverage retry count, wrong receipt/provider fields/body binding,
failure-only continuation, final20ms root-expiry priority, exact-owner cleanup
and unrelated live peers. The existing financial tests remain the reducer oracle.

Run scoped Crypto tests and strict all-target Clippy sequentially with jobs4,
format checks, both inventory ratchets and banned patterns. Full family size,
CPU, replay and shared coverage stay in CI. Obtain bounded source/caller/map,
HMAC, selection and core Plan-source reviews; do not inherit executed evidence
from the constructor review. Push only an intended freeze candidate after the
requested reviews return, with one PR Rust run per head. READY follows green CI
and clear source reviews; landing still requires the overseer's window.

## Actual ValidateOnly composition

Actual #2078 `cec22de6e875dc0e7b9e0ec628bba5ceeb7ae2ff` is plain-merged at
`ff18f131bbe`; the current rewrite tip was already contained. Only CALLERS
conflicted and keeps incoming plus the exact own suffix once. Crypto's atomic
consumer row declares dispatchSigning and ValidateOnly. Its registry feature
enables provider setup, its constructor receives opaque Primary plus
read_credentials_valid.map(ReadCredentials::status), and the engine registers
the same identities as the provider. No shared setup algorithm changes here.

The five registry controls pin trusted consumption/erasure with primary-only
provider installation, raw-invalid read precedence, absent/valid input, and
injected-secondary refusal. A native caller cannot preset a trusted validation
result; that refuses as unexpected input. A host-supplied false result instead
keeps the original secondary-validation error before venue construction.
Direct-venue startup and identity precedence remain in the private-funnel
controls. Four JS wrapper controls use the real construction declaration and
generic setup: no raw options or secondary capsule, true/false/absent bounded
status, and unchanged caller-owned input. Their mocked constructor proves
transport, while registry controls prove native refusal semantics.

At this composition all252 native Crypto tests, five registry controls and four
wrapper tests pass; strict scoped checks and bans/ratchets follow before push.
The new wrapper target is excluded by the existing ESLint test ignore, so its
successful Vitest execution supplies its local check; no lint coverage is claimed.
The exact inventory delta versus cec22 is request8->0, credential-selection5->0,
and signing14->2 (only EraseSecrets). No foreign inventory or checker changes.
Per the overseer's current instruction, push a draft once and run its one
PR-triggered Rust workflow in parallel with bounded source reviews. Keep the
two Aster runs unchanged; full custody/size/CPU/READY are not claimed here.
