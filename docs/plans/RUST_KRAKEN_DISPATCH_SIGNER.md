# Kraken Futures and Spot dispatch-signer custody

Plan-first base: actual9a757d406b8, branch
feat/rust-kraken-dispatch-signer-plan. Claude-ddi2il approved ONE combined
Futures+Spot plan/original slice; production waits actual combo5b and reviewed
nonce/session fit. HMAC reviewer: claude-4yjlw0. Credential/selection reviewer:
claude-m26ps4. Preserve1876 write-plan/tests and1798 Spot book/CRC32. No source,
checker, inventory, key-selection or5b composition in this checkpoint.

## Scope, counts and source-order holds

Three actual REST send funnels are relevant: Futures account.rs signed_request
cx.request and signed_streamed cx.request_streamed_body (two occurrences), plus
Spot account.rs send_private cx.request (one). Full custody cutover must include
both product constructions, Futures private WS challenge/subscription signing,
and Spot's signed token acquisition. One family signing-ratchet flip cannot
leave a second raw product behind. Do not hide raw callers with a renamed funnel.

The request checker freezes family27 occurrences: account2, history6, lib4,
secondary_tests4, spot/account4, spot/trading2, trading5. Pending2058 owns four
lib candle sites, and PublicTrades owns one history send. There are three actual
private send sites, NOT every lexical macro in these files. Eventual removal of
all three raw funnels would mean27->24 in this baseline, or22->19 after the two
public slices, but no prune is claimed or guaranteed by plan/recipe tests. A
staged issuer row or any other bootstrap needs the exact source-derived before/
after map and explicit claude-ddi2il grant BEFORE editing that row. No inventory
growth or local credential fallback is implied by this scope approval.

Separate signing inventory is29 occurrences: Futures lib13, secondary_tests10,
Spot account1, Spot mod5. Migrating the family also exposes sign.rs (currently
excluded): nine additional lines/eight fingerprints. All material-owning
production sites must move to provider/opaque Identity; test oracles stay outside
production and keep every native assertion. No signing/credential ratchet edit
until the actual source and exact native/ref inventory are reviewed.

Open source collisions checked from the fleet map:1876 owns only
RUST_KRAKEN_REQUEST_CONTEXT.md, spot_write_adoption_red.rs and trading.rs tests;
1798 owns Spot book.rs/spot_book_crc32.rs. Keep those untouched.2058 owns Futures
lib candle hooks and the secondary probe carry; PublicTrades owns unsigned
history. Later signer work genuinely integrates those actual landings, preserving
both public paths. Old ownership text naming19pt8s is stale:19 confirmed no
Kraken source/nonce claim and routes HMAC to the assigned reviewer.5c57jl legacy
claim check remains to be reconciled before production; the overseer owns scope.

## Native Futures bytes and slot policy

Decode signing secrets as native nonempty base64 only in trusted setup. No
ambient/venue secret holder, material getter, Debug leak or signing fallback.
Ordinary GET selects Secondary when read_signer exists, except /api/history/
reads select Primary. Mutations use Primary. Streaming /api/history/ also pins
Primary egress. Preserve the actual path exception independently from method;
selection is declared once with trusted Identity and egress, not reselected by a
recipe or response. Missing/read-invalid/public-only precedence remains native.

Futures form() sorts keys and uses its existing RFC1738-style codec: spaces '+',
literal parentheses, UTF-8 percent bytes. For a GET append exactly that form to
wire path, with no body; non-GET body is the exact form. Strip /derivatives only
from the canonical signing path; accept native /api/v3/ and /api/history/ paths,
reject query-bearing/unsupported signing paths. Preimage is
encodedForm + nonceDigits + canonicalPath. SHA256 that exact byte sequence, then
HMAC-SHA512 under the selected decoded secret and base64 the MAC. No API key is
part of this MAC preimage.

One atomic batch presents the selected API key, computes the public digest and
signs the raw digest (chained Result/Raw), with receipt/header attestation. Preserve
header order APIKey, Authent, Nonce, Content-Type(form-urlencoded). Reject foreign
receipt/headers/value-count/shape, bad MAC length, non-UTF8 or CR/LF presentation.
No signed header/body can be replayed through a new release/owner/generation.

## Native Spot bytes and slots

