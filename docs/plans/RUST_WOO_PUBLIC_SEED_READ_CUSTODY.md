# WOO unsigned public-seed read custody

## Status and prerequisites

The overseer authorizes local originals and source implementation; all work stays
unpushed while exact review and qualification proceed. #2169 landed as25ca8466d63d8f39954618635305cc083f9ec3f2,
which is integrated locally without rebase. Its exact landing tree is
3bc41fd92fe8e1baa8db1e3fbc9d6065fc0c09b8. Reviewer: codex-hcmq3f.
The plan/matrix direction is CLEAR in scratch artifact
ad94dc2a7d30df24af1eee7ce7cdace07e78246c9000ef389471e9b8772686b4;
this committed copy updates status and adds the index row, with the reviewed
API and complete typed-matrix proposal retained. Originals qualification and
exact review remain before any production candidate.

The committed funnel map is RUST_WOO_ADOPTION_AND_FUNNEL.md. This narrows its
unsigned Background observation block, without taking caller candles, depth,
identity/account/history or signed writes. Recheck open PRs and checked-out
branches before implementation: the current query found only #2169 as a WOO
production PR; #2079 mentions WOO but is a generic command-child read delivery
PR, which this Background-only block must not depend on or edit.

## Sites and expected size

Two raw issue sites in venues/woo/src/lib.rs: seed() selects either Markets or
Tickers, and funding_read() issues Funding. The observations are three native
routes: GET /v1/public/info, /v1/public/futures and /v1/public/funding_rates.
Expected production delta: roughly 200–350 lines across a focused seed-owner
module and dispatch integration, plus additive contracts and reviewed startup
fixture release adaptations. Exact count/fingerprints must be regenerated from
the actual landing before originals; expected request occurrences 30 -> 28,
signing 20 and credential 6 unchanged, socket 0 retained. No generic engine,
signer, provider, credential or protocol change.

## Existing native behavior to preserve

Markets/Tickers are streamed rows. Their Pending owns all staged rows and
diagnostics. on_response first checks status and Envelope, then publishes.
Markets rejects empty results, keeps existing listed markets/ranges on refresh,
tracks skipped/anonymous rows and bounded reread stalls, and may schedule the
bool-keyed seed Backoff. Only its first accepted catalogue starts private account
work or publishes empty public-only account state, then starts the ticker seed.
Later catalogue responses return after refresh publication/recovery handling.

Tickers rejects empty results, publishes native indexes/rows, clears its Backoff,
refreshes account, connects the public feed and arms the funding timer. Preserve
the native reducer, public suppression, private eligibility and socket lifecycle.

Funding stages rates/times then updates existing tickers. Failure warns only.
The genuine funding timer schedules its next observation every 60 seconds at
dispatch, independently of response success; there is no immediate failure retry.
Markets/Tickers failures warn and invoke their existing keyed Backoff. Those
later timers create distinct observations; they do not renew an issued Context.

Do not invent a latest-per-kind gate. Keep exact physical custody distinct from
catalogue_committed, reread_stalls, retained markets/ranges and native ticker
publication rules. A still-live older request retains native behavior; duplicate,
foreign, retired or expired delivery cannot publish or trigger successors.

## Proposed shared API use

One genuine Background Read Context per physical observation, keyless with
Primary egress and the native unspecified route (None). No PublicCors route,
SelectedRead, credential binding, caller receipt, fabricated Command or socket
authentication/HTTP retry grant. Use the existing keyless managed streamed-read
API and retain its exact issued owner with the native payload in Pending.

Proposed NEW per-observation policy is 20_000 ms / RetryPolicy::Never. This is
not an aggregate startup deadline and not a renewal of an old Context. Get
hcm's exact API-fit and the local-refusal/recovery matrix disposition before
originals; document the new bound and any changed admission row in CALLERS.

DATA validation precedes every row, staged diagnostic, response status/envelope
diagnostic and Store mutation. Publish accepted native state while DATA is live.
Then retire that exact owner before Backoff work, private startup, ticker seed,
account refresh, socket connect or funding-timer effects. Use explicit deferred
native effects so retirement is not moved before publication as a shortcut.

Failure-only cleanup validates/finishes the exact issued handle without requiring
expired DATA permission. A genuine core-returned expiry gets once-only cleanup;
duplicates cannot warn twice or rearm recovery. Nonfinite admission/callbacks
produce no new read or timer. Keep native finite failure presentation and keyed
Backoff behavior, and funding's warning-only response policy.

Typed refusal/failure matrix (corrected exact proposal for review):

Ownership is established only by the exact issued Context and once-only successful
retirement. Error::Duplicate/Revoked/State/Stale/RetryForbidden is never a proxy
for foreign or retired ownership. A genuinely owned callback always attempts its
exact retirement once, including expiry without DATA. Only an actually foreign,
invalid or already-retired handle is inert. No latest-kind ownership filter.

At finite admission time, Capacity/TimedOut schedule native keyed Backoff for
Markets/Tickers, or one Warn only for Funding. Every remaining admission reason
(Invalid, Duplicate, State, Stale, Revoked, RetryForbidden) warns only, with no
additional recovery. There is no issued Context to retire on admission refusal.

At finite precommit/queue failure, retire exact owned custody first. The ONLY new
recovery whitelist is reason Capacity or TimedOut, or reason Invalid with error
kind Network/Timeout/Internal. That whitelist uses native keyed Backoff for
Markets/Tickers; Funding warns only. All other precommit reasons (Duplicate,
State, Stale, Revoked, RetryForbidden), and Invalid with None or any other kind
(Rejected, InvalidInput, AuthInvalid, AuthExpired, AuthPermission, RateLimited,
VenueUnavailable, Unsupported), are owned warning-only dispositions. No reason
alone makes cleanup inert or grants recovery. No message classification.

