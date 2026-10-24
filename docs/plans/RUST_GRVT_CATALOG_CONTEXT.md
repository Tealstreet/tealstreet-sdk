# GRVT unsigned instrument catalog custody

Plan-first base: actual5ae97ac93c6e7ad0bacc1ad06039f1856c098019.
The overseer approved the one unsigned catalog sender plus deletion of one
provably unreachable option-refusal arm under the #2074 ruling. The plan and original controls
were committed before production; both independent original reviews are CLEAR.

## Scope and inventory

The startup/refresh sender is Grvt::fetch_markets in lib.rs, an unsigned POST to
/lite/v1/all_instruments on the native mainnet/testnet market-data host, with
body{} and Content-Type application/json. Native streaming accepts r/result.
Job::Markets stages rows, signing metadata and malformed instrument identities.
The native startup Backoff continues indefinitely after failure; successful
publication schedules the existing300000ms refresh. No transport retry loop is
added inside one catalog read.

Migrate exactly the fetch_markets legacy self.request occurrence:
7f179c914a5bbc8e6e8e7df1c90d04c0c10713495d542a0b5c719caf776b76c1.

Separately DELETE the unreachable FetchOptionChain/FetchOptionExpiries/
FetchAccountGreeks arm that calls cx.fail with Options are not enabled on this
account (cab66fc0399e6d1f53504198a9a5df6893df31bad853bf84c1dcd416c2326cdd).
The engine checks capabilities.options before begin_command_entry and refuses
all three commands without calling GRVT. GRVT leaves options=false; no internal
entry calls on_command for them. Delete this arm, letting the existing unsupported
match path cover exhaustiveness. Do not claim adoption or a new authority path.
A control must prove the engine's native pre-entry refusal and zero venue entry.
CALLERS explicitly labels this as dead-code deletion with no behavior change.

These are two singleton lib inventory removals:8->6. GRVT has34 occurrences on
this base, giving32 here. The independent pending #2076 and #2081 removals project
29->27 after both land: one actual catalog migration plus one dead deletion.
Do not prune or claim their trading/account keys. Preserve every foreign raw
inventory byte/order. No renamed conservative reference is counted as a removal.

## Engine authority and wire

For every genuine startup, retry or periodic refresh, http_context with
Scope::Background and Policy(Read, credentialNone,20000ms,no pacing,Never)
creates a fresh unsigned root. request_managed_with_headers queues the exact
native POST/body/header, then stream_response uses the native r/result envelope.
Real Prepare/Bind/Claim is required; no raw HTTP or signed fallback exists.
Admission/preparation refusal follows native startup failure/backoff, with no
command terminal. Every retry timer obtains a distinct new root.

The20s admission-to-expiry bound includes route preparation, held Prepare/Bind
and physical I/O. It is a NEW caller contract: native catalog timeout_ms=None.
CALLERS must disclose it, including context-pool contention/refusal before I/O.
There is no caller receipt or command cap for this Background read, no G1 task,
Session, Preflight, Plan, retry grant or key selection.

Keep root authority in the Job::Markets payload. Pending.context remains login
Session-write-only and is always None for catalog. Do not stamp Session egress
or borrow private login authority. Route managed catalog callbacks ahead of
login take() and generic legacy callbacks where needed, preserving all other
jobs' default delegation exactly.

## Publication and retirement

At the first line of every catalog item and final response callback, validate
validate_background_read_http_delivery(root) before status/codec/stage changes,
metadata/Store mutation or native startup/private continuation. An expired,
unclaimed, old or foreign delivery cannot publish, clear startup backoff, open
sockets or initialize private state. Validation is authority only, not evidence
that the native status or rows are valid.

Preserve native row parsing/skip notices, PERPETUAL filtering, instrument-id
fallback, malformed held-row retention, leverage-rail preservation, market and
ticker order/remapping, signing metadata replacement and all first/new-market
continuations. Publish synchronously and finish the root exactly once AFTER
successful native publication/continuations, before returning from the callback.
Do not move Store publication after retirement. Failure paths retire before
scheduling the existing Backoff; stale DATA does not invent a replacement event.

Status/protocol/integrity/empty catalog failures remain native startup failures.
Provider/preparation refusal and physical timeout also retire and take native
startup backoff; no immediate ReadFailure retry or auth refresh is granted.
Failure callbacks provide no DATA publication authority. Preserve startup
Backoff deduplication and cancellation on success and300s refresh exactly.
An old callback cannot cancel a newer read's backoff or overwrite its catalog.

The managed startup success may call existing connect/private_start/refresh_leverage
as before, but their implementations and account/login/signing/socket source stay
byte-exact. Shared generic request/transport handling for every noncatalog job,
private auth refresh, queues and financial reducers remain outside this slice.

## Original controls and independent review

