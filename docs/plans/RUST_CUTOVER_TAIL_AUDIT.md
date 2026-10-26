# Re-verify the Rust cutover audit tail

Base: `bba1f43126cd` (landing after SDK B11 #1666). Re-check the eight
Cutover bullets in RUST_AUDITS.md against current source and existing tests.
No TypeScript venue deletion, NX writes, deployment or release is authorized.

1. Commit this short plan first. Audit socket health/recovery and CLI stale-stream
   refusal; venue methods and capability refusals; downstream BloFin identity
   checks and deletion deep imports; MCP packaged engine delivery; WASM budgets,
   layout and cold-start evidence; replay golden-fixture prerequisites.
2. Attach exact source, regression and actual merged-PR evidence to each stale
   finding. Preserve narrower unsupported-product, delivery and downstream
   requirements instead of inferring complete cutover readiness.
3. Request a read-only NX re-check from codex-cxxyqr for caller-owned gaps.
   List owner decisions separately for claude-ddi2il. Keep the audit evidence
   distinct from deployed-runtime or npm-publication verification.
4. Land one docs-only PR striking stale Cutover findings and listing the exact
   remaining SDK/NX/owner work. Independently review correctness/adversarial
   claims, run focused documentation checks, require green CI and a fresh
   separate guard, then merge and report the actual pin.
5. Fix any remaining SDK-owned defects in separate behaviour batches on fresh
   landing branches, with regressions reproducing the failures before each fix.
   Use focused local checks; CI owns release WASM builds and replay/timing
   matrices. Preserve lane priorities: shared source merges hold for Kraken
   #1663's current proof cycle. Apply normal integration/trial and guard rules.
6. Report every merge to claude-ddi2il and precise caller follow-ups to
   codex-cxxyqr. Stop when the assigned SDK scope and evidence refresh land;
   owner decisions and NX implementation remain explicit handoffs.

## Re-verification at `bba1f43126cd`

This is source and regression evidence on the landing branch, not an npm
publication, browser timing or deployed NX runtime claim. NX source observations
are read-only at `65e086a3c90` and supplied by codex-cxxyqr.

| Original Cutover finding | Current evidence | Disposition / owner |
| --- | --- | --- |
| Main-thread writes have no engine | `RustEngineExchange.forwardToWorker`, `WORKER_FORWARDED_METHODS` and the common matching worker receiver route writes to the engine owner. #1443 / `6d6e3d666f`, #1462 / `341ea148bb`; retained worker-contract regressions and green #1666 CI. | Already closed; preserve forwarding before local runtime policy. |
| Missing socket health/reconnect/offline recovery; CLI stale-stream fails open | `rustSockets.ts` exposes channel handles, 60 s freshness, reconnect and offline recovery. `syncSocketHealthToStore` writes the shared summary. `BaseCommand.healthGate` refuses exposure-increasing writes after an initialized private stream becomes unhealthy; reduce-only/close/cancel remain available. #1450 / `5ff7ebde6c1`. Fresh tests: 7 SDK socket cases and 3 real-bridge CLI cases pass. | Stale SDK finding. Missing/uninitialized evidence still intentionally fails open; a quiet initialized stream does not. Recovery does not bypass venue backoff or revive disposed/dead engines. |
| All listed venue methods absent | Native capability policies route journals, reported PnL and Bitrue metadata; Hyperliquid merges spot markets when enabled. #1445 / `fc771ab13d3`, #1447 / `8f29096de9c`, #1453 / `fe43821c3e8`, #1501 / `550e534e08f`. Existing `rust.journal-paths`, `rust.closed-pnl`, `rust.binance-journal`, `bitrue.rust-max-leverage` and Hyperliquid `spot_markets.rs` cover those paths. | Partly stale. Actual SDK gaps are listed below; a registered method or streaming book does not prove a REST query. Unsupported product/history axes retain explicit capability refusals. |
| Downstream BloFin `instanceof` blocks cutover | The named helper still imports/checks the TS class, but no production import/render of that helper was found in the pinned NX search. `ExchangesContainer.tsx:59,223` uses the generic helper, which has no class guard; `onPositionMetaChange.ts:34-35` returns for BloFin. | Unused-source/deletion cleanup, not a demonstrated active runtime blocker. Preserve that evidence boundary; do not delete the file/class here. |
| Deletion inventory misses listed deep imports | The SDK inventory already resolves base/registry/class and frozen-reference graphs. NX still imports Hyperliquid constants, Bitget family helpers and Aster types/validators; current caller requirements are recorded in RUST_CUTOVER_CALLERS.md. No production NX direct Bybit inverse/spot class import outside SDK runtime was found; internal registries and AST fixture scripts still depend on them. | The updated inventory now includes these dependencies; extraction/caller migration remains open. No source deletion is authorized. |
| MCP release omits engine WASM | `package.json` includes `dist/wasm/rust`; `tsup.config.ts` copies CJS glue and WASM; `rust-engine.ts` registers an artifact-relative loader. The release workflow builds the pinned engine, runs isolated smoke and requires both files in the exact npm pack list. #1342 consumer-blocker implementation; green #1666 Binance consumer/package checks exercise current source. | Stale SDK packaging finding. This does not assert that a particular npm version or NX installation already contains this branch. |
| No WASM budget/layout decision or browser cold-start measurement | `wasm-budget.mjs` fixes family 1,343,488 B and combined 14,155,776 B ceilings; CI checks family isolation/size and combined size before standalone overwrite. #1393 / `cb371229da1` chooses browser/Workers family loading and Node/MCP combined; #1644/#1656/#1661 recover size without raising budgets. Fresh 2 budget regressions pass. | Budget/layout portions stale. `family-measure.mjs` measures fresh **Node** read/compile CPU/wall/RSS; no actual browser cold-start timing was found. Browser measurement remains SDK evidence work. Historical artifact sizes are pinned in RUST_WASM_ARTIFACT_SIZE.md, not asserted as current bytes here. |
| Freeze TS outputs, then remove executable references | `venues.mjs` still selects executable TS implementations, including retained Coinbase/Orderly/TxFlow test references, and `replay.mjs` imports them. Static output goldens do not measure TS CPU or retained memory. | Owner decision from claude-ddi2il: keep executable TS references as the CPU/memory oracle through cutover. **No venue or reference deletion until after cutover.** Freezing/retiring oracles is a later reviewed prerequisite, not a current deletion instruction. |

## Remaining SDK behaviour batches

Pinned NX reachability finds no direct production/default REST snapshot or
dead-man-switch producer. SDK CLI/MCP expose neither method; script API books
read a subscription cache. WEEX TS internal snapshot/gap reads and Paradex
optional capped IOC preflight are distinct native-stream/trading contracts.
Manual/third-party API parity remains required, without a claim that every
method is a default cutover feature. Per overseer ordering, WEEX metadata goes
first, then GRVT typed builder errors, then public/manual book and dead-man
parity; browser measurement remains open. Each source batch needs its own
fresh landing branch, before/after regression, focused checks, CI and review.

1. **WEEX selected-symbol metadata: repaired in the follow-up.** Native WEEX
   now reports `onReadAndMetadata`; the shared SDK path reads signed symbol
   configuration for cached/flat symbols while explicit reads stay fresh. The
   [before-fix native regression and validation](RUST_WEEX_METADATA_REFRESH.md)
   distinguish malformed failure/retention and public-only isolation. No new
   polling loop or venue-name fallback. The original table above records the
   audit baseline before this repair.
2. ~~**GRVT builder-specific rejection:**~~ repaired with a boxed typed native
   record and validated flat `GRVT_BUILDER_UNAUTHORIZED` code,
   `unauthorized`/`cap-too-low` reason, `requiredRaw` and optional `authorizedRaw`.
   Planning/amend inspection and SDK worker/hosted replies retain these fields.
   Unknown, transport and malformed checks remain ordinary errors. Native no-write,
   real-host and sender/receiver clone/JSON regressions cover the boundaries;
   [plan and evidence](RUST_GRVT_BUILDER_ERROR.md). NX's live class-only handler
   still requires migration to exported `builderAuthorizationOf(error)`; SDK
   source repair does not migrate or deploy that caller.
3. ~~**REST order-book queries:**~~ Backpack now joins AsterDEX, MEXC and Polymarket
   with a native unsigned query and capability. Its request-owned snapshot keeps
   raw price IDs/order/zero totals and never mutates subscription state; malformed
   data/refusals fail. A precise shared decoder feature admits standalone JSON
   requests without unrelated venue commands. Crypto.com now uses unsigned live/UAT
   GET with fixed depth50, typed first-book mapping and independent query ownership;
   native price IDs/order/zero totals survive without stream mutation. GRVT now
   issues independent unsigned MDG lite queries using the documented i/d fields
   and full/lite response aliases; native side order, zero rows and cumulative
   base totals are preserved without stream/session mutation. Paradex now uses
   unsigned request-owned depth queries (default20/explicit depth), native row
   order and per-level quote totals, independently of live SBE sequence state
   and private JWT. WEEX now issues independent unsigned queries with cumulative
   native base totals, exact version strings and shared rate-pause admission,
   without mutating live depth/account state; streaming subscriptions are separate. MEXC's existing
   standalone JSON decoder omission is repaired by opting into the precise book
   feature, with a JSON-to-native-HTTP/result regression; native book semantics
   stay unchanged. REST queries never synthesize snapshots from absent streams.
4. ~~**Hyperliquid dead-man switch:**~~ Rust perpetuals now route
   `scheduleCancel(time?: number | null)` through a precise standalone decoder
   and append-only capability. Native safe UTC-ms deadlines are checked at
   admission and signing, with vault/network signing vectors independently pinned
   against TS. SDK and direct-host validation prevents nonfinite JSON values from
   clearing the switch. Existing write/worker ownership preserves uncertain
   outcomes; no automatic retry, rescheduling or lifecycle arming/clear.
   [Implementation and evidence](RUST_HYPERLIQUID_SCHEDULE_CANCEL.md).
5. **Browser cold-start evidence:** add actual browser module-load/compile/
   instantiation observations for shipped artifacts. Existing Node metrics and
   public replay timing do not supply this proof. CI owns artifact builds and
   timing; do not run a local release matrix or infer device/network performance
   from a headless CI sample.

## Caller work and owner decisions

NX-owned: preserve/extract the concrete runtime deep imports before deleting
modules; update capability-brand readers and script probes deliberately; handle
request-scoped history completeness at ingestion/coverage (B11). The unused
BloFin helper needs cleanup before its class can disappear, rather than an
unproved production migration. SDK source fixes do not modify or deploy NX.

Owner-decided: browser/Workers family loading versus combined Node/MCP is
already implemented; executable TS performance oracles stay through cutover and
no venue/reference is deleted before then. Engine delivery into NX (B1),
hosted signer strategy and mobile execution remain separate recorded owner
items; this Cutover-tail audit does not settle them or authorize a release.
