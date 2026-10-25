# Backpack account GET Read custody — scope proposal

Base: actual landing c6a5c4bb0a406fccd93db00e9d9db270fce1112b.

## Selected block and expected size

Adopt the four native account GET readers: account_info (/api/v1/account), positions_read (/api/v1/position), balance_read (/api/v1/capital/collateral), and orders_read (/api/v1/orders). Expected production delta about 300–450 lines plus dedicated controls and narrow fixture adaptations. Target one shared streamed-request inventory occurrence in account.rs (90455717d42b4ec702a3daffd7a277887a30622046104a208dc55b3e8202992c, count1; independently reproduced with the canonical scanner). The raw non-streamed write sender remains for native internal trading; no claim that this slice removes both account request entries. No generic engine change is planned.

## Preservation and custody requirements

- Preserve native startup gating, account-ready primary eligibility, GET-only secondary selection, primary receive-window inheritance, routes, headers, egress and parameter encoding. Use the real installed selected provider; no keyless signed context, fake identity or raw signing fallback.
- Preserve the existing Info/Positions/Balance/Orders sequence, independent reads, polling clocks, native revisions, SnapshotBuilder admission, OrderSnapshotToken custody, accepted row/version rules, live update precedence, repairs, readiness and backoff behavior.
- Each physical background read holds a genuine selected Read Context and exact pending ownership. Propose a new explicit 20s/Never per-observation bound, subject to exact native/API review; native later polling/recovery creates distinct observations. Record the bound in CALLERS.
- Validate the exact request Context and DATA before rows, status, codec diagnostics or Store publication; physical liveness is independent of native revision/token freshness. Publish successful native results under live DATA, then retire exact custody before successors/recovery. Failures retire exact custody before failure-only continuation; nonfinite callback time cannot schedule invalid work.
- Never grant a revoked, duplicate or foreign physical request another request's Context, snapshot token or credit. A still-live older request keeps its own native continuation even when its publication revision/token is stale. Retain native accepted-prefix rules and typed local/preparation/released failures; no fabricated DATA permission.
- Remove the now-unused streamed raw helper arm only when every production caller is proved migrated. If removing its stream flag requires the existing trading caller's constant false argument to disappear, disclose that mechanical hunk for the G2c owner; do not change trading decisions, recipes, reducers or writes.

## Plan/API review before originals or production

hcmq3f: native account lifecycle, snapshot/credit/continuation fit and exact selected-background Read API. m26ps4: real identity selection, readiness, inherited window and public-only behavior if a shared provider seam is implicated. No new runtime hook is assumed approved.

Original controls should cover four real managed preparations, held-release/equality/late hostile DATA, startup and periodic sequencing, genuine primary/secondary signatures, native stale-publication fences with preserved live-request continuations, exact failure cleanup and pressure/credit return. Existing native financial controls stay intact. Source-stage controls must exercise both streamed financial readers and retain exact accepted snapshot publication rules.

First source commit measures optimized Backpack plus combined against the same bare tip and pinned toolchain; no Bybit/Bitget unless generic engine scope is added. Scoped Cargo only, jobs4, one build at a time; one frozen push/PR run after source review gates.

## Overlap check

2026-10-25: 19 open PRs on feat/rust-rewrite inspected. No open PR touches venues/backpack/src/account.rs or lib.rs. #2093 touches sign.rs (avoid); #1961 touches trading.rs (avoid except separately reviewed mechanical argument removal, if necessary); #2033/#1990 touch shared Backpack support/managed fixtures and admission controls (notify owners before editing; prefer a dedicated test target). All20 local feat/fix/docs Rust Backpack branches are ancestors of actual c6a5c4bb0a4; no unpublished unique account/lib/support changes in those branches.

Receipts: /tmp/codex-backpack-next-open-pr-map.json and /tmp/codex-backpack-next-branch-overlap.json. No production edits, branch push or build performed for this proposal.

## Exact existing API direction for review

SelectedRead::select(Slots(native-ready primary, read_identity, SlotPolicy::default()), ReadRole::Read { scope: None }), then SelectedRead::background with ReadPolicy { timeout_ms: 20_000, pacing: None, retry: Never }. AdmittedRead::queue retains the same selection and uses the existing sign::Http recipe with the native primary window, ReadRequest stream None for Info/Balance and ArrayAt::Root for Positions/Orders. Pending must own the admitted handle and native request payload; no reconstructed Context or per-event reselection. validate_background_read_http_delivery must precede item/status/decode/publication. Exact API and missing/invalid selection behavior are review gates, not implementation clearance.

The native account_response currently calls positions_read/next_account/balance_read and may start repairs while still inside response processing. Source must separate successful Store publication from deferred successor work: publish with live DATA, retire the completing Context, then run the unchanged ordered successors, repairs or poll scheduling. Never retire before native financial publication merely to free a credit. A 255-genuine-peer control must fill all 256 credits with the predecessor, then prove its retirement supplies the sole successor credit; an orders snapshot token is never transplanted into another request.

