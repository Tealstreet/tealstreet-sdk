# TxFlow unsigned account-setting poll custody

Plan and original controls, owner codex-hcmq3f, 2026-10-25. Base is actual
`49f7d1857b66fc14f5164c8ca82ce42d48a53c1d`. No production, inventory, Cargo or
existing fixture changes belong to this checkpoint. The preceding ticker-seed
slice is draft #2075 at `05878ca91f0`, not an actual landing dependency yet.

## Size and inventory

One occurrence in `account.rs::next_setting`, fingerprint
`433b39542c0b7e8d83e35d3e897cccc64377df2f741272c2b8a3ec51f571b6e1`, count one.
Estimate 140–220 production lines across account.rs/lib.rs, plus 250–400 lines
of dedicated controls. Actual family inventory at this base is 10 after #2040
landed. This slice alone gives 10 to 9. After the separate pending candle slice
#2071 (three occurrences) and ticker seed #2075 (one occurrence), it gives 6 to 5.
Neither pending runtime is an actual dependency here; integrate their genuine
landings before adoption if they land first. This is one physical sender, not
an additional key-selection or signing cutover.


This sender is independent of the shared info sender. Managing metadata alone
cannot remove the shared sender while account/history still use it, so no
cosmetic split or shared-sender inventory reduction is proposed here.

## Native behavior and proposed custody

`activeAssetData` is an unsigned main-wallet/vault read: POST /info, original
JSON user/coin/body and Content-Type then x-api-version header order. Native
account routing has no PublicCors override. It is not a setting mutation and
needs no signing, credential selection or fabricated identity.

Propose one genuine keyless Background Read Context per physical setting call,
Never retry and no pacing, admitted with
http_context_with_egress(Background, policy, Egress::Primary). Credential None
otherwise defaults to RoutingIntent::Public, which does not preserve this
account-scoped wallet/vault query's host route. Explicit Primary egress carries
no credential or signing authority. There is no Session here. Its explicit 20-second admission-to-expiry bound,
including a stalled Prepare/Bind, is NEW policy. There is no aggregate deadline
over the sequential settings queue. Review the exact existing Context/managed
send API and native egress before source.

Retain native private-startup/public-only availability, one inflight setting,
the 100 ms successor timer and 60-second refresh. Pending::Account(Setting)
retains the exact Context alongside its existing generation/revision fence and
coin. A private current-attempt marker must distinguish old and new ownership.
Validate exact current ownership and `validate_background_read_http_delivery`
before status, envelope, subject/coin decoding or any financial effect.

Keep generation/revision checks, setting/hedged state, buy/sell leverage rails,
field precedence and malformed-data diagnostics unchanged. A current response
with an old revision still follows native release/100 ms scheduling, but cannot
publish. A stale owner must not clear a newer inflight flag, timer or Context.
Failure-only cleanup uses exact returned Pending/current ownership without
requiring DATA authority, retiring before native notice/cadence. Engine
retirement must release retained Contexts without inventing follow-up work.

An unissued admission or queue refusal clears only its own newly acquired
inflight state and follows bounded native 100 ms progression once; no recursive
admission or credit leak. Nonfinite clocks cannot create HTTP or nonfinite
recovery timers. These refusal details require exact source review, rather than
being described as unchanged native transport behavior.

## Original controls and source gates

Commit and execute originals against unchanged source before adoption:

- Separate REDs for eager physical HTTP, absent pre-Prepare admission timer and
  setting publication at the new exact 20-second bound.
- Native private setup and exact unsigned POST/body/headers/route, settings and
  hedge rails without positions, user/coin mismatch and malformed/status errors.
- A separate NEW-CONTRACT control requires the actual managed Prepare intent
  to be Primary; retain bound request route None and no PublicCors. The native
  route-only control cannot prove managed host egress by itself.
- Genuine write/socket revision changes fencing a held response, native 100 ms
  successor and 60-second refresh, plus ordinary no-read/public-only behavior.

Before source freeze, cover real 256-context pressure, one-credit recovery and
all-credit reclamation; late/stale Prepare and duplicate DATA; old generation
cleanup that preserves a replacement; released Timeout/Network and typed
precommit failure. Preserve existing financial assertion bodies. Only host
collectors may switch to actual Prepare/Bind/Claim at their original clocks.

Account state/orders/fills, the shared info sender, history, trading, key
selection, signer, sockets, engine, registry, Cargo and build/budget settings
remain outside the slice. Obtain native/caller and core/API plan/original
CLEAR before production. Integrate preceding actual TxFlow landings genuinely
before source or freeze, with exact source-conflict review when necessary.
Scoped Cargo only, sequential CARGO_BUILD_JOBS=4; no workspace sweep or WIP push.

The plan precedes the new isolated setting_context.rs target. Against unchanged
actual49f runtime, ten controls compile and execute: three intended assertion
REDs and seven GREENs, zero ignored. Separate first assertions pin eager raw
HTTP, missing pre-Prepare timer and exact-deadline leverage publication. The
late-data suffix after the failing publication assertion remains gated.

The seven native positives pin unsigned POST/account routing and headers,
valid leverage without invented positions, foreign user/coin rejection,
malformed/status failures, the 100ms successor and a fresh minute refresh with
no aggregate 20s account-poll bound. Existing fixture and all production blobs,
inventories, key selection, Cargo and shared source remain incoming-exact.
No fixture correction or compile failure precedes this execution. Scoped strict
Clippy and package formatting pass with jobs=4. The execution log is retained
outside Git at /tmp/codex-hcmq3f-txflow-setting-original.log. Capacity, exact
replacement/retirement, typed failure and generation/revision/hedge controls
remain source-stage gates; no runtime adoption, source clearance, CI, readiness
or landing is claimed.


API review correction: m26 identified that implicit keyless routing would turn
this wallet query into Public intent. Source is held until the explicit Primary
egress plan/control delta is reviewed. The new Prepare-intent assertion is a
managed-adoption contract, not a claim about a legacy routing breach. All ten
prior original control bodies remain unchanged; no runtime or inventory change
is present in this correction.
