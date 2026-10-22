# Engine read scheduler

## Scope and boundaries

Owner: codex-5c57jl. Base: clock #1709, actual merge
`6f815755c37de240d1a87bfec6edc1e4d4c8b614`.

Centralize read demand, coalescing, attempt progress, scoped pauses and startup
retry policy in one engine service. This is a cross-cutting engine change, not
a collection of venue fixes. Scheduling grants an attempt, never permission to
publish stale financial data or override authentication or accepted writes.

All deadlines use raw host scheduling time (`Cx.now`). Protocol clock calibration,
signing, request construction, mutation retries and socket supervision stay outside
this service. Keep endpoint retry safety, read joins, credential/symbol isolation,
snapshot revisions and degraded/recovered readiness policy explicit at callers.

## Foundation implementation boundary

The first implementation adds a bounded registry with explicit pause groups,
opaque attempt tokens and separate `Refresh`/`Invalidate` demand. `take_ready`
reserves work; the caller must successfully `mark_issued` immediately before I/O.
A late queued signer result cannot issue an expired or revoked reservation.
`finish` covers the complete logical read, not an individual response. `progress`
may renew its response budget only for validated matching page/join progress,
never for incoming WS updates or repeated demand. The caller drives deadlines
through existing Cx timer APIs; no dispatch hook is introduced by the foundation.

`startup::Backoff` delegates retry arithmetic/counters to the shared `RetryState`
while retaining dynamic keys and existing timer/notice ownership. Its trait bounds,
admission, ban scope, jitter draw order and public readiness behavior stay intact.
The general scheduler is not yet wired to ordinary venue reads; that migration
remains open and must retain their existing authority and continuation policies.

Independent correctness/adversarial review found two foundation edge cases:
non-representable positive leases could expire immediately, and checking an idle
key's lease could block a healthy peer. Strict future lease validation now applies
only to due, unpaused candidates before state/entropy changes. Response validation
also preserves the unsent reservation on error. A further adversarial check found
equal-time hot-key starvation; successful reservations now rotate behind peers
when due times tie. Four regression groups fail with the earlier implementations
and pass with those fixes. The foundation has fourteen state-machine regression
tests; existing startup/poll/financial controls remain.

## Committable phases

1. Audit current contracts across venues; commit this plan and its index row.
   Identify reusable scheduling state separately from financial authority.
2. Add a deterministic keyed scheduler in `engine/src/read_scheduler.rs`, with an
   additive module export and focused state-machine regressions. Separate ready
   admission from actual issue so asynchronous signing cannot spend a response
   lease while unsent. Coordinate any dispatch/request/signing changes with
   codex-uufub4; none are needed for this foundation.
3. Route the existing shared startup retry helper through the common policy where
   compatibility permits. Preserve timer identities, retry delays, jitter draw
   count/order, per-key reset and public degraded/recovered notices. Keep this
   adoption separate from later venue read-state migrations if their contracts
   require a different review boundary.
4. Adopt venue scheduling in scoped batches, preserving existing publication,
   auth, write and generation fences. Migrate duplicate policy into the service;
   do not add another per-venue scheduler. An unwired foundation does not complete
   all-venue standardization. Record remaining callers and exceptions here.
5. Independent correctness/adversarial and architecture reviews; focused native
   checks (four Cargo jobs), Binance regression checks and CI replay/size/timing
   gates. Integrate relevant landing changes, run a fresh separate revert guard,
   merge on green and report the actual merge pin.

## Required invariants and regressions

- Retain the first debounce deadline under sustained demand; coalesce in-flight
  demand into at most one follow-up without extending the current attempt lease.
- Match success, failure, expiry and cancellation to an opaque attempt token;
  old callbacks cannot release or reset newer work, including after key reuse.
- Start response leases only on actual issue; bound lost-read replacement without
  treating an unsent request as a completed network attempt.
- Gate first, dirty and retry admission by explicit budget/read-group pauses;
  extending a pause neither consumes another attempt nor draws more entropy.
- Preserve deterministic due-time/insertion ordering, independent keys and scoped
  pauses. Bound retained keys and provide explicit retirement/cancellation.
