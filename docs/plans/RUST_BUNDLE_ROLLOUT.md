# Per-user Rust bundle rollout preparation

Sam's final decision is a **hard-cut bundle switch per user**. Old means the
frozen production app and its TS trading sessions. New means the new app with
Rust for **every** venue and matching Rust exchange-server sessions. A user
moves all accounts together; no account setting, venue fallback or blended
bundle changes that decision. Rollback selects the frozen bundle again.

This is preparation only. It authorizes no live capture, orders, shipping,
deployment, flag changes or SDK-master merge. Implementation belongs to later
reviewed app/server changes; this PR changes only planning and offline tools
on `feat/rust-rewrite`. Sam's separately flagged Vultr/infra migration remains
outside this plan. Its effective selection must be recorded during rehearsals
so a bundle incident is distinguishable from an infrastructure incident; this
plan neither changes that flag nor requires a combined switch.

## Evidence and scope

[Live gate](RUST_LIVE_GATE.md) defines public capture, private read-only shadow,
smoke and observation evidence. [Cutover readiness](RUST_CUTOVER_READINESS.md)
and [master blockers](RUST_MASTER_MERGE_BLOCKERS.md) define SDK/consumer and
legacy-removal requirements. Replay CI and a Rust registration are source
proof, not live readiness. Existing private replay FAILs remain failures.
Earlier per-account engine-selection and default-flip recommendations in
[RUST_ROLLBACK.md](RUST_ROLLBACK.md) are historical alternatives; Sam's bundle
decision controls this rollout. The rollback safety rules in that document
still apply.

The readiness matrix records one immutable SDK landing and every registered
alias, its source family, HTTP legacy inventory, socket migration status,
dispatch-signing declaration and registry default. Those lexical counts are
bounded source inventories, not counts of live requests or proof that every
operation is managed. All runtime evidence is unverified until an operator
records matching artifact, alias, host, signing and account-shape evidence.
No user cohort is currently certified by this plan.

## 1. Select the bundle before app load

Introduce one trusted per-user assignment, provisionally `RUST_BUNDLE`, with
values `frozen` and `rust`, default `frozen`. The name is a proposal, not an
installed flag. The edge resolves the authenticated user's authoritative
assignment before returning HTML or the first application script. A signed,
HttpOnly, Secure, SameSite cookie may cache assignment revision and bundle ID;
it is a cache of server authority, never a user-editable engine grant.

Anonymous users and missing, expired or unavailable assignments receive the
frozen entrypoint. A known Rust assignment with an invalid artifact or session
contract fails closed before account start; it does not silently start TS
inside the new app. Authentication changes invalidate another user's cookie.
An assignment change triggers a controlled whole-app reload, not a component
or running-engine toggle. A multi-device user has one authoritative assignment;
old tabs cannot retain write authority after a revision change.

Serve immutable manifests for both bundles, with exact app/SDK/worker/WASM and
exchange-server artifact digests and wire versions. Partition HTML caches by
assignment/bundle revision or bypass shared caching for assigned HTML. All
asset URLs and service-worker cache namespaces include the bundle digest;
never overwrite a frozen asset with a new bundle asset at the same URL. Test
first load, signed-in refresh, login/logout, stale cookie, CDN hit, service
worker upgrade and offline startup. A stale/offline app may display cached
state but cannot create a trading session against a different assignment.

## 2. Exchange-server must match the session

A new-bundle account always requests `engine: 'rust'` through the shared account
mapper and both direct and worker construction. Forward the selection in
`INIT_FIELDS` and the server's validated account reconstruction. The server
checks it against the authenticated user's assignment, echoes **effective**
engine plus assignment revision, bundle/SDK IDs and session generation, and
refuses mismatch before startup or any write. Missing engine remains a frozen
protocol case; it must not silently default a new session to TS.

All server creation paths need proof: private sessions, shared public feeds,
restarts, signing hosts and reconnect/resume. Current public-feed caches keyed
only by venue cannot mix engine-dependent ownership silently: partition by
bundle/engine/protocol or prove an explicitly read-only common feed contract.
A keyless TS signing delegate is not a TS trading engine; retained delegate
wire/credential semantics must work while the effective trading engine is
Rust. Unsupported signing/account shapes block the user.