Use actual Engine/GRVT ingress and real prepared resources, Bind and Claim; raw
compatibility is allowed only in original wire/reply-ingress probes, visibly apart
from managed-custody assertions requiring Prepare. A legacy reply probe conveys
no managed release authority. Do not fabricate publication, root provenance,
metadata or login state. A thin delegator may defer native start until real peer
commands fill the pool; that scheduling is a fixture, not a simulated catalog.

Working controls pin both deployment hosts, ordered POST/body/header/envelope,
row filtering/order, malformed-row retention and refresh metadata behavior. Keep
existing products/markets_refresh/startup_resilience assertions unchanged except
transparent real managed drivers. Pin startup backoff without assuming a fixed
jitter value: compare native timer disposition, and preserve300s refresh.

Separate new-contract assertions cover eager raw I/O before Prepare, unclaimed
DATA, preparation refusal, held/unreleased20s expiry, released physical timeout,
late item/DATA inertness, strict status/integrity/empty response and fresh-root
retry/refresh. Full256 genuine Command Read owners must refuse catalog admission
without outbound work; retire the exact peer and validate255 after recovery.
Pin cleanup/capacity reuse and duplicate old callbacks. Disclose suffixes gated
behind the first Prepare assertion on original production.

For deletion, invoke all three option commands through genuine Engine entry,
confirm exact native pre-entry error and no I/O/Store change, and count zero
forwarded native GRVT on_command calls. A positive ordinary command proves the
delegator is wired. Do not alter capabilities or construct fake direct entry to
make the dead arm reachable. This control passes before and after deletion.

Commit plan before compiling originals; runtime assertion REDs only, with working
controls and no missing-method/setup failures counted. Core review isfe7lgm and
native/preservation review is9czdjh. Production awaits both original reviews.
Source is confined to catalog-related lib.rs hunks, the dead arm deletion, own
lib inventory, dedicated control/plan and publication docs; no account/trading/
identity/history/signing/host/shared-engine or key-selection source edits.

Scoped local Cargo uses CARGO_BUILD_JOBS=4, one own build at a time; clean only
own target over15G between builds. Strict scoped Clippy/fmt and ratchets follow.
No broad local builds or WIP CI; publish an intended freeze draft only. Source
CLEAR plus green exact-head CI permit READY; CPU infra retry requires a fresh
workflow_dispatch attempt1 cohort, never failed-job attempt2 proof.

## Original execution checkpoint

On unchanged actual5ae97 source, the dedicated target compiles and executes26
controls:14 intended assertion REDs/12 GREEN/0 ignored, with no missing-method,
compile or fixture/setup failure. Managed startup/bound/prepare/held/timeout/
refresh/cleanup and stale-stream suffixes fail first at the genuine Prepare
assertion against eager raw HTTP. Full256-context admission fails at leaked raw
outbound work. Unclaimed DATA and late DATA at20s independently publish on the
legacy path, exposing the missing admission/deadline contracts.

The mid-stream test drives a real item callback (count1), confirms Store is still
unpublished, then finishes at the new equality deadline: native final DATA
publishes, yielding the intended assertion RED. It is not a claimed execution
of a Prepare-gated suffix. Native wire/reply ingress probes explicitly accept
legacy HTTP on original production; managed-custody probes never do.

Working controls pin both deployment wires/publication/300s refresh, native
1000/2000 jittered backoff, strict HTTP/code1000/empty/missing-envelope/nonUTF8
failure, success followed by late failure with no extra backoff, and all3 engine
pre-entry option refusals with zero forwarded venue calls plus ordinary-command
positive controls. Seven existing library controls pass unchanged, including
noncatalog preparation default delegation and login composition. Capacity
recovery/255-survivor, root cleanup/reuse, held release and managed retry/refresh
suffixes remain unexecuted after earlier REDs until cutover. Production, Cargo,
all inventories and every existing fixture remain byte-exact the actual base.


## Production checkpoint

The catalog sender now carries its own Background root in Job::Markets and
validates every item, final response and stream-error delivery before native
mutation. Success retires after all synchronous native continuations; failures
retire before startup Backoff. The dead options arm is deleted independently,
with all three actual engine pre-entry refusals unchanged. Exactly two singleton
lib hashes disappear (8->6); no foreign key or predicate changes.

All26 originals now pass, including the previously gated suffixes. One appended
source-stage control proves that Shared public-feed mode still expires an
unanswered catalog read and follows native Backoff without socket activity.
Existing transport collectors in the delegation/public/startup fixtures now
drive real Prepare/Bind/Claim. The socket-supervision fixture services its allowed
REST refresh instead of leaving it unanswered under the new20s contract; its
zero socket supervision assertions remain. The old diagnostic describes the
serviced refresh accordingly. Original26 assertion bodies stay unchanged.

Scoped library, account, login_session, products, markets_refresh, public,
startup_resilience and request_context_catalog execute123PASS/0ignored. Strict
all-target GRVT Clippy -D warnings passes sequentially with CARGO_BUILD_JOBS=4.
Source review and intended-head CI remain required before READY.