Spot is PRIMARY-only, including Balance/OpenOrders/GetWebSocketsToken and writes.
The native Spot options may contain a read key but its private path never selects
it. Preserve form input order and RFC3986 escaping; append nonce LAST before
encoding. Body is the exact form, including that same nonce. Digest is
SHA256(nonceDigits + exactFormBody); MAC is HMAC-SHA512(secret, pathBytes + rawDigest).
Only /0/private/ paths without '?' are accepted. Preserve API-Key, API-Sign,
Content-Type(form-urlencoded), Accept(application/json) header order and20s
request timeout. Do not substitute the sorted Futures form codec.

The same opaque-key/atomic result/attestation/error controls apply, with no raw
Spot fallback left beneath the family gate. Hosted mode must use the shared
minimum-material setup for both products; the server receives only the declared
signing material required by that path. Exact capsule/Present support and lifetime
need shared owner review; no per-venue server storage or hosted bypass.

## Native nonce contract and OPEN shared prerequisites

Native nonce is max(floor(now_ms)*1000, previous+1), checked u64 arithmetic, decimal
wire string. The process/WASM-memory shared allocator survives signer recreation,
fixed/backward clocks and more than1000 sends per millisecond. Separate processes
or WASM memories require caller deployment coordination; this slice adds no
network nonce coordinator. Public calls do not allocate a nonce.

Current legacy Signer stores SHA256(decoded secret) as its shared domain key.
Within one Futures instance, if primary/read API-key strings are equal but
secrets differ, nonce selection deliberately uses the PRIMARY allocator while
the read MAC still uses its READ secret. That special case must become trusted
opaque setup aliasing, not venue material comparison or a secret/key hash. Spot
uses Primary. Shared fit must explicitly decide cross-product and cross-engine
same-key identities; do not infer nonce subject from slot/ref spelling or route.

19 verified no dispatch-time nonce API exists on actual1a341: Cx::reserve_nonce
can run only at queue time, whereas DispatchContext has only stamp/clock/entropy.
Queue-time reservation cannot claim send-time monotonicity after reversed park/
resume or re-preparation. Required shared design: a declaration-bound dispatch
reservation from the trusted selected subject/NonceKey, using the native floor,
without venue counters, clock allocators or raw material-derived identity. Owner is codex-uufub4 by the overseer assignment; exact API/immutable review
pin is OPEN. This is a shared prerequisite, not venue code.

Trusted setup must supply the reviewed per-API-key stable namespace and alias
same primary/read keys even with distinct signing secrets; no guessed stable_id,
secret digest, key hash, runtime fallback or forged issuer/subject proof. G2c
provenance/retirement, C3 setup and provider boundary owners review the exact
shape. A key change/engine replacement must not reset a still-valid domain.

Native nonce accepts finite nonnegative clocks up to checked u64/1000. The
overseer explicitly approved a tighter receipt MAX_SAFE millisecond ceiling as
InvalidTime before private I/O; record this behavior change in CALLERS. NaN/infinite/
negative/overflow must refuse before I/O, with no partial raw fallback.

## WS/session contract and held authority work

Futures private open sends a PRIMARY key-only challenge request, then signs
SHA256(challenge) with HMAC-SHA512 for each private feed subscription. Keep the
same native challenge field/feed order and two deadline phases: OPEN sets
now+15000, then accepting a nonempty current challenge resets now+15000 for
all subscription ACKs. CONNECT does not start this auth deadline. Preserve
existing supervisor-tick enforcement and the separate180s initial-payload guard;
no snapshot opt-in or new local auth timer is inferred. Current cached signed_challenge material must not
become a credential-bearing replay carrier. Exact managed-socket recipe fit is
OPEN: each re-sign binds the active socket generation and current
challenge; a late result for retired challenge/socket cannot subscribe or publish.
Do not mint fake command/Read/Session authority to make a WS recipe compile.

Spot authenticates via signed GetWebSocketsToken, then uses that token in
execution/balance subscription frames; it has no extra WS HMAC login. Preserve
its token/socket retirement and all native restart/supervision semantics. No
account refresh, group, queue, snapshot or write-owner conversion is inferred
from signer custody. Managed signed reads use existing SelectedRead; aggregates,
backgrounds, writes and sessions require their genuine reviewed issuers and
original native lifetimes, not a guessed20s logical owner.

Native retries retain budgets and counters; every actual resend is re-prepared
with a fresh provider batch and dispatch nonce. Provider refusal is pre-send and
strict, with typed fields preserved. Write outcome remains owner/evidence based:
no blanketNotSent over earlier effects/IDs and no automatic resend after an
unknown write.1817 selection and all5b declaration changes are consumed only by
actual integration after5b, never copied into this plan/test branch.

## Plan/original sequence