The frozen server also needs an approved minimal assignment/session fence (or
an already-proven compatible admission boundary). Retaining an unchanged old
bundle does not by itself revoke a parked writer. This compatibility-only
exception must be implemented and rehearsed before reversible cohorts.

Session parking/resume binds user, assignment revision, effective engine,
bundle/wire version and generation. A frozen session is not resumable as a
Rust owner, or vice versa. On a switch, fence and retire the prior writer on
all tabs/devices, close old session resumption rights, then start the replacement.
Never let simultaneous frozen and Rust sessions write for one account.

## 3. Eligibility is the intersection of all held venues

Evaluate every saved/enabled account, including hidden, dormant, demo, regional,
spot, inverse, hedge/one-way, unified/portfolio, broker/subaccount and hosted
signer shapes. Include venue-dependent algorithms, orders and protection
refreshers. Unknown aliases or shapes, an unavailable product, missing old
bundle support or unverified proof blocks the **whole user**. No account is
silently omitted to make the cohort green. Re-evaluate before adding/enabling
an account; an ineligible addition holds trading and follows an explicit cohort
exit procedure, never a per-account TS fallback inside the Rust bundle.

For each alias/shape require exact release evidence: applicable replay and
consumer tests; public quiet/volatile captures; credentialed no-write shadow
with matching account state; dedicated-account smoke; client and hosted engine
parity; storage/rollback rehearsal. Include native precision, order identity,
settings, history completeness, private auth/reconnect, remote/payload signing,
routing and secondary credentials. A documented unsupported capability is not
a passing applicable test. Its user must not depend on it.

Bitunix remains legacy inside the Rust engine under the temporary
`managed-sockets` boundary in #1771. That boundary is **remove-before-master
unless Sam approves a specific Bitunix endgame**; this plan grants no waiver.
Other legacy HTTP/socket entries and master blockers remain visible. Their
presence never counts as successful live proof.

## 4. Pre-user isolation and cohort stages

| Stage               | Entry and evidence                                                                                                                                                                                                  | Exit / decision                                                                                                                        |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Internal isolation  | Record Sam's independently proven infra selection; exercise immutable app/server artifacts using internal traffic.                                                                                                  | Proven transport/session routing, startup and failure attribution; no external users.                                                  |
| Private shadow      | Authorized dedicated read-only accounts, each venue/shape; transport refuses financial writes, settings changes and side-effectful startup. Replay one capture through both engines; budget read/rate/signing load. | Zero unexpected state/precision/identity divergences; missing fields stay missing; no customer credentials or private tapes committed. |
| Dedicated smoke     | Explicit operator authorization for the existing live-smoke procedure; minimum-size budget, cleanup and reconciliation.                                                                                             | Every applicable lifecycle step passes. A skipped operation is not PASS.                                                               |
| Internal end to end | All accounts held by internal users eligible; client and exchange-server both Rust; exercise one flag flip and flip-back, multi-tab/resume and kill switch.                                                         | At least seven healthy days and one maintenance/reconnect event, per existing live gate; independently reviewed rollback closure.      |
| Opt-in              | Explicit user consent; eligibility for every held alias/shape; reversible cohort assignment.                                                                                                                        | Same observation period, monitoring and support review; Sam authorizes the next cohort.                                                |
| Widen               | Small bounded cohorts, then larger cohorts after review; preserve frozen artifacts and capacity.                                                                                                                    | Clean signals for each cohort; stop growth on missing evidence or incidents.                                                           |
| Default             | All remaining eligible cohorts and fallback policy reviewed by Sam.                                                                                                                                                 | Separate approval; no default change is performed by this preparation.                                                                 |

A material engine, adapter, signer or account-shape change invalidates affected
release evidence and restarts the affected observation period. Wall-clock soak
alone never closes a product or compatibility gap.

## 5. Shared state and safe flip-back

Treat frozen readers/writers as active peers until the last cohort completes.
No destructive migration or global IndexedDB-version bump may prevent the
frozen app reopening its database. Additive fields need old-reader and
old-write round-trip controls, including unknown-field preservation. Namespace
new-only caches separately; do not fork authoritative account/preferences data
without an explicit reconciliation contract.