Failure-only progression is distinct from DATA. Genuine native HTTP/status/parse failures retain their original next_account continuation and retried_failure/STREAM_MALFORMED presentation after exact physical retirement. Native local signed(...) configuration errors are silent and do not call account_failed or next_account; managed adoption must preserve that silence. A still-live older request may follow its own native next even if revision/token publication is rejected; only duplicate, revoked or foreign physical delivery is inert. Nonfinite event time forbids new observations and timers, independently of whether a native publication fence would have passed.

## Review disposition: physical custody and native publication fences

There is no latest-per-kind Context gate and no replacement-cycle suppression inferred from native revision changes. Each live Info/Positions/Balance/Orders request retains its own exact Context, selected binding and Next value. Existing native publication fences run only after DATA is admitted:

- Info with a stale leverage revision skips leverage publication but still starts Positions.
- Positions with a stale positions revision skips publication but still runs its original Next.
- Orders with a rejected Store token skips publication and repairs, but still runs its original Next. Never fabricate a token commit or treat the token as a Context credit.
- Balance preserves StaleGuard.admit_issued exactly: after two discarded observations, a sufficiently newer stale observation may publish; an observation older than the committed one may not. If accepted while stale, issue corrective Balance(recalculate, Next::None) before its original Next. Independent fill-triggered Position/Balance observations may coexist with polling/startup.
- Duplicate, revoked or foreign physical delivery produces no row/status diagnostics, publication, successor, repair or replacement-owner retirement. This physical denial does not turn a live native stale revision into a revoked Context.

Keep native positions_revision/balance_revision increments and invalidate_order_snapshots then begin_orders_snapshot in their original order relative to request construction, before any new admission refusal. Drop an unqueued Pending/token on refusal and retire only an actually admitted Context. No financial commit is invented to release physical credit.

## Failure and admission matrix before originals

| Cause | Native preservation or explicitly new disposition | Continuation and recovery |
| --- | --- | --- |
| Missing native-ready primary/read identity, native read configuration/seed/sign setup refusal | Preserve signed(...) Err being ignored: no HTTP, account_failed, next_account, synthetic notice, new timer or readiness effect. Validate with the same precedence and exact native messages/status. | Existing independently issued timers/reads remain untouched. Do not initiate a startup/poll chain from this refusal. |
| Genuine HTTP transport failure after release | Preserve retried_failure("backpack.account:613", native message) and the pending's original Next, with no payload adoption. | Retire exact physical custody before the original finite-time continuation. |
| HTTP non-2xx status | Admit DATA before status/body diagnostics, then preserve native account_failed HTTP text and original Next. | Retire exact physical custody before continuation. |
| Successful status but native parse/stream rejection | Admit DATA before diagnostics; preserve STREAM_MALFORMED text/level, original Next and the existing 10s poll fallback where account_response used it. | Publish nothing from unaccepted rows; retire before original ordered continuation/timer work. |
| NEW Capacity refusal during Context admission or queueing | No HTTP or fabricated DATA, no immediate next_account or same-observation re-admission; preserve already performed revision/token effects and drop pending/token. | At finite time coalesce one existing account.tick poll at the native 10s cadence if none is present; that timer starts a distinct Info observation. Never reset an existing poll deadline. |
| NEW nonfinite admission or callback clock | No new observation, timer or invalid timestamp; retire exact existing custody when present and discard inadmissible payload. | Leave existing independent finite timers untouched; do not create a recovery timer from NaN/Infinity. |
| NEW managed precommit/preparation failure or 20s expiry | Drop returned exact Pending/token and retire admitted Context; no payload, immediate successor, repair or immediate re-admission. Known native local configuration/selection errors remain in the first row and cannot be routed here to acquire recovery behavior. | For finite transient managed failure, coalesce one 10s account.tick as above. Nonfinite causes take the row above. This is new managed-failure recovery policy, not a claim about native HTTP failure behavior. Exact typed reason dispatch remains a source-review gate. |

The new recovery rows authorize only delayed distinct observations, not RetryPolicy changes or retry of the failed Context. Native HTTP/status/parse failures remain separately preserved; no broad failure handler may silently advance local configuration refusals. Pin notice text/level from actual native execution; do not infer extra kinds or outcomes. Exact typed classification of new precommit reasons must be reviewed before production, without message matching or a new engine seam.

## Required originals and working controls