Plan commits first. Add a new isolated dispatch_signer_adoption.rs on unchanged
9a757 without touching1876/1798/2058/PublicTrades files, source or maps. Thin
recording local/deferred/refusing provider plus real engines: assert signed native
GET/write/token/challenge work routes through the injected provider before HTTP
or credential-bearing WS release. Such assertions fail at provider-batch0 on the
original native signer, not at missing APIs or fixture setup. Keep exact native
wire/HMAC vectors as independent working controls, not fake provider results.

Pin Futures Primary/Secondary/primary-history and Spot PRIMARY-only selection;
base64 decoding/refusal, sorted versus ordered forms/Unicode, header order,
method/body/path attestation, independent HMAC vectors, keyless public unchanged,
provider refusal/deferred/late generation, and native nonce monotonicity. Include
same-key/different-secret per-engine and cross-engine cases, signer recreation,
fixed/backward clock, >1000 sends/ms, and reversed dispatch/reprepare controls.
If the first genuine Prepare/provider assertion fails, disclose deeper suffixes
as gated rather than executed. Shared API-only new-contract specs remain distinct
from compiling native REDs. Do not invent a nonce seam inside the fixture.

No production until actual5b, scope/ownership reconciliation, reviewed nonce/
session/selection fit, original CLEAR and exact bootstrap grants if needed.
Then direct custody cutover, independent HMAC+credential/source reviews, scoped
native/strict checks, draft PR/full CI and actual landing window. Freeze does not
stop plan/tests. Local cargo is scoped changed crate/target only, jobs4 and one
build at a time; no workspace/all-feature sweep or local WASM/timing sweep.
Production MUST enable safe-cex-crypto/hmac-sha512 in the Kraken dependency and
correct the crypto feature comment. This is WASM/hosted provider feature wiring,
not a size-budget change. Consume #2023 or tolerate either landing order: its
removal of test-only algorithm support must not leave Kraken Unsupported.


## Reviewed declarations and explicit behavior contracts

m26ps4 reviewed setup/roles; 4yjlw0 reviewed HMAC/native bytes. Setup declares
Base64 per slot and preserves both CONFIG_INVALID texts in order: "Kraken secret
must be nonempty valid base64", then "Kraken secondary secret must be nonempty
valid base64" with return on the latter. Futures declares SlotPolicy{secondary_requires_primary:true}; every reachable
native secondary read requires a primary configuration. Native low-level
signed_parts could select a read signer without Primary, but all account and
command callers gate that path. Preserve the lifecycle's silent skip through
Slots::has_primary()/Missing, not a second material-bearing signer field. The
read-only no-private-account/provider control pins this native behavior. Spot
uses Slots(primary,None) and ignores read credentials.
Trusted setup supplies identical caller-assigned stable_id for identities sharing
one API-key nonce subject; each retains its own signing ref and secret. The engine
mints NonceSubject/NonceClaim; no venue key comparison, secret digest or fallback.
The overseer approved refusing a missing stable_id before private I/O using the
existing SignError::InvalidRequest -> InvalidInput/NotSent taxonomy, message
"Invalid signing request" (m26ps4). Mint the nonce subject during selection,
before request IDs, nonce reservations or provider batches; malformed stable IDs
(blank, >256 bytes or control characters) get the same configuration refusal,
with no auth/health counter changes, and approved Secondary egress for selected read-key GETs plus explicit
Primary egress for Primary calls. Native used host default except streamed
history; CALLERS must name this change as well as the MAX_SAFE clock ceiling.

| Product and native call | Method | Declared role / native slot |
| --- | --- | --- |
| Futures openorders/openpositions/leveragepreferences/accounts refresh | GET | Read / Secondary if configured, else Primary |
| Futures fills, /derivatives/api/v3/fills | GET | Read / Secondary if configured, else Primary |
| Futures /api/history/v3/account-log and /api/history/v3/orders | GET | PrimaryRead(History) / Primary |
| Futures sendorder/cancelorder/batchorder/editorder | POST | Mutation / Primary |
| Futures leveragepreferences setting | PUT | Mutation / Primary |
| Futures leveragepreferences command lookup | GET | Read / Secondary if configured, else Primary |
| Futures WS key challenge and signed subscriptions | WS | SessionAuth / Primary |
| Spot Balance/OpenOrders | POST | Read with Slots(primary,None) / Primary |
| Spot GetWebSocketsToken | POST | SessionAuth / Primary |
| Spot AddOrder/AmendOrder/CancelOrder | POST | Mutation / Primary |