Compatibility fixtures must cover saved account credentials by reference,
stable IDs (including any old WEEX ID mapping), aliases/product settings,
secondary credentials, layouts/watchlists, order and client IDs, financial
units/precision, history coverage, algorithm state and protection settings.
Never store engine permits, socket IDs or live session authority as reusable
state. Server persistence, localStorage, IndexedDB, service-worker assets and
multi-tab messages each need old->new->old tests using the actual frozen
artifact, not an assumed old schema.

The audited eager store uses DB `tealstreet-eager-exchange`, version 1,
`snapshots`, keyed by account name with a three-day TTL and no bundle/engine tag.
Cached `loaded` or order state is display-only: invalidate incompatible
snapshots and bootstrap from the venue before trading. Preserve user
configuration when clearing a cache. Coordinate pending asynchronous cache
writes so a retired app cannot repopulate incompatible state after invalidation.

Rollback has one operator action: assign the user to `frozen`. Its controlled
handoff first pauses new writes and algorithms, persists/reconciles in-flight
outcomes, fences the former session across devices, and disposes it. Unknown
writes are never replayed on TS: query the venue by stable order/client ID,
retain unknown when reconciliation is unavailable, and keep trading visibly
paused. Existing orders and venue-side stops remain; transfer any refresh/dead-
man owner exactly once. Load frozen app/server artifacts, rebuild authoritative
account state, verify capabilities and identity, then resume. Flipping the flag
alone is not evidence that reconciliation or rollback has completed.

Removing `engine: 'rust'` from today's SDK is **not** the rollback mechanism.
Some registry defaults already use Rust, and a test-only TS reference is not a
production fallback. Verify that the actual frozen bundle and its matching
server contain usable TS for every held alias/product and signer shape; if
not, that user cannot enter this reversible cohort until the gap is resolved.

## 6. Flags, kill switches and monitoring

| Control                                               | Current state / intended role                                                                     | Scope                                                                 |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Per-user bundle assignment                            | Proposed authoritative `RUST_BUNDLE`, sticky before app load.                                     | Both app and exchange-server session engine, one assignment revision. |
| Global/cohort kill                                    | Proposed force-frozen override with precedence over user assignment; same handoff/reconciliation. | Stops growth immediately; pauses new writes during transition.        |
| `ExchangeOptions.engine: 'rust'`                      | Existing SDK construction input; absent does not force TS.                                        | Set from new bundle contract, never an independent user rollout flag. |
| `ENABLE_HOSTED_SERVER` / `FORCE_ENABLE_HOSTED_SERVER` | Existing NX flags; transport choice/default, not engine parity.                                   | Preserve their semantics; verify direct and hosted paths.             |
| `ENABLE_EAGER_EXCHANGE`                               | Existing eager-cache hydration flag.                                                              | Display cache only; no session/write authorization.                   |
| `managed-sockets`                                     | Temporary compile-time migration boundary, not user kill switch.                                  | Legacy list shrinks; Bitunix obligation above.                        |
| Sam's infra flag                                      | Separately owned existing migration.                                                              | Record effective value only; do not change or redesign it.            |

Monitor by cohort revision, bundle/app/SDK/server digest, alias, account shape
and host (pseudonymous IDs only). Signals: effective-engine mismatch or mixed
writer; duplicate/unintended orders; unresolved/aged Unknown outcomes; startup
and readiness latency/errors; signature/auth/rate-limit bursts; private state
freshness and read-shadow diffs; sockets/reconnect storms; history-incomplete
signals; WASM crashes/fatal engine death; rollback reconciliation and support
reports. Never log keys, signatures, signed URLs or private tape bodies.

Zero tolerance: duplicate/unintended write, wrong effective engine or dual
writer, lost state, financial divergence, secret leak, fatal engine failure.
Stop cohort growth and use the kill switch, preserving Unknown reconciliation.
For latency/reconnect/auth rates, require a predeclared TS baseline, window,
absolute rate and denominator before cohort admission; missing numerical
thresholds block eligibility rather than inventing a value after an incident.
A healthy aggregate cannot hide a failing alias/account shape. The offline tool
checks evidence presence, not whether the evidence is true; human approval and
live monitoring remain separate.

## 7. Frozen bundle maintenance

