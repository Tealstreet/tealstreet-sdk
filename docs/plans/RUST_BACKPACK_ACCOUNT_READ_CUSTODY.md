# Backpack account GET Read custody — scope proposal

Base: actual landing c6a5c4bb0a406fccd93db00e9d9db270fce1112b.

## Selected block and expected size

Adopt the four native account GET readers: account_info (/api/v1/account), positions_read (/api/v1/position), balance_read (/api/v1/capital/collateral), and orders_read (/api/v1/orders). Expected production delta about 300–450 lines plus dedicated controls and narrow fixture adaptations. Target one shared streamed-request inventory occurrence in account.rs (90455717d42b4ec702a3daffd7a277887a30622046104a208dc55b3e8202992c, count1; independently reproduced with the canonical scanner). The raw non-streamed write sender remains for native internal trading; no claim that this slice removes both account request entries. No generic engine change is planned.

## Preservation and custody requirements

- Preserve native startup gating, account-ready primary eligibility, GET-only secondary selection, primary receive-window inheritance, routes, headers, egress and parameter encoding. Use the real installed selected provider; no keyless signed context, fake identity or raw signing fallback.
- Preserve the existing Info/Positions/Balance/Orders sequence, independent reads, polling clocks, native revisions, SnapshotBuilder admission, OrderSnapshotToken custody, accepted row/version rules, live update precedence, repairs, readiness and backoff behavior.
- Each physical background read holds a genuine selected Read Context and exact pending ownership. Propose a new explicit 20s/Never per-observation bound, subject to exact native/API review; native later polling/recovery creates distinct observations. Record the bound in CALLERS.
- Validate current Context and DATA before rows, status, codec diagnostics or Store publication. Publish successful native results under live DATA, then retire exact custody before successors/recovery. Failures retire exact custody before failure-only continuation; nonfinite callback time cannot schedule invalid work.
- Never grant a stale request a replacement owner's Context, snapshot token, credit or recovery authority. Retain native accepted-prefix rules and typed local/preparation/released failures; no fabricated DATA permission.
- Remove the now-unused streamed raw helper arm only when every production caller is proved migrated. If removing its stream flag requires the existing trading caller's constant false argument to disappear, disclose that mechanical hunk for the G2c owner; do not change trading decisions, recipes, reducers or writes.

## Plan/API review before originals or production

hcmq3f: native account lifecycle, snapshot/credit/continuation fit and exact selected-background Read API. m26ps4: real identity selection, readiness, inherited window and public-only behavior if a shared provider seam is implicated. No new runtime hook is assumed approved.

Original controls should cover four real managed preparations, held-release/equality/late hostile DATA, startup and periodic sequencing, genuine primary/secondary signatures, stale revision/token refusal, exact failure cleanup and pressure/credit return. Existing native financial controls stay intact. Source-stage controls must exercise both streamed financial readers and retain exact accepted snapshot publication rules.

First source commit measures optimized Backpack plus combined against the same bare tip and pinned toolchain; no Bybit/Bitget unless generic engine scope is added. Scoped Cargo only, jobs4, one build at a time; one frozen push/PR run after source review gates.

## Overlap check

2026-10-25: 19 open PRs on feat/rust-rewrite inspected. No open PR touches venues/backpack/src/account.rs or lib.rs. #2093 touches sign.rs (avoid); #1961 touches trading.rs (avoid except separately reviewed mechanical argument removal, if necessary); #2033/#1990 touch shared Backpack support/managed fixtures and admission controls (notify owners before editing; prefer a dedicated test target). All20 local feat/fix/docs Rust Backpack branches are ancestors of actual c6a5c4bb0a4; no unpublished unique account/lib/support changes in those branches.

Receipts: /tmp/codex-backpack-next-open-pr-map.json and /tmp/codex-backpack-next-branch-overlap.json. No production edits, branch push or build performed for this proposal.

## Exact existing API direction for review

SelectedRead::select(Slots(native-ready primary, read_identity, SlotPolicy::default()), ReadRole::Read { scope: None }), then SelectedRead::background with ReadPolicy { timeout_ms: 20_000, pacing: None, retry: Never }. AdmittedRead::queue retains the same selection and uses the existing sign::Http recipe with the native primary window, ReadRequest stream None for Info/Balance and ArrayAt::Root for Positions/Orders. Pending must own the admitted handle and native request payload; no reconstructed Context or per-event reselection. validate_background_read_http_delivery must precede item/status/decode/publication. Exact API and missing/invalid selection behavior are review gates, not implementation clearance.

The native account_response currently calls positions_read/next_account/balance_read and may start repairs while still inside response processing. Source must separate successful Store publication from deferred successor work: publish with live DATA, retire the completing Context, then run the unchanged ordered successors, repairs or poll scheduling. Never retire before native financial publication merely to free a credit. A 254-peer pressure control must prove the chain admits successors after retirement; an orders snapshot token is never transplanted into another request.

Failure-only progression is distinct from DATA. The exact Pending and its native continuation may advance the native account cycle after a genuine typed preparation, transport, expiry or released HTTP failure, without adopting payload or granting success authority. A stale/replaced request must not advance a replacement cycle. Pin exact native failure notice text/level and continuation behavior before changing it; admission failure behavior and token cleanup need hcmq3f disposition. Nonfinite clocks may not mint further observations or timers.

## Review and execution sequence

1. Commit this plan and index row, then hcmq3f bounded native/API fit review.
2. Add original assertions plus working native controls on the unchanged runtime; run only the scoped new target with jobs4. Record exact assertions and gated suffixes honestly. Review originals before production.
3. Apply only cleared source shape, preserve all existing native assertions, qualify scoped crate/strict/format/inventories, and request immutable source/fixture/map review. Any shared fixture edit gets its owner's notice first; sign.rs and generic engine remain outside scope.
4. At the first source commit measure same-toolchain bare/composed Backpack and combined. Push one frozen draft after gates, keep one PR Rust run, then READY to8x5tm1 on full green and source CLEAR. Exact-tree GO remains mandatory.

## Explicit limits

This is not a private WebSocket or signer cutover, G2c provenance implementation, account economic redesign, new serial command root or permission to add generic engine hooks. No production source, inventory removal, original RED, build or CI proof exists at this plan commit. The 20s bound is proposed new policy, not a claim that native already expires those reads. Scanner-prune count may only decrease when the exact shared streamed helper site is actually gone; helper consumers in test fixtures must be inventoried before removal.
