# Rust landing → master: remaining blockers

Sam’s decision checklist, re-verified at landing `74c45b4f162` (2026-10-22).
This lists outstanding cutover work, not authority to deploy, capture credentials
or trade. SDK fixes/green CI do not establish NX adoption or live evidence.
[Historical consumer conformance](RUST_CUTOVER_READINESS.md) stays separate;
[the caller inventory](RUST_CUTOVER_CALLERS.md) owns the detailed migration list.

## 1. Owner decisions and required sign-off

| Item | Cutover disposition / what remains |
| --- | --- |
| **B1 — engine delivery** | Choose a distinct mirrored engine path plus NX CI build, or a prebuilt package, and prove delivery to each chosen host. NX’s unrelated `sdk-rs/` and gitignored `pkg/` do not deliver the engine; mobile follows B3. [Delivery contract](RUST_CUTOVER_CALLERS.md#build-and-delivery-steps). |
| **B2 — keyless signing** | Short-term policy is **already decided: preserve browser remote signing**. Retain the TS hosted-signing delegates, including Coinbase/Orderly. The later engine port/session-key direction is undecided; preserving today’s hosted mode does not require that port. [Decision and costs](RUST_REMOTE_SIGNING.md). |
| **B3 — React Native/Hermes** | Choose/verify retained device TS, hosted execution or native/JSI integration. Hermes cannot execute WASM; the loader hook is not a shipped host. Device TS remains a recommendation. [Options and app boundaries](RUST_MOBILE_B3.md). |
| **Rollback — #1699** | Approve resolver/policy, TS reachability, observation windows and tested recovery. No forced-TS switch exists; unsetting Rust restores defaults, and TxFlow has no production TS fallback. Retain TS/helpers/signing delegates/executable references through cutover. [Recommendation, costs and soak plan](RUST_ROLLBACK.md). |
| **Bybit split preflight** | Accept whole-batch rejection before writes when one split lot cannot format, or authorize another policy; TS skips that order. Partial TP/SL semantics are already decided. [Bybit retained findings](RUST_AUDITS.md#bybit-family). |
| **Coinbase CFM accounting** | Verify CFM/spot accounting and downstream equity consumers before choosing summary PnL/equity treatment. Native reports PnL while TS uses zero; hiding or double-counting it is unacceptable. [Accounting decision](RUST_AUDITS.md#coinbase-already-defaults-to-rust). |
| **Request lifecycle legacy bridge — hard gate** | The staged bridge is authorized only during family migration. Its exact legacy call-site allowlist may only shrink; new legacy callers fail CI. Before landing → master, finish fan-out and delete both the empty allowlist and bridge. Legacy work has no inferred semantics, generic replay or outcome-sealing claim. [Lifecycle and removal contract](RUST_REQUEST_CONTEXT.md). |

## 2. Required NX caller changes and integration proof

Read-only caller audit is pinned to NX `65e086a3c90`; none of these is a claim
about a deployed build. Upgrade matching SDK/engine/protocol consumers together.

| Item | Required cutover result |
| --- | --- |
| **Startup degraded UI / coded health** | Forward `engine-event`; show per-exchange degraded state despite toast suppression, clear on recovery, retain Info delivery and metadata-based deduplication. Keep terminal failures separate. Attach CLI listeners **before** startup; hosted stdout is insufficient. Adopt classified errors/code-based health. [Startup requirements](RUST_CUTOVER_CALLERS.md#required-startup-degradation-visibility), [coded diagnostics](RUST_CUTOVER_CALLERS.md#coded-diagnostic-caller-cutover-pr5). |
| **Empty-book repair — high priority** | Propagate invalidation through worker/hosted listeners and smoothing; reset interpolation/depth and clear rendering. Prove disconnect/backoff/reopen and one-sided recovery, smoothing on/off, without stale opposite levels. [Required Bitunix caller repair](RUST_CUTOVER_CALLERS.md#bitunix-disconnected-books-1639). |
| **GRVT builder handler** | Use validated `builderAuthorizationOf(error)`, preserving reason/caps, modal/reload, cleanup/rethrow and outcome. Upgrade SDK sender/receiver together; prove both reasons and malformed/duplicate controls through actual hosting. [Required migration](RUST_CUTOVER_CALLERS.md#grvt-builder-authorization-failures-required-caller-migration). |
| **B11 history coverage** | Read request-owned completeness **before** mapping; retain partial/flagged-empty outcomes and OR across derived axes. Partial rows may persist under the user guard, never `completeWindow`. Prove concurrent worker/hosted isolation with matching SDK sender/receiver; no coverage-only records schema change. [Transport and ingestion contract](RUST_CUTOVER_CALLERS.md#incomplete-history-1542-1554). |
| **Capability contract / call order** | Migrate removed brand helpers and `COMMAND_TIMEOUT` matching; **require** typed pre-start/failed-start/terminal handling, `errorReported` and unsent outcomes. SDK waits pending startup; explicit worker-aware readiness is **recommended**. Cached markets are insufficient. Skip Rust eager warm-start; never create a second mirror engine or auto-replay writes. [Capability callers](RUST_CUTOVER_CALLERS.md#capability-record-rust_quality-item-8-1400-1411), [readiness](RUST_CUTOVER_CALLERS.md#runtime-capability-readiness-1637), [warm start](RUST_CUTOVER_CALLERS.md#warm-start-skips-rust-accounts-b4). |
| **Entropy runtime proof** | Prove secure defaults in shipped dedicated/window-fallback workers, Bun private **and public** engines/restarts and compiled Bun CLI. No redundant constructor arguments; providers stay local, outside JSON/messages. SDK tests are insufficient. [Host/realm checklist](RUST_CUTOVER_CALLERS.md#host-entropy-and-nx-construction-1624). |

Sam owns deployment: NX exchange-server before web; separately deploy the
matching **safe-cex-server** event validator before native/DO Info+ event
consumers. Pause BloFin ingestion for the owner-run historical-order unit
backfill before Rust writes (or use idempotent conversion); never convert twice. [Cutover edits and operational prerequisites](RUST_CUTOVER_CALLERS.md#what-remains-before-cutover-beyond-owner-decisions-b1b3).

## 3. Live-capture and evidence gaps

- Obtain approved public/private captures, credentialed lifecycle smoke and
  read-only shadow/canary/rollback evidence for enabled venue/product/account
  shapes. Scripted tooling is not live proof; unknown writes require reconciliation.
  [Live gate](RUST_LIVE_GATE.md), [smoke](RUST_LIVE_SMOKE.md), [rollback](RUST_ROLLBACK.md).
- Re-record Rust-driven public inputs through TS; retain executable TS oracles.
  Repeatability and accepted **FAIL** declarations are not parity. Missing native
  IDs/timestamps/financial fields remain unknown. [Evidence limits](RUST_ARCHITECTURE_GATE_AUDIT.md#gate-honesty-evidence),
  [raw-proof receipts](HARNESS_TAPE_TIME.md).
- Resolve or explicitly accept account-dependent limits, including Kraken ledger
  paging beyond 500 rows, through owner-run read-only evidence.
  [Venue probe](RUST_LIVE_GATE.md#3a-owner-run-venue-probes).

## 4. Live-master hot patches pending Sam

These also affect today’s TS/mobile users; a landing merge is not a production patch.

| Item | Pending action / source |
| --- | --- |
| **OKX `:8443`, before 2026-10-31** | Patch live TS URLs and deployed global/EEA proxy/Caddy upstreams plus relay allowlists to 443. Rust #1588 is already landed; deployed upstreams are separate. [Deadline and ops matrix](RUST_CUTOVER_CALLERS.md#okx-websocket-port-operations-prerequisite-1588). |
| **TS Lighter client-order-index collision** | Patch the production 100-suffix-per-millisecond generator; tape-only deterministic replay entropy does not fix live TS. Keep duplicate-ID verification. [Reproduced collision and boundary](HARNESS_TAPE_TIME.md#lighter-phase). |
| **TS Hyperliquid fee subject** | Use account/vault instead of agent. Native #1701 is fixed; live TS needs Sam’s patch/rollout. [Fee identity contract and TS exclusion](RUST_HYPERLIQUID_FEE_SUBJECT.md). |
| **Mobile Hermes transport / Lighter / AsyncStorage** | Confirm shipped builds; register transport, route/support/hide Lighter writes, and move API/sync secrets to secure storage. B3 does not fix these existing app issues. [Live production inventory](RUST_MOBILE_B3.md#live-production-issues-found). |

Merged SDK method gaps, socket-health recovery and browser observations are not
new owner decisions. Browser #1703 adds warnings only; no hard timing gate is
approved. [Caller status](RUST_CUTOVER_CALLERS.md#summary-cutover-blockers-no-new-equivalent),
[browser measurement/threshold boundaries](RUST_BROWSER_COLD_START.md).