Freeze features and dependencies, not exchange correctness or urgent security.
Retain exact app/server/WASM/assets, reproducible build inputs and rollback
capacity. Security/credential handling fixes and required exchange changes
(e.g. OKX port 443/routing) receive a narrowly scoped reviewed patch to the
frozen branch, with both old and new contract tests. Preserve artifact lineage;
issue a new frozen digest, recheck shared-state round trips and repeat rollback
rehearsal. No opportunistic features or broad dependency drift. Record owner,
reason, upstream API deadline and both-stack test result for every exception.

An unpatched mandatory exchange change makes that venue unavailable for safe
rollback and blocks affected cohorts. Do not call an obsolete bundle a
fallback just because its assets are retained. Retire it only after Sam approves
the last cohort, the support period and the resulting recovery mechanism.

## 8. Migration mechanism comparison and NX change budget

| Mechanism                                                  | Benefit                                                          | Non-additive cost / decision                                                                                                                                                                                                       |
| ---------------------------------------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A: one app, per-account `engine` flag on client and server | One asset graph and familiar construction input.                 | Mixed classes/feed ownership, cache identities and server sessions; cannot meet the chosen all-venues hard cut and would need an ordinary TS selector the SDK does not currently expose. Historical alternative, not this rollout. |
| B: two immutable app bundles, per-user pre-load switch     | Exact frozen recovery artifact; one user move across all venues. | Edge/cookie/cache and session identity contract, old/new storage compatibility, two maintained artifact graphs and frozen exchange patches. **Sam selected B**, with matching session engine.                                      |

Pinned NX audit and required work are in the companion NX change-budget section.
No NX application, infra or flag source is changed here. Exact compressed
bundle size and changed-line diff require Sam's frozen production and candidate
artifact/ref IDs; current source is not a deployed-artifact substitute.

### NX source audit and measurement limits

Read-only audit pin: `Tealstreet/tealstreet-next@8209aa518023d39505b553a6e0e8c3775a2f7a8d`.
Fifteen inspected files total **212,913 source bytes**. This is audit coverage,
not bytes changed, compressed app size, a frozen-production pin or a deployment
claim. The old `feat/safe-cex-rust` branch is 1,739 commits behind that pin and
adds zero commits; it is not the proposed new bundle. Required interfaces below
are changes to existing paths, so the rollout is not merely adding WASM assets.

| Workstream                  | Inspected current boundary                                                                                                  | Required non-additive work / proof                                                                                                                                                                                                 |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Assignment                  | `packages/feature-flags-core/src/flags.ts`; web `getFeatureFlags.ts`, `serverFeatureFlag.ts`, `/api/feature-flags/route.ts` | Authoritative before-load assignment and revision, signed cookie/edge verification, cache partition; existing runtime flag fetch alone is too late for bundle serving. No implemented edge selector was established by this audit. |
| Bundle delivery             | `apps/web/next.config.mjs`; `exchangeHostRuntime.ts`                                                                        | Build/retain two immutable asset graphs; prove worker/WASM loading and stale/offline caches. Need actual build manifests for byte totals.                                                                                          |
| Hosted init                 | `apps/web/src/utils/shim/worker/ExchangeServerWorker.ts:193-220,977`                                                        | `INIT_FIELDS` omits `engine`; explicitly forward and validate bundle/revision/engine instead of silently stripping them.                                                                                                           |
| Hosted engine               | `services/exchange-server/src/server.ts:939-960,1123`; `virtual-worker-context.ts`                                          | Server reconstructs account options without an engine selection; validate and echo matching selection at all private/public/restart paths.                                                                                         |
| Resume/ownership            | `server.ts:572-640,1294`                                                                                                    | Existing parked sessions need revision/engine/bundle fencing before reuse; prove no second writer across tabs and app switches.                                                                                                    |
| Shared preferences/accounts | `buildExchangeAccountConfig.ts`; `normalizeExchangeAccountConfig.ts`; `atomWithCustomStorage.ts`                            | Unknown-field spreading is not a compatibility contract. Round-trip actual frozen/new writers and preserve IDs/credentials/layouts.                                                                                                |
| Eager cache                 | `exchangeStoreCache.ts:11-14,52-59`                                                                                         | Engine/bundle-neutral snapshots cannot grant readiness. Namespace or invalidate cache safely without an incompatible shared DB upgrade.                                                                                            |
| Operational controls        | Existing flag registry, server startup and bundle manifests                                                                 | One cohort kill switch, effective-engine identity, precise monitoring thresholds and a recorded rollback rehearsal; no current live implementation certified here.                                                                 |

