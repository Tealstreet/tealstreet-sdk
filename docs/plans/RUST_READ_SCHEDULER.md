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
also preserves the unsent reservation on error. Three regression groups fail with
the earlier implementations and pass with those fixes. The foundation has thirteen
state-machine regression tests; existing startup/poll/financial controls remain.

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