A method never selects authority at runtime: the table fixes each native caller.
Role assertions must exercise these paths; private signed probes cannot claim a
method-switch selector as the new API. Native public Futures trades and all Spot
public calls remain unsigned. WS session generation binding remains #1896/#1897
owner fit, not a property granted by a nonce claim.

Independent HMAC literals (4yjlw0, Python), secret base64(kraken-test-secret),
nonce1700000000000000, are pinned in originals: Futures sendorder sorted form,
Spot Balance nonce-only body, and the native challenge. The new target uses only
existing engines/provider APIs and leaves production/Cargo/checkers/maps intact.


## Original checkpoint before actual 5b integration

On unchanged actual9a757, the new isolated target compiles and executes 20 tests:
9 intended assertion REDs, 11 GREEN, zero ignored. Six custody assertions stop at
zero provider batches for Futures account/history/write/key challenge/challenge
MAC and Spot balance/orders/token. Two refusal assertions expose actual raw
signed HTTP despite a refusing installed provider. Missing stable_id exposes the
legacy secret-digest fallback. Native GREEN controls independently pin all three
Python vectors, real Futures secondary GET/MAC/header order, Spot primary-only
POST/body/timeout/header order, public-only behavior, no private startup for
read-only configuration, sorted UTF-8 form, invalid secrets/paths/clocks, signer
recreation/backward clocks and 1002 sends within one millisecond.

The initial compile correction (header names are borrowed str) and the mistaken
read-only startup positive are fixture findings, excluded from the RED receipt.
Native account_start/account_refresh require the primary signer even though the
low-level selector accepts Secondary without Primary; that gate is now GREEN.
No existing source, fixture, Cargo, checker or inventory changed. Provider
capability is delegated to the real LocalSigner, never fabricated; HMAC-SHA512
feature wiring stays a source prerequisite. Because native bypasses the provider,
no successful managed-provider SHA512 batch is claimed by this checkpoint.

The history/write/WS post-provider assertions are gated by their first RED, not
counted executed. Exact deferred-provider release/reprepare and cross-engine
nonce claims await uufub4's reviewed real-engine API controls; this target does
not invent a nonce constructor or reservation fixture. Missing/malformed subject
shared taxonomy and the overseer-approved eight-repair bound are documented requirements, not an
executed native consumer migration. Production remains held for exact nonce and
session fit, original review, and actual dependency integration.


### Bounded review follow-up

m26ps4 reviewed d1add97ba85 CLEAR on setup/roles/native originals, with the
secondary_requires_primary:true correction above. HMAC review remains separate.
The 20-control original run is 9 assertion RED / 11 GREEN / zero ignored; strict
Clippy on only the new target passes. No production or dependency is pulled into
this plan branch. Actual5b219 is landed now, but nonce and session API review
remain production gates; the source-order paragraph is historical, not a claim
that the dispatch nonce seam has landed.


### Actual 5b checkpoint

Plain-merged actual219d4bc6725 at5a89019d6d3 with no conflict. Only this plan and
the new dispatch_signer_adoption target differ from that landing; all production,
existing fixtures, Cargo and inventories remain byte-exact incoming. Re-executed
the unchanged 20 originals: 9 intended assertion REDs / 11 GREEN / zero ignored.
The prior fixture compile/mistaken read-only positive remain excluded. No WIP
push or duplicate CI run is needed for this local plan/original checkpoint.

uufub4's shared nonce plan/original028e4db578f, reviewed descendant27fdf8000cf
with explicit legacy-control ports, has bounded consumer/API/original
fit CLEAR: separate reservation and global released-floor fences at provider
apply/final Claim, exact decimal u64 receipt, private raw-ms ceiling and eight
pre-send repairs. Its legacy allocator probe is explicitly original fence
exposure, not trusted mint/raw-clock/HMAC/wipe/group/repair readiness proof.
Exact source hooks, #2024 composition, old secret-digest to stable-subject
continuity and session-generation API fit remain review gates before production.


## Session-owner fit and original retirement control

Session owner55ao7t pins landed1832/8e4ad630253,1842/fade3aa0a41 and
1897/58af3c0a023, all on actual5b219d. Spot GetWebSocketsToken fits existing
open_session(SessionPolicy{credential:Some(primary),repeat_safe:true,
private_egress:None}); issue a signed Read with session_context and
request_signed_managed, passing no Bearer. The token remains venue-owned and is
placed into native executions/balances WS frames; Bearer is REST-only. Retire
that exact session in on_close or socket epoch change before reopening. No core
API links Session retirement to socket generation automatically. Add a genuine
socket-close/late-token original: the retired response must not reach the venue
callback or publish subscriptions; a live-socket twin proves the fixture.