At finite released failure, preserve native on_request_failed for EVERY reason:
Network, Timeout, Internal, decode, rejected status/envelope, native empty result,
and any other callback all give the existing Markets/Tickers Warn+Backoff versus
Funding Warn-only. Genuine owned20s expiry has that same once-only failure
cleanup. Each Backoff wake is a distinct observation, never renewal or retry of
old Context authority. Unaccepted rows/IDs/diagnostics never publish.

Every finite funding timer DISPATCH ATTEMPT rearms EXACTLY ONE native60s cadence
timer, including admission refusal, even a permanent refusal. The failure row
adds NO EXTRA recovery timer and never resets/clears that cadence. The timer is
independent of reply success; failure callbacks do not create another cadence.

Nonfinite admission/dispatch/callback is cleanup-only: no read, timer or diagnostic,
and no unrelated-work damage. A returned owned handle may retire/cancel exactly;
foreign/duplicate work remains inert. Existing cadence is not converted into a
new timer at a nonfinite callback. All finite refusal paths must distinguish
absence of DATA from legitimate once-only failure cleanup.

## Originals and source-stage controls

Preserve every existing WOO assertion, financial literal, timestamp and native
wire expectation. Before source, pin genuine Prepare/Bind/Claim release and
held/equality/late streamed rows, status/parse/ROW_SKIPPED denial, complete Store
and action comparisons, released expiry, duplicate failure and NaN/+Inf/-Inf.
Retain working native empty/partial catalogue, bounded reread, listed-market
retention, ticker/position valuation, public-only and independent private setup.

Pressure: 255 genuine managed peers plus the completing predecessor must prove
its exact released credit admits its successor, with all peers surviving and
full eventual reuse. Keep DATA publication-before-retirement as an exact source
order check too. Pin Markets -> native account eligibility -> ticker ordering,
Ticker -> native account refresh/public connect/funding scheduling, and funding
timer/reply independence. No blanket timer drain or predicted resource IDs.

Startup fixtures may need local real release of the two managed seed kinds at
their existing 0/10/20 clocks. Shared raw requests() stays unchanged; no broad
private/trading collector rewrite. Submit exact assertion/payload/clock proof
for any existing fixture adaptation before applying it.

## Qualification and size

First-source local WOO in #2169 is 1,321,677 / 1,343,488 (21,811 headroom),
strictly scoped to its recorded 6fb -> 22a pair. The landed2169 CI family record is1,321,515 /1,343,488 (21,973 headroom);
re-measure the next block on its actual source baseline, without relabelling
the earlier first-source pair.
This limited room is a measurement constraint, not permission to change budgets,
profiles or CPU bounds. Measure the exact first-source pair on the actual landing
before publication, and keep one local composed combined proof before READY.
Bybit/Bitget only if generic engine source is separately authorized; none is
planned. One local build at a time, jobs4, delete each own scratch target.

Plan/API review -> working native controls plus intended assertion REDs -> exact
source review -> execution/fixture/typed-failure/credit/size qualification ->
immutable preservation carry -> one source push/PR run -> full green CI family
record plus current local combined -> auditor exact-tree GO and guarded merge.

## Local originals checkpoint

On the actual25ca8466d63 landing, the new public_seed_context target has32 tests:
7 native working controls plus one exact-cleanup calibration pass and24 intended custody/policy assertions fail,
with0 ignored or filtered tests. Runtime, Cargo, inventories and every existing
fixture are unchanged. The explicit NEW20s deadline also excludes an older
funding response after the next60s cadence; that is an assertion RED, not a
native-parity positive. The seven positives pin financial/wire values, native
empty/partial catalogue behavior, saved-primary unsigned reads, failure
presentation, independent funding cadence and duplicate success.

The managed-release, typed precommit, expired-owned cleanup, sole-credit turnover,
full256-slot funding refusal/cadence and recovery suffixes remain honestly gated
on the legacy runtime. All infinity variants and the full type matrix must
execute after source adoption; this checkpoint does not claim they passed.
Exact original/source/fixture review remains before any production candidate.
The checkpoint is local and unpushed, with no new PR or CI dispatch.

The cleanup correction captures the physical timer and preparation from genuine
actions and permits each exact cleanup only once. Calibrators reject foreign,
duplicate and missing cleanup using a separately issued peer; that peer still
completes at finite time. Nonfinite Funding delivery also retains its genuinely
captured independent60s cadence and a finite successful next observation.

## Local source qualification

The local candidate implements one boxed physical context plus native staged
payload for all three seed routes. Corrected originals execute32/32PASS with no
ignored tests. Two native controls distinguish exact cancellation cleanup from
new work; nonfinite controls separately pin the inherited engine startup
watchdog notice before applying the strict physical cleanup ledger. Exact
source/control review is pending. The first full family run has11 fixture-target
failures because raw startup collectors cannot capture managed preparations;
these are qualification failures, not infrastructure. Narrow fixture artifacts
are under review before adoption. No full-family GREEN or publication claim.

The current tip74a0ba44d38 is plain-merged at8a73888af55; README is full incoming
plus the own row once. No incoming WOO source overlap. First-source measurements
must use this actual composition and retain their exact source/toolchain pins.