- Preserve retry caps/reset, logical timer fencing, Infinity pause behavior and
  startup readiness/repair distinction. Do not retry writes by inferring safety
  from their HTTP method.
- Prove attempt progress under slow or missing responses while retaining all
  existing financial snapshot, accepted-write and authentication rejection tests.

## Audit starting points

`engine/src/startup.rs` owns keyed exponential failure retries and shared pause
extension, but not fresh-read admission, dirty demand or response leases.
`engine/src/engine.rs` owns poll cadence and logical/physical timer fencing;
`engine/src/socket.rs` owns socket lifecycle. Keep those responsibilities intact.
`engine/src/fence.rs` controls financial admission and is not scheduling policy.

Representative caller constraints: Binance has separate depth/background pauses;
Orderly has independent regular/algo/position slots; Bitunix couples recovery to
auth and settings revisions; OKX joins risk and positions atomically; WEEX has
multi-read cycles; Phemex groups settlement currencies. Inventory and evidence
will be expanded before each migration, without claiming source-only suspicions
as reproduced production defects.

## Read-only inventory at the base pin

| Callers | Scheduling contracts to retain during adoption |
| --- | --- |
| Bybit | Per-coin joins and read generations; polls coalesce, explicit invalidation requests one follow-up; accepted leverage ACK advances the position fence. |
| Bitget | First repair timer, shared account REST pause, UTA dirty rerun; regular/protection/trailing walks retain one logical owner through all pages. |
| Bitunix | Ten-second ordinary-read replacement, matching serial completion, independent regular/TPSL slots; recovery admission precedes serial allocation. Settings/auth/write revisions remain separate. |
| BingX | Staggered polls and throttled repair; balance/mode bounded stale admission differs from stricter orders/positions. Command metadata remains independent. |
| Binance / Orderly | Separate Binance depth/background budgets; independent Orderly position/regular/algo reads and every-fourth-tick order poll. |
| OKX / WEEX / Phemex | Atomic risk/position joins; multi-read cycles and matching page-progress budgets; independent settlement-currency slices. |
| Aster / Kraken | Repair and snapshot revisions are distinct. Aster's rearmed WS timer merits a regression before alleging starvation; scheduling alone cannot authorize crossed-revision publication. |
| KuCoin / WOO | Busy-cycle/page ownership and settings fences; WOO regular/algo join and its deliberate REST-only hedged-position admission. |
| Bitrue / Polymarket | Symbol queues and independent retries; existing dirty follow-up survives periodic Infinity pause. |
| Hyperliquid / MEXC | DEX/weight isolation; staged order waves retain stronger full-repair intent. |
| Lighter / ApeX / BloFin | Serialized account or initial snapshot joins; paginated order families remain logical reads. |
| Coinbase / GRVT / Paradex | Product-specific slots and pause floors; session, builder and JWT prerequisites remain outside ordinary read scheduling. |
| Backpack / Crypto.com / TxFlow | Chained continuations, final-chain completion, independent history exhaustion and settings pacing. |
| BreakoutProp | Extract ordinary reads from its prerequisite queue without broadening trade retries. |

Socket reconnect/keepalive stays in `socket`; live periodic polling and Infinity
pause stay in the engine poll service. The scheduler does not bypass either.

## Venue-adoption execution plan after the clock core

Base: actual clock PR2 #1722
`d3e34e6e10523bd3bb465be362e4cc7fc75671b2` (scheduler foundation #1717
actually merged as `faf799268857f7aed35badba5349b572ef836c4a`).
The owner selected shared policy extensions first, followed by Bitunix balance
as the reference adopter and then scoped venue fan-out.

1. Extend only the shared scheduler and its state-machine tests now:
   - Declare separate refresh and invalidation debounce durations. Immediate
     periodic/start/reconnect demand must not inherit notification repair delay;
     repeated notifications retain the first pending deadline.
   - Declare whether an issued response lease expires into retry/backoff or an
     immediate replacement. Immediate replacement must neither sample entropy nor
     increment/reset failures; an unsent expired reservation remains distinct.
   - Add neutral superseded-read completion: release only a matching live issued
     attempt, retain existing retry state, and coalesce one repair at its retained
     first notification deadline (or a newly established notification deadline).
     Neither a stale callback nor an expired token may release a replacement.
   Keep existing policies and tests compatible, preserving startup retry behavior.