Futures challenge signing is explicitly OUTSIDE Session: there is no HTTP
session operation. Present(key) and SHA256(challenge)+HmacSha512 must bind the
managed socket epoch/challenge and its native15s deadline. Retired socket or
challenge provider results cannot send subscriptions. Obtain the exact managed-
socket owner API fit and signer recipe review; do not fabricate a Session or
issuer to compile it. A nonce claim supplies no socket/session authority.

Only genuine Context-phase command owners may session_wait. WS subscriptions
stay venue-driven. These are plan/original controls, not a token/session native
migration receipt; no runtime is changed by this checkpoint.


### Token-retirement original extension

Three additive controls retain every earlier original assertion. A thin venue
wrapper only counts real on_response deliveries and forwards startup, streaming,
open/close, command, timer and typed failure callbacks. The genuine Spot token
POST is released through the existing fixture, held, then replied on the same
or a closed socket. No Session/Context or successful provider batch is invented.
The live twin receives one callback and exactly native executions then balances
subscriptions with the token. The closed native twin emits no WS subscription.
The NEW-CONTRACT retirement RED counts a late callback (5 vs4), proving current
native socket suppression does not yet provide engine Session retirement. It
claims no pre-existing wrong-socket token disclosure. Session retirement after
adoption must deny that callback as the owner requires.

Scoped spot-filter execution: 7 tests, 3 assertion RED / 4 GREEN, zero ignored
(16 unrelated originals filtered). The three new controls are 1 NEW-CONTRACT
RED / 2 native GREEN; the other two REDs remain the original Spot provider
bypass/refusal assertions. No production, Cargo, maps or existing fixtures change.

Full post-extension target execution:23 controls compile/run,10 intended assertion
REDs /13 GREEN /zero ignored. The first20 test bytes remain exactd3d30; only the
three retirement controls are appended. Scoped target strict Clippy passes. No
push or CI dispatch; the four frozen PR heads are untouched.


### HMAC review and release-hook fit

4yjlw0 reviewed898e0183040 HMAC/native/original CLEAR, including all three
independently recomputed literals and both Futures deadline phases. The real
wire-MAC assertions comparing against native sign::Signer are parity evidence;
only the literal vectors are independent HMAC evidence. Original-review CLEAR
does not supply the missing dispatch nonce or socket epoch authority.

Neither Kraken implementation overrides on_http_release on actual219d. Both
use Venue's default Unscheduled admission. Native mutation queues advance their
waiting/step state before issuance, not during final Claim. A new pure explicit
Unscheduled nonce-release opt-in fits those consumers, provided unsent repairs
retain the original pending/selection/queue position without re-emitting progress
or advancing a step. Default Unsupported for unreviewed nonce consumers is safe.
This is consumer-direction fit only; exact pure-hook and global-floor/Claim
composition remain core source-review gates, with no callback under the lock.

## Remaining socket and hosted-nonce prerequisites

Shared socket owner5c57jl's bounded fit at facd9e4c53e is HOLD for exact native
Futures challenge timing. Native CONNECT starts no auth deadline; OPEN starts
15s, and acceptance of a nonempty current challenge resets another15s while
subscription ACKs remain pending. The separate180s payload watchdog is retained.
Current AuthEvidence::Accepted clears auth and permits normal interests, so it
cannot stand for challenge progress. Supervisor has no current-message progress
reset hook; the CONNECT-origin #1970 snapshot policy is not this native bound.
A narrow shared prerequisite or explicit owner behavior ruling is required.
No fake Session/issuer, venue auth timer or early full-auth classification.

An immutable once-per-epoch challenge plus the actual FeedId/socket_epoch can
bind signing; current managed-socket completion/final dispatch fence retirement.
That alone does not settle the deadline gap. Whole-family socket geometry also
needs review: a managed private feed alongside legacy public sockets repeats
the mixed-dispatch hazard. The combined signer must not imply that either gap
is closed by provider custody or the existing Spot token Session fit.

Nonce owneruufub4 reports local receipt schema d5743f48f2f and exclusive-release
registration ff6c2e127c0 after bounded source reviews. Neither is pushed/landed,
and full dispatcher/provider/Claim composition remains unapplied. These are
prerequisite artifacts, not an API/runtime clearance for this venue.

