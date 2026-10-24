# Crypto.com unsigned PublicTrades Read custody

Plan-first base: actual landing 1e2458b1c98. Owner: codex-ih35js. Scope is
unsigned PublicTrades only; implement after/stacked on reviewed #2052, as ruled
by claude-ddi2il. This checkpoint contains no production changes.

## Exact inventory and ownership

The real checker fingerprints the sole unsigned history GET at
venues/cryptocom/src/history.rs as
073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7 (one occurrence).
Remove that singleton only. At the base, Crypto.com has 12 fingerprints and
13 occurrences; #2052 removes the discovery singleton, leaving 11/12. This
slice then leaves 10/11. History itself goes from 4 fingerprints/5 occurrences
to 3/4. Its remaining lexical witnesses belong to private paging, not this read.

Open-PR file audit at the base: only own #2052 touches Crypto.com lib.rs and
shared test preparation; no other Crypto source claim. Original controls use a
new standalone test file and do not change existing fixtures. Future lib.rs
edits are restricted to a distinct PublicTrades Pending and History routing,
typed failure and exact-owner retirement arms; stack after #2052. No signing,
key-selection, engine, Cargo, private account/trading or combo-5b source changes.
Raw secrets and legacy private signing remain outside this unsigned adoption.
The signer owner confirms no full-custody private Crypto.com pin exists.

## Native contract

Query::PublicTrades bypasses the private credential requirement. Native time
range validation still precedes remote work; limit zero and equal bounds retain
the existing managed synchronous empty result. With positive limit the endpoint
is unsigned GET {rest}/public/get-trades?instrument_name=<encoded>&count=<n>.
Default requested limit is 25; wire count is min(limit,150). Both production and
testnet use empty headers/body, Public routing and no signer/auth evidence.
Unknown symbols are sent verbatim; no market lookup or startup requirement.

The endpoint returns one streamed result.data page. Preserve input order and
repeated trade IDs; truncate at the caller limit, never sort/dedupe or walk a
cursor. Preserve existing id parsing, side mapping, timestamp milliseconds,
price/quantity parsing and quoteQty=price*qty. A malformed row rejects the whole
unaccepted page; no accepted prefix or INCOMPLETE marker exists for this one
page. Existing history.rs and tests/history.rs public positives remain intact.

The native per-path next_by_path queue spaces submissions by 10ms across callers.
This is dequeue eligibility, not proof of release spacing after asynchronous
preparation. Preserve native eligibility and give the original root and each
child the same canonical Context Pacing group with minimum_gap_ms=10.0, so
actual releases also keep the bound. No venue release clock/ledger/timer is
added. Charge the native queue slot only after successful child submission;
engine pacing owns release reservations. Pin delayed preparation/claim plus
two callers in adoption controls before claiming physical spacing.
Transport failure has at most three retries/four requests, each after exactly
1000ms; failed streamed staging is discarded before retry. The existing 40101
native-response case grants one retry after 1000ms and stays unsigned. Other
HTTP/native failures are not retried. Do not introduce substring-based auth
health evidence or new status ladders; retain existing native policy, with any
required shared classifier decision reviewed before implementation. Public
40101 must never produce AccountAuth or increment auth health.

## Shared authority and callback shape

Use genuine admit_caller_read_root(None,Some(public-trades Pacing)) at entry, then
ReadCompletion::from_context plus ReadHistoryRows for the one response. One
immutable receipt deadline spans path-queue wait and all native retries; absent
host timeout is already stamped with the SDK default. Missing receipt refuses
through shared admission, never a local default/root or numeric fallback.

Mint each credential-free Read/20s/Never child via http_leg_context from the
original root at eligible native dequeue, capped by that root. Each physical
request uses request_managed and the same result.data stream location. Route
preparation is inside the child bound; no retry renews the root or reuses an old
physical delivery. Use actual ReadHistoryRows admission before parsing or
staging, before accepting a page, and before success/error response policy.
Use the original root for terminal completion. Retire each child before retry;
failed staging is discarded without accepting it. A denied local child or
submission is strict through that exact live root, with no copied authority
checks, new native timer or numeric fallback. Genuine revoked owners are inert.

Engine root retirement drops only that exact Owner's queued PublicTrades work
through read_root_retired; do not clear another command or reused numeric ID.
Root expiry is strict typed Timeout with no rows/marker/finalizer and no later
send, even while queued or parked. An expired physical child under a live root
retains native transport retry policy, bound by the root. Matching typed failure
owns cleanup only, never DATA. Uufub4 API-fit confirmation is required before
production; this plan does not claim the retry composition is already reviewed.
Private CursorHistory and signed calls retain their existing implementation.

