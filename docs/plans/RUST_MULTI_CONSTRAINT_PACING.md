# Multi-constraint HTTP release pacing

## Assignment and scope

Owner codex-5c57jl; reviewer codex-uufub4. Baseline actual
c55cd23074d80fbb7a59500e0e0e227d9e672551; branch
feat/rust-multi-constraint-pacing. This is a small engine prerequisite before WOO
signer production. WOO snapshot #1788, sockets #1930 and signer remain separate.
No venue, host wire, scheduler, threshold, inventory or feature-profile change.
Plan, compiling original controls, implementation, independent source/runtime
review, scoped checks, then engine PR through the train. No merge without GO.

## Reviewed declaration shape

uufub4 cleared this API direction, not a source pin: keep Policy/Context's scalar
Option<Pacing> and existing immutable leg/retry/replacement/Plan surfaces. Add
PacingSet { group:u32, constraints:&'static [Pacing] } and default-empty
Venue::http_pacing_sets(). Existing Venue::http_pacing declares primitive floors.
A scalar group names either a primitive or a flat immutable set. Set references
are unique primitive IDs only; no nesting, cycles or set-owned clock. Primitive
and set IDs are disjoint. Validate finite nonnegative member/floor gaps, missing
IDs, duplicates, nonempty sets and bounded storage before engine installation.
Bound sets and members at16 each; pre-resolve indices and gap overrides once.
No allocations on eligibility check/Claim, no per-Context arrays or venue clock.
SendPacing::new remains the scalar-compatible constructor; with_sets installs
both declarations. Two sets sharing a primitive share its ONE last-release state.

Outer request minimum_gap_ms maxes each referenced member's declared gap and
primitive floor. WOO declares primitive global100 and algo500; algo-set outer0
references both, ordinary uses global100 and PUT global200. No 500ms global
workaround. Context carries the immutable selected ID across route generation,
retry and Plan admission; declarations cannot be replaced while requests live.

## Atomic Claim and eligibility

Resolve/validate EVERY member before admission. Return WaitUntil(maximum current
eligibility) if any member is blocked, or TimedOut when that wait reaches the
original physical deadline. A successful admission charges every member together
using max(primitive floor, member override, outer request gap). Only Released
updates state; WaitUntil, Invalid/Stale/TimedOut, admission Err or Denied changes
none. One admission invocation and one transport release, never one per member.
No fallible resolution/allocation after admission. Preserve raw time and checked
lease/signature/deadline handling byte-exact outside the pacing call sites.

No early signing: after route preparation and before activating a managed signer,
check the SAME expanded registry. A blocked prepared request retains exactly its
existing host resource, immutable physical deadline, route expiry and signing
custody; use its existing managed timer for the earliest pacing/route/deadline
boundary. On ordinary timer ingress validate all bounds and current preparation
again, rearm an early wake, then activate signing only once eligible. Duplicate
prepared notifications or stale timers cannot re-activate or replace custody.
If the route expires first, use existing reprepare rather than renew it; if the
physical bound expires, use existing typed failure/Plan cleanup before provider
or entropy. Final Claim rechecks all constraints because another release may
have moved a shared primitive since signing began. Prepared-before-wait is not a
physical release and consumes no rate slot. No new timer/attempt/timeout rights.

## Zero-pacing composition

G2b's zero-pacing admission must inspect EXPANDED primitive/member floors even if
outer gap0; a100/500 set cannot masquerade as zero. Provide one canonical
read-only SendPacing resolver/zero check, coordinate exact API with hcmq3f before
G2b composition and require refusal before resource IDs/admission. No copied
member expansion or inventory exemption. Owner/head coordination is pending;
source integration must consume the reviewed resolver, not infer zero from the
outer scalar. A positive zero-floor set remains admissible only when every
expanded floor and outer gap is zero.

## Controls and review gates

Original compiling core controls use existing scalar signed managed requests to
expose provider activation during a known pacing wait, including delayed route
preparation. They are assertion REDs, not missing-API compilation failures.
The WOO original checkpoint14a92 separately demonstrates native handoff drift:
100/PUT200/algo500 collapse to10ms after delayed1800 handoff/1810 ACK, paired
with native positives. Those are action-handoff witnesses, not managed Claim.

New set/Claim controls must pin algo0 -> ordinary100 -> algo500, PUT's200 global
extension, two distinct set IDs sharing a primitive, second-constraint wait or
invalidity with no partial charge, admission Err/Denied consuming neither,
per-call/member/floor max, zero set/unknown/duplicate/nested/collision/overbound
validation, scalar/default behavior unchanged, deadline equality and reprepare.
Real Engine controls must park provider work, delay route completion and Claim,
move another shared release between signing and Claim, prove no Sign/entropy
before eligibility and fresh stamps thereafter, expiry-before-wake and early/
duplicate/stale timer behavior. Capture exact raw deadline and preparation
identity before/after waits and reprepare. Exercise default feature-off and
managed-sockets/http-plans combination; keep Plan/fatal/expiry cleanup rails.
Run scoped tests/strict Clippy/fmt, both bans and targeted negative controls.
Full feature/size/CPU gates run in fresh CI; one watcher at120s or more.
Independent uuf source/custody/runtime review and PR/train GO remain required.

## Original core checkpoint

New integration target multi_constraint_pacing uses only the existing public
API and actual managed route -> provider -> Bind -> Claim path. Original result:
2 named assertion RED / 1 working GREEN / 0 ignored. After a successful Claim0,
route preparation10 wrongly starts signing before eligibility100; a provider
parked until300 and delayed second route310 wrongly starts signing before400.
Eligible route/provider/Bind/Claim with one release remains GREEN. Initial fixture
compile needed Venue's required callback methods and Debug on Pending; corrected
before the recorded assertion-only receipt. No production source changed in this
checkpoint. Shared-Mac build jobs2. Command:
`cargo test -p safe-cex-engine --features test-support --test multi_constraint_pacing`.

G2b owner hcmq3f confirms #1956 local ee6e790fef0, source28e1bb36f3e, remote
older/notREADY: validate_mutation_group currently checks scalar zero before IDs
via request_context_group_plan::normalize. Their narrow helper must call our
canonical expanded-floor zero resolver with the same Result signature; normalize
stays unchanged. They own that future composition and alias no-ID controls;
this PR owns resolver/storage/check/released and prepared-signing wait custody.

## Source checkpoint

Original RED commit5b348d03356 is retained. Implementation adds flat resolved set
storage and validate_zero on SendPacing, the default-empty venue declaration,
construction wiring, pre-allocation invalid-group rejection and prepared signer
waiting on the existing managed timer. No Context/Policy/Plan or host-wire format
change. Final Claim's admission/deadline/signature sequence is unchanged except
that its existing check/released calls now resolve all primitive constraints.

The new dedicated target passes15/15 on default-off engine features with
 test-support. Added cases cover alias/primitive shared state, denial/second-bound
atomicity, WOO+PUT, maximum member/outer floors, canonical expanded zero checks,
configuration rejection, raw-bound expiry, early/duplicate wake, route-expiry
reprepare and a later competing primitive release before delayed final Claim.
Recipe preparation counter proves no invocation before the expanded algo floor.
Combined feature and existing lifecycle regression checks remain in progress;
independent review, strict checks and fresh CI precede readiness.