For the exact frozen/candidate comparison, supply two complete artifact
manifests to `compare-artifacts`. Each pins its commit and lists every file with
SHA-256 plus measured raw, gzip and Brotli bytes from the same build/compression
protocol. The tool reports added, modified and removed paths, whether changes
are purely additive, total bytes and deltas. Keep source-change manifests
separate from served-asset manifests; do not add client and server bytes into
one claimed download. Also record initial, per-venue lazy worker/WASM, hosted
server and duplicate-bundle storage totals separately. A valid artifact manifest
must include all files in its declared scope; the tool cannot discover omitted
files or authenticate external build provenance.

**Exact NX change LOC and old/new shipped bundle bytes remain unmeasured**:
the frozen-production artifact/ref and chosen new-bundle artifact/ref have not
been supplied. Do not substitute NX master or an obsolete prototype for either.
These identity and measurement gaps block cohort approval, while this plan and
the offline tooling can be reviewed now. Future measurements must bind the
actual frozen and candidate artifacts plus SDK/server versions, not just a
source branch name.

## Offline preparation tooling

Run from the SDK mirror with Node 22 and Corepack Yarn:

```sh
corepack yarn node scripts/rust-rollout/cli.mjs matrix
corepack yarn node scripts/rust-rollout/cli.mjs evaluate /absolute/path/synthetic-cohort.json
corepack yarn node scripts/rust-rollout/cli.mjs compare-artifacts /absolute/path/frozen-assets.json /absolute/path/rust-assets.json
corepack yarn node --test scripts/rust-rollout/model.test.mjs
```

The CLI has no network, credential, flag-write, deployment or apply command.
`matrix` verifies pinned source hashes, then prints the committed source
inventory. `evaluate` checks **submitted declarations**, all held accounts,
immutable artifacts, matching client/server engine/revision, old-owner fencing,
unknown writes, storage flip-back, required evidence and controls. It returns
exit 2 for missing declarations or blockers. Exit 0 means declarations are
complete for human review; it **never** verifies live evidence or authorizes a
cohort. The committed matrix intentionally blocks every alias until reviewed
live account-shape evidence and master dispositions exist.

Use synthetic pseudonyms and account IDs only. Never put credentials, private
account snapshots, signed URLs, session tokens or live tapes in input files.
The documented input contract and synthetic example are in
[`scripts/rust-rollout/README.md`](../../scripts/rust-rollout/README.md).

## Pinned per-venue readiness matrix