2. Regress immediate polls versus first notification debounce, sustained demand,
   lost-read replacement without entropy draws, neutral completion with/without
   dirty demand, retained next failure step, pause floors, stale callbacks and
   independent keys. Independent correctness/adversarial and architecture review,
   four-job engine/Binance native checks, fmt/Clippy and current-head CI precede a
   separate guard/merge. No venue/runtime adoption is claimed by this policy PR.
3. Bitunix ordinary balance is the first runtime adopter after the shared release
   seam lands. It has one request/response and fixed identity; retain serial and
   accepted-write/auth/publication fences, raw 500ms polling and first 1s repair,
   ten-second lost-read replacement, selected credentials and owned prerequisites.
   Replace duplicate coalescing/debounce/lease state with the shared service.
   Keep one shared private-GET pause authority during partial adoption; unmigrated
   axes must consult that authority rather than mirroring a second pause window.
4. Actual-release admission belongs to codex-uufub4's RequestContext follow-on
   immediately after signer prerequisites #1721. `Cx::request` only queues work;
   neither venue request creation nor signing preparation may mark a read issued.
   The common plain/signed HTTP release path supplies raw release time, rejects
   revoked/expired reservations before I/O, and anchors the response lease there.
   The signer receipt callback is metadata-only and is not this seam. Coordinate
   exact API/ownership before runtime edits; no per-venue callback/queue workaround.
5. Then migrate Bitunix positions and independent regular/TP-SL logical reads,
   bounded symbol-settings keys, and the audited venue inventory in scoped batches.
   Parallel legs/pages share one logical attempt; continuation admission must not
   call first-issue twice or renew a response lease without validated progress.
   Preserve order coverage, paired publication, per-symbol write/auth fences and
   each venue's current retry/readiness contracts. Signer-family migrations remain
   separately gated on actual #1721 and coordinate account-file ownership.

Audit evidence: Bitunix `balance_read_progress.rs`, `auth_results.rs`, and later
`order_read_scheduling.rs`/`position_read_progress.rs`/`settings_read_progress.rs`
already prove financial authority boundaries and must remain intact. Bybit's
per-coin joins and Hyro cumulative-fill paths stay outside the reference slice.
Architecture gate: this extends one engine scheduling policy and one shared
release boundary; adapters map identity and outcomes, not another scheduler.

RequestContext coordination refinement: add `Scheduler::admit(token, raw_now)`
in the policy phase. It returns a live raw lease deadline, starts the lease only
on the first actual release, and validates later legs/pages without renewal.
All new physical releases consult the same group pause; already-issued responses
may still make validated progress. Existing `mark_issued` keeps first-issue-only
semantics. The RequestContext owner caps this returned deadline by the original
request lifecycle and distinguishes admission cancellation from network/auth
failure. No dispatcher, request metadata or signer receipt code changes here.

Policy implementation evidence: separate retained notification intent now takes
its minimum with queued refresh intent before retry and pause floors. A review
regression reproduced a notification incorrectly waiting at 100ms behind a slow
refresh instead of its own 11ms deadline; the fix also retains that first deadline
across completion and raw-clock rollback and produces no extra covered read.
Neutral completion retains failure history; unjittered response expiry does not
increment or reset it. Idempotent admission gates every physical release without
renewing the lease; `mark_issued` remains first-only. Eleven additional regression
groups cover these policies and their pause/cancel/continuation interactions.
Focused engine/Binance verification passes 578 native tests, denied-warning
Clippy, formatting and bans; independent correctness/adversarial review is clear.
This remains pure shared policy: no venue, dispatch or startup adapter edits.
Fault-disabling refresh selection, retry floors, unjittered expiry, neutral failure
history and continuation lease stability produces the corresponding new test
failures (five controls, seven named regressions). The restored implementation
passes all 25 scheduler tests. The real first-notification regression also failed
before its fix; no existing financial or lifecycle assertions were weakened.