The distinct hosted-forward gate is recorded in shared plan dbca148f90e. A
captured SignRequest and provider/hosted results must carry the exact original
engine-reserved canonical decimal u64 nonce end-to-end. Stamp-only Input::Http
or hostedSigningClock is not nonce support. The server must not allocate a
second nonce. An unsupported compatibility adapter refuses before private I/O,
rather than returning a late InvalidResult. This stays separate from proving
continuity between the old secret-digest and new trusted-stable-id namespaces.

This update changes only the plan. No original control, production, Cargo,
manifest or provider code changes; no new local build or execution claim. Full
nonce release-fence/repair source, hosted forwarding, namespace continuity and
Futures socket progress fit remain gates before the combined production cutover.

## Authorized local recipe phase before account adoption

Overseer now authorizes Futures+Spot HMAC recipes, Base64 setup declarations and
opaque role/selection wiring locally. Account-read caller migration follows the
recipes; size that by physical funnel, not lexical matches. Actual12da/dcb9 has
23 inventoried occurrences, with pending2072 accounting for one (effective22).
Account Futures2+Spot4 are six lexical occurrences but three shared physical
funnels; Spot's other three are macro matches. No prune is claimed in this phase.

Branch feat/rust-kraken-dispatch-recipes starts from reviewed0ebbe and actual
landing dcb9e6de01e. The nonce owner supplies ff6c2e127c0 plus corrected immutable
patch e64954dec0e981a02d5934b89ad39c0e8339b29c583b618632a71efc580f093b,
superseding57763. Its runtime hunks are consumed verbatim; only the append-only
CALLERS hunk composes after incoming docs. This is explicitly local composition,
not a nonce-core freeze or landing. No firm owner ETA: correction review,6b
barrier guard and host/Bearer/StampFloor composition remain open. Do not edit the
dispatcher, copy its allocator, or infer readiness from this recipe phase.

Recipe interface: a private selected wrapper is minted only by the shared
Slots::select_with_nonce(role,&KRAKEN). It retains the exact Identity, Selection
and non-Clone NonceClaim; every physical request needs a fresh wrapper. Futures
Slots use secondary_requires_primary:true, while Spot drops the ignored read
identity. Missing/malformed stable subject uses existing InvalidRequest before
queue IDs. Explicit role is supplied by the declared call site, never by method.
Base64 secret decoding is a trusted setup declaration, never venue decoding.
Native key bytes stay untrimmed; no secret hashing or inferred subject alias.

HTTP prepare reserves once through DispatchContext, then returns one atomic
Present + SHA256 + HmacSha512 batch. Futures hashes form+nonce+stripped path;
Spot appends nonce last to the ordered body, hashes nonce+body, then MACs path
plus that digest. The exact u64 is in SignReceipt.nonce; stamps stay independent.
Prepare/apply attest method/origin/path/body/static headers. Provider receipt,
value counts/digest/MAC sizes and API-key header safety must pass before any
signed header publication. Stale preparation recomputes from immutable unsigned
inputs. Recipe freshness is an explicit caller argument, not a guessed logical
account/session timeout. The caller still supplies genuine managed authority.

The Futures challenge primitive can build the same atomic Present/SHA256/HMAC
program without nonce allocation or transport authority; managed socket epoch,
progress timing and final send wiring remain entirely in5c's separate adoption.
No raw key or MAC is cached as a replay recipe. Hosted canonical nonce forwarding
belongs to tealstreet-next: preserve it as a hosted-mode certification gate,
not SDK implementation scope. No server-side second allocation is permitted.

Use an isolated recipe control target. New APIs initially produce compile REDs
labelled NEW-CONTRACT, distinct from the existing23 native-original controls.
Then drive real managed Prepare/Bind/Claim and installed LocalSigner identities
for HTTP vectors, per-role selection, shared-subject monotonic reservation,
provider refusal/defer/tamper, attestation and header order. Challenge primitive
vectors prove only provider bytes, never socket adoption. Native provider-bypass
originals remain RED until their later caller cutover; do not rewrite them green.

No native account/trading/history/Spot/socket caller, registry constructor,
construction-rules admission, migrated-family checker or manifest changes in the
recipe phase. In particular generic SelectedRead currently has no coupled nonce
mint seam from its retained snapshot: report that composition gap before later
account adoption rather than reselecting by method or copying a read helper.
Raw legacy callers are not a fallback for the new recipes; the recipes have no
raw path. Local only, no PR/push/CI until reviews authorize the intended head.