- Four route preparations and real native-ready primary/secondary signatures, inherited receive window and startup eligibility; held release, 20s equality/late payload and row-diagnostic DATA boundaries.
- Native silent missing/configuration/seed cases: zero HTTP, successor, new timer and synthetic notice, retaining existing independent work and native revision/token issue ordering.
- Live stale Info and Positions skip publication but retain original continuations; rejected Orders token forbids financial publication/repairs yet preserves Next.
- Overlapping Balance controls reproduce two discards, newer-stale acceptance, corrective Balance(Next::None) before original Next, and rejection of an older-than-committed observation. Actual startup, periodic and fill-triggered reads coexist.
- Single-credit retirement proof: 255 genuine peer Background Contexts plus the completing account Context occupy all 256 credits. Successful publication occurs while DATA is live; retirement releases exactly the credit used by the successor, all peers remain usable, and eventual retirement restores all credits.
- Separate 254-peer Balance fanout proof: retirement frees a second slot before the corrective Balance then original Next preparations, preserving exact native ordering. This is not the single-credit proof.
- Genuine transport/status/parse failure text/level and continuation working controls; new Capacity, typed precommit and expiry delayed-recovery assertions, no immediate re-admission, no duplicate poll timer and nonfinite zero timer/new-work controls.
- Exact duplicate/revoked/foreign physical delivery is inert without suppressing still-live native stale continuations. Streamed malformed rows retain the native accepted/rejected snapshot policy; no partial financial publication or fabricated token completion.

## Review and execution sequence

1. Commit this plan and index row, then hcmq3f bounded native/API fit review.
2. Add original assertions plus working native controls on the unchanged runtime; run only the scoped new target with jobs4. Record exact assertions and gated suffixes honestly. Review originals before production.
3. Apply only cleared source shape, preserve all existing native assertions, qualify scoped crate/strict/format/inventories, and request immutable source/fixture/map review. Any shared fixture edit gets its owner's notice first; sign.rs and generic engine remain outside scope.
4. At the first source commit measure same-toolchain bare/composed Backpack and combined. Push one frozen draft after gates, keep one PR Rust run, then READY to8x5tm1 on full green and source CLEAR. Exact-tree GO remains mandatory.

## Explicit limits

This is not a private WebSocket or signer cutover, G2c provenance implementation, account economic redesign, new serial command root or permission to add generic engine hooks. No production source, inventory removal, original RED, build or CI proof exists at this plan commit. The 20s bound is proposed new policy, not a claim that native already expires those reads. Scanner-prune count may only decrease when the exact shared streamed helper site is actually gone; helper consumers in test fixtures must be inventoried before removal.

## Originals execution on unchanged runtime

The dedicated account_read_context target executes 36 tests: 10 native working controls GREEN, 26 intended assertion REDs, zero ignored. Rust 1.99.0; scoped Cargo test only, CARGO_BUILD_JOBS=4 and test-threads=1. Runtime, Cargo, inventories and all existing fixture files remain byte-exact to actual c6a5. No production or prune has been applied.

The GREEN controls pin both installed slot configurations' native four-route signatures/window/wire and chain, missing/public-only private startup, exact malformed/31-byte read startup CONFIG_INVALID, native transport/status/parse text/code/Error level and Next, live stale Info/Positions continuations, rejected Orders token suppression with Finish, Balance two-discard acceptance/corrective-before-Next, older-than-committed rejection despite two further discards, and inert duplicate response at a fixed snapshot comparison clock.

The REDs cover four Prepare boundaries, four held-DATA publications, four NEW20s equality publications, four nonfinite publications, four selected-provider signatures against different valid raw material, held streamed row diagnostics, the 255-peer sole-credit proof, separate254-peer Balance fanout, full Capacity at a genuinely fired poll, managed precommit recovery and engine-owned expiry cleanup. The Capacity fixture reserves all256 genuine peer Contexts before the existing poll timer invokes account_info; it does not assume derived public seeds run before account_start.

Evidence limits remain explicit. The legacy precommit control fails at its managed-Prepare assertion, so typed failure dispatch and duplicate recovery suffixes remain source-stage checks. Expiry cleanup suffixes follow the first failed publication assertion; the peer survival/full-reuse assertions follow the first failed physical-credit assertion. The original nonfinite loop stops at its first NaN assertion on legacy, so +/-Infinity suffixes still require source execution. Native bad-read tests pin the actual constructor/startup gate; silent signed-helper Err return preservation still requires source inspection, since immutable admitted setup provides no ordinary runtime mutation to manufacture that state. Each signature control independently reaches its own route and contrasts the real installed secondary with different valid raw read material and status Some(Ok(())).

The test audit wrapper delegates native managed mutation reconciliation/ready/retirement hooks for the genuine leverage-write control; it adds only test-owned peer reservation/probing and one separately identified cleanup timer. Shared support helpers and financial assertions were not edited. Initial fixture assumptions were corrected before this frozen original pin: public-only loaded flags, streamed failure presentation, mutation wrapper delegation, fixed snapshot comparison time, actual stale-Balance ordering and full-capacity entry. Only the qualified 10GREEN/26assertRED log is evidence for this pin.