Source: SDK `2badd1cb0e3217b844e73d6b81ec023b3abc6d8b`.
Machine-readable policy: [rust-bundle-readiness.json](rust-bundle-readiness.json).
Every row is Rust-registered. `D` is only the dispatch-signing construction flag;
absence is not proof that signing is unsupported. `TS`/`Rust` is the current SDK
registry default, **not** verified frozen-bundle rollback availability. HTTP
counts are **family totals repeated across aliases**; do not sum these rows.
All rows have socket-core status **not landed at this pin** (#1771 is separate),
private/live account-shape proof **unverified**, frozen rollback compatibility
**unverified**, and cohort disposition **BLOCKED**. This matrix records no live
run. Refresh the immutable source inventory and evidence after relevant landings.

| Alias             | Family       | SDK default | Family legacy HTTP sites | D   | Live / rollback / cohort          |
| ----------------- | ------------ | ----------- | -----------------------: | --- | --------------------------------- |
| apexomni          | apexomni     | TS          |                       37 | no  | unverified / unverified / BLOCKED |
| asterdex          | asterdex     | TS          |                       71 | no  | unverified / unverified / BLOCKED |
| backpack          | backpack     | TS          |                       27 | no  | unverified / unverified / BLOCKED |
| binance           | binance      | TS          |                       34 | yes | unverified / unverified / BLOCKED |
| binanceinverse    | binance      | TS          |                       34 | yes | unverified / unverified / BLOCKED |
| binancepm         | binance      | TS          |                       34 | yes | unverified / unverified / BLOCKED |
| binancespot       | binance      | TS          |                       34 | yes | unverified / unverified / BLOCKED |
| bingx             | bingx        | TS          |                       43 | yes | unverified / unverified / BLOCKED |
| bitget            | bitget       | TS          |                       34 | no  | unverified / unverified / BLOCKED |
| bitgetinverse     | bitget       | TS          |                       34 | no  | unverified / unverified / BLOCKED |
| bitgetusdc        | bitget       | TS          |                       34 | no  | unverified / unverified / BLOCKED |
| bitrue            | bitrue       | TS          |                       42 | no  | unverified / unverified / BLOCKED |
| bitunix           | bitunix      | TS          |                       23 | no  | unverified / unverified / BLOCKED |
| blofin            | blofin       | TS          |                       23 | no  | unverified / unverified / BLOCKED |
| breakoutprop      | breakoutprop | TS          |                       21 | no  | unverified / unverified / BLOCKED |
| bybit             | bybit        | TS          |                       54 | yes | unverified / unverified / BLOCKED |
| bybitdemo         | bybit        | TS          |                       54 | yes | unverified / unverified / BLOCKED |
| bybitinverse      | bybit        | TS          |                       54 | yes | unverified / unverified / BLOCKED |
| bybitspot         | bybit        | TS          |                       54 | yes | unverified / unverified / BLOCKED |
| coinbase          | coinbase     | Rust        |                       34 | no  | unverified / unverified / BLOCKED |
| coinbasespot      | coinbase     | Rust        |                       34 | no  | unverified / unverified / BLOCKED |
| cryptocom         | cryptocom    | TS          |                       30 | no  | unverified / unverified / BLOCKED |
| grvt              | grvt         | TS          |                       56 | no  | unverified / unverified / BLOCKED |
| hyperliquid       | hyperliquid  | TS          |                       50 | no  | unverified / unverified / BLOCKED |
| hyperliquidspot   | hyperliquid  | TS          |                       50 | no  | unverified / unverified / BLOCKED |
| hyrotrader        | bybit        | TS          |                       54 | yes | unverified / unverified / BLOCKED |
| kodiakfi          | orderly      | Rust        |                       29 | no  | unverified / unverified / BLOCKED |
| kraken            | kraken       | TS          |                       47 | no  | unverified / unverified / BLOCKED |
| krakenspot        | kraken       | TS          |                       47 | no  | unverified / unverified / BLOCKED |
| kucoin            | kucoin       | TS          |                       80 | no  | unverified / unverified / BLOCKED |
| kucoinspot        | kucoin       | TS          |                       80 | no  | unverified / unverified / BLOCKED |
| lighter           | lighter      | TS          |                       46 | no  | unverified / unverified / BLOCKED |
| lighter-robinhood | lighter      | TS          |                       46 | no  | unverified / unverified / BLOCKED |
| mexc              | mexc         | TS          |                       29 | no  | unverified / unverified / BLOCKED |
| mode              | orderly      | Rust        |                       29 | no  | unverified / unverified / BLOCKED |
| okx               | okx          | TS          |                       45 | no  | unverified / unverified / BLOCKED |
| okxdemo           | okx          | TS          |                       45 | no  | unverified / unverified / BLOCKED |
| okxeu             | okx          | TS          |                       45 | no  | unverified / unverified / BLOCKED |
| okxeudemo         | okx          | TS          |                       45 | no  | unverified / unverified / BLOCKED |
| paradex           | paradex      | TS          |                       56 | no  | unverified / unverified / BLOCKED |
| phemex            | phemex       | TS          |                       50 | no  | unverified / unverified / BLOCKED |
| polymarket        | polymarket   | TS          |                       35 | no  | unverified / unverified / BLOCKED |
| salsadex          | orderly      | Rust        |                       29 | no  | unverified / unverified / BLOCKED |
| tealstreet-dex    | orderly      | Rust        |                       29 | no  | unverified / unverified / BLOCKED |
| txflow            | txflow       | Rust        |                       25 | no  | unverified / unverified / BLOCKED |
| weex              | weex         | TS          |                       51 | yes | unverified / unverified / BLOCKED |
| what              | orderly      | Rust        |                       29 | no  | unverified / unverified / BLOCKED |
| woo               | woo          | TS          |                       37 | no  | unverified / unverified / BLOCKED |
| woofi             | orderly      | Rust        |                       29 | no  | unverified / unverified / BLOCKED |
