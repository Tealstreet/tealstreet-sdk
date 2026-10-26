# Engine rollback through cutover

Sam's current rollout mechanism is the [per-user hard-cut bundle switch](RUST_BUNDLE_ROLLOUT.md):
frozen TS app/server versus new Rust app/server for every held venue. Its cohort,
shared-state and rollback rules supersede older per-account/default-switch
recommendations below; the evidence and reconciliation requirements still apply.
This is preparation only, with no deployment or flag change.

Status: recommendation for owner decision 4; no selector, flag, default or runtime
behavior is implemented by this document. No deployment, live shadow, trading
or rollback exercise was performed. Retain executable TS implementations and
performance references through cutover; deletion is a separate later decision.

Companion contracts: [cutover callers](RUST_CUTOVER_CALLERS.md),
[remote signing](RUST_REMOTE_SIGNING.md), [mobile](RUST_MOBILE_B3.md),
[live evidence](RUST_LIVE_GATE.md) and [cutover readiness](RUST_CUTOVER_READINESS.md).

Mirror source: landing `fa960d9cfcf` (including actual #1696, #1697, #1690 and #1693).
SDK source references below are relative to `packages/safe-cex/` unless stated.

## Current construction contract

The mirror SDK accepts only `ExchangeOptions.engine?: 'rust'`
(`packages/safe-cex/src/types.ts:138-139`). Both `createExchange`
(`src/index.ts:83-99`) and `ExchangeWorker.start`
(`src/webworker/ExchangeWorker.ts:345-349`) honor it through `runsOnRustEngine`
(`src/exchanges/rust/rust.exchange.ts:200-213`). The shared account mapper spreads
explicit overrides last (`src/createExchangeFromAccount.ts`), but does not map
an engine preference from persisted account fields. These are construction
choices; changing an options object on a running exchange does not switch it.

Unsetting `engine: 'rust'` returns to the registry default. It does not force TS.
The browser and native registries default Coinbase/spot, six Orderly broker
names and TxFlow to Rust. Coinbase and Orderly still have production TS classes
and `hostedSigningVenues` loaders for keyless remote signing; that compatibility
path is not a general ordinary-account rollback selector. TxFlow has no
production TS fallback in the current registry or hosted loader map. Its frozen
test reference is not a shippable fallback. A future explicit TS request must
report unavailable for TxFlow until a separate implementation is restored and
proved. Existing signing delegates must remain reachable independently.

MCP `packages/exchange-mcp/src/config.ts:38-43` accepts `EXCHANGE_ENGINE=rust` or unset; the string
`typescript` is rejected. CLI constructors use the shared account mapper.
No current SDK or MCP forced-TS switch should be advertised as operational.

## Options and recommendation

| Placement | What it can control | Cost and risk |
| --- | --- | --- |
| Shared SDK factory/resolver | Explicit `typescript` or `rust`, registry defaults, implementation availability and signing compatibility across factory and worker. | Required common primitive; must preserve aliases/products, native loaders, signing delegates, class caching and default behavior. Both implementations stay buildable and tested. It cannot distribute an operator policy to NX by itself. |
| NX account setting or per-venue rollout policy | Which browser/worker/CLI sessions request an engine; a deliberate UI or operator override can expose effective selection. | Needs schema/editor/sync decisions, shared account mapping and init propagation. A web-only setting cannot govern the hosted server. A synchronized preference and a device-local override have different semantics. |
| Hosted server per-venue policy/flag | Cohort/venue selection for server-owned sessions and shared public feeds, with an operator rollback lever. | Requires a new validated policy consumer and effective-engine response; none exists today. Public creation/restart and private creation must agree. It cannot change local browser/CLI sessions. Per-account mixed-engine canaries complicate venue-keyed shared feeds. |

Recommend one SDK construction resolver, with caller-owned rollout policy
feeding it, rather than three independent implementations of engine selection.
The eventual SDK contract should accept `typescript | rust`, retain current
registry defaults when no policy applies, validate implementation/product/host
availability and report requested engine, effective engine and any signing
compatibility reason. This is a proposed contract, not a field available today.
A valid selection must not silently reuse a class cached for another effective
engine. Unsupported selections must produce a typed, user-readable availability
error before startup or any write.

Start hosted rollout with one approved engine policy per current venue key
in a server process, matching its public-source ownership. Distinct product
policies under that same key, or per-account mixed-engine cohorts, require a
qualified public-feed identity or proved compatibility first. Preserve keyless remote-signing and payload
signer behavior; applying a blanket Rust policy cannot remove required TS
signing delegates. Keep the decision and effective selection explicit across
both the mirror and engine-owning realm.

NX rollout controls and a hosted administrative override are complementary
policy inputs. Define one precedence rule before implementation: an approved
operator rollback policy wins; an allowed explicit session preference follows;
otherwise retain the venue/platform default. Resolve once for a new session,
validate the result, and pin it for that lifetime. A later policy change queues
a controlled restart. Do not implement a second fallback algorithm in each host.

## NX propagation and caller work

Pinned read-only NX source audit: `65e086a3c9039a79e4c7e9f354aba31d8f83009e`.
This is not deployed-binary or adopted-SDK evidence. The NX SDK at that pin is
older than the mirror contract above: its factory/worker still follow their
local registry. There is no persisted engine selector, init wire field or
server engine environment consumer in the audited paths.

| NX path | Required implementation boundary |
| --- | --- |
| Account schema/sync: `packages/safe-cex/src/types.ts:253-341`, `apps/web/src/utils/accountSync/accountFields.ts:60-97` | Decide synced account preference versus device-local policy; add typed validation and mapping. The normalizer spreads unknown properties, but that does not make a field supported end to end. |
| Main web: `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:1730-1743,2855-2866`; web `utils/exchange/createExchangeFromAccount.ts:118-164` | Resolve policy before construction and send the same effective selection to the owner; a mirror must not start a second engine. |
| Browser worker: `packages/safe-cex/src/webworker/ExchangeWorker.ts:308-316` at NX pin | Upgrade factory and worker together. The current mirror SDK already honors explicit Rust in both; the proposed forced-TS resolver is additional work. |
| Hosted client: `apps/web/src/utils/shim/worker/ExchangeServerWorker.ts:193-216,938-967` | INIT_FIELDS currently strips engine. Add a validated nonsecret field or use server authority and report its effective choice to the client. |
| Hosted private: `services/exchange-server/src/server.ts:325-355,933-952,1016-1069` | Server rebuilds options explicitly. Apply policy here while preserving credentials, signing delegates and ownership. Inheriting process.env is not a policy resolver. |
| Hosted public: `services/exchange-server/src/public-exchange-manager.ts:74-89,239-245,278-280,391-405` | Apply policy on creation and restart; the current map keys only by venue. Resolve sharing/caching compatibility before mixed engine cohorts. |
| CLI: `apps/cli/src/platform-adapter.ts:203-245`, `exchange-factory.ts:21-26`, `record/recorders/orderbook.ts:43` | Propagate policy through shared account and public/recorder factories; a web flag alone does not reach CLI. |
| Signing host: `apps/web/src/utils/shim/worker/signingHost.ts:88` | Preserve signing-specific TS selection and report any effective-engine constraint; do not blindly forward a trading rollout override. |

These are required future caller changes, not edits included in this docs PR.
Mobile/Hermes availability and remote signing remain separate owner decisions;
a selector does not create WebAssembly support or a new signing implementation.

## Cutover merge requirements

Before enabling a Rust default, retain a separately addressable production TS
loader for every venue declared rollback-capable. Retain its dependencies,
exported helpers, signing delegates, platform registry entries, build chunks
and packaging paths. Compile and exercise explicit TS and explicit Rust through
the actual factory, worker, hosted, CLI/MCP and supported mobile host paths.
Verify packed/compiled distributions, construction failure visibility, init
serialization and restart behavior, not just direct class construction.

Keep executable TS performance references and strict replay comparisons as
independent evidence. A file under `__tests__/reference` does not establish
production reachability, packaging, signer compatibility or current API support.
Do not delete a family, deep-imported helpers or its oracle at the default flip.
TxFlow must be listed as having no TS rollback until an independently reviewed
fallback exists; a whole-release rollback needs a compatible, verified previous
artifact and its account/protocol support, not merely an older commit SHA.

## Soak, read-only shadow and deletion evidence

Proposed minimum observation windows: use the upper end of the existing
3–7 day shadow requirement (seven days), then seven consecutive healthy canary
days for each venue/product before wider rollout. Keep the verified TS fallback
for at least one full production release cycle after wider rollout, including
an exercised rollback. Any material engine/adapter/signing change restarts the
affected observation window. Duration alone cannot close missing coverage;
these are recommendations awaiting owner approval, not completed evidence.

1. Approve per-venue environments, accounts, products/aliases, brokers, transports
   and rate budgets. Collect authorized private lifecycle captures and declared
   deltas; no production orders or captures are authorized by this document.
2. Run both implementations on the same sanitized captures with complete strict
   request/receipt/state comparisons. Keep known failures visible. Repeatability
   is not parity; missing native metadata stays unknown.
3. Observe the proposed seven-day read-only production shadow before a default
   flip, pending owner approval, covering
   ordinary activity, startup, reconnect/recovery, rate limiting and market
   changes. Enforce a no-mutation boundary in the shadow transport, including
   startup/settings side effects and automated strategies. Do not equate POST
   with write: some venues use POST for authenticated reads. Independently test
   a venue-specific read allowlist and reject all other actions before dispatch.
   Do not duplicate live order writes to compare engines.
4. Compare full balance/position/order/fill/settings snapshots with freshness
   and request ownership, explicit missing/incomplete fields and approved
   tolerances. Budget duplicate read load and signing access. A stale cached
   mirror, missing timestamp or identical empty result cannot certify coverage.
5. Canary one approved venue/product/cohort at a time. Require visible errors,
   readiness, history completeness and financial reconciliation; hold formerly
   circular-evidence venues until independent evidence is sufficient. Exercise
   the rollback path with approved accounts and reconcile live orders/outcomes.
6. Retain both production paths during the canary/soak and agreed rollback
   support period. Before deleting TS, require the evidence above, successful
   rollback exercises, caller/deep-import migration, owner approval and a
   compatible release recovery artifact. Re-run against the exact release to
   be shipped. This document establishes none of that live evidence.

## Controlled rollback and operational costs

Pause new order writes and algorithms first. Record in-flight command identities
and known/unknown outcomes; reconcile unknown outcomes against the venue before
allowing resubmission. Never retry an uncertain write through another engine.
Quiesce the old owner, fence its generation/listeners and dispose it before
creating the replacement. Existing orders remain at the venue; rollback itself
must not cancel/recreate them or assume they vanished with a socket.

Create a fresh exchange using the approved effective selection and identical
account/product/routing/signing inputs. Bootstrap authoritative account state,
reconcile existing orders/fills, pending outcomes and venue-side protection or
dead-man schedules together with their refresh owner, and wait for real startup
and capability readiness. Switching engines must not silently abandon or
duplicate a required protection refresher. Cached loaded flags are insufficient. Publish the
new session generation and effective engine coherently to clients; resume
writes/strategies only after reconciliation. Failure must remain visible and
leave trading paused, rather than activate two writers.

The main costs are retaining two implementations/dependency graphs, testing
multiple host/product combinations, provisioning a policy propagation path and
operating extra read/signing load during shadow. Rollback interrupts the session
and may be delayed by outage or unknown command outcomes. TS may itself lack a
new Rust feature; capability/product compatibility must be checked before a
canary, not discovered during an incident. Full-release rollback also has frame,
cache, settings and deployment compatibility costs.

Recommendation for the owner: require a tested explicit TS/Rust construction
resolver and coherent policy propagation as a cutover prerequisite for each
rollback-capable venue; keep defaults unchanged until implemented and proved.
Adopt the staged shadow/canary/deletion criteria above. Keep TxFlow's unavailable
TS fallback explicit. Selection implementation, policy authority, live approval,
soak duration/support window and deployment remain future approved work.

## Plan and verification scope

The short docs-only plan was committed before this recommendation. Re-verify
SDK construction and the pinned read-only NX call sites; compare resolver,
application policy and hosted controls; document TS reachability, costs,
reconciliation and proposed soak/deletion criteria. Independently review the
recommendation, check local links/index and preserve unrelated audit/caller text.
Require green CI, a separate exact three-doc trial guard and an exact-head merge.
No source tests, live evidence or adoption are claimed by this documentation.