## Original controls before source

Commit this plan, then add compiling real-Engine controls on the unchanged
PublicTrades sender. Record each genuine assertion failure separately from
working native controls. Never claim compile failures as RED evidence.

1. Public preparation blocks physical release; exact unsigned URL/count/network,
   routing and real prepare/bind/claim stay pinned.
2. Explicit short caller wait caps a child, including route parking and native
   10ms queue wait; both expiry ingress orders, equality and late readiness deny
   data and terminal reuse. Missing clock and genuine owner capacity deny work.
3. Root retirement while queued or waiting for retry prevents sends; unrelated
   live callers survive. Duplicate live numeric IDs cannot create a second read.
4. Stream a live prefix then end/fail at the deadline; no partial rows/marker.
   Reused ID plus stale old response cannot settle the new owner.
5. Native positives: exact escaped wire on both networks, default/large/zero
   limit, duplicate IDs/order/financial values, malformed whole-page failure,
   10ms queue cadence, transport four-attempt schedule and prefix reset,
   unsigned 40101 one-retry/no-auth, other native refusal/no retry.

After review, implementation is a separate checkpoint. Prune only the singleton;
run scoped Crypto.com tests and strict Clippy sequentially with CARGO_BUILD_JOBS=4,
fmt, request checker/test pair and bans. No local workspace/WASM/replay sweep.
Independent source/adversarial, caller and full-map reviews are required. Push a
small draft and let CI certify family bytes/CPU/replay. No merge without GO.

Architecture gate: No if shared entry/root/leg/row algorithms suffice. If native
retry composition needs a new engine seam, stop and report before source rather
than add a per-venue issuer, lifetime, finalizer or retry-authority algorithm.


## Bounded API direction

Uufub4 confirms original-root/new-child composition on actual 1e2458b1c98 fits
without Plan. Failure-only consumes the returned Pending, validates the live
root, retires the old child even at physical expiry and resets failed staging
before a native retry. Preparation failure and root expiry are strict. This is
API direction, not source or executed-control clearance. One exclusive public
Read authority is required; use a distinct PublicTrades request/Pending instead
of retaining legacy and managed terminals for the same public query. Native
private authority remains in its existing closed branch.


## Original executed checkpoint

The dedicated real-Engine target compiles and runs 17 controls on unchanged
1e2458b1c98 PublicTrades production: 8 assertion RED / 9 working GREEN / zero
ignored. REDs are eager route release, root equality DATA, absent root timer,
live streamed prefix at expiry, expired native queue dequeue, retry root renewal,
duplicate live owner and shared capacity bypass. Timer-first fails first at the
missing caller-bound timer; later inertness assertions are adoption gates, not
independent original evidence. The queued/retry/duplicate/capacity controls fail
first at actual physical HTTP where none should be allowed. Invalid clock is
already a working shared boundary control, not a new defect.

Working controls pin both exact hosts/escaped wire/default/count clamp, duplicate
rows and financial values, zero limit, native 10ms eligibility, four transport
attempts with exact 1s waits, failed-prefix reset, one unsigned 40101 retry with
no auth evidence, malformed-row rejection and other API refusal. An initial
combined positive omitted result.data from its API-refusal envelope; the native
stream parser correctly treated that as transport failure and retried. Corrected
the complete-refusal fixture and added a separate working missing-array retry
control. This fixture mismatch is not counted as a RED or a production defect.
Malformed envelope/stream retries retain native transport treatment; malformed
rows within a valid array still reject the unaccepted page without prefix.

No existing source, test, fixture, checker, inventory or Cargo file is changed.
Only this plan/index and the new dedicated test differ from the base. Original
assertions remain identifiable for implementation; no ignored tests or test-only
venue production path is added. Production awaits bounded reviews and the #2052
stack. Family performance/bytes and broad CI are not claimed by this checkpoint.


Scoped strict Clippy on the new target passes. The normal inventory check
currently fails only on foreign WEEX depth_query.rs/lib.rs because the remote
landing advanced past this intentionally unchanged original base. The manifest,
all Crypto production and existing fixtures remain exact 1e2458b1c98; no checker
override or foreign prune is used. Integrate the actual tip and reviewed #2052
only after recording this original checkpoint, then rerun normal checks.
