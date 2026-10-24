# TxFlow unsigned account-setting poll custody

Plan-only next slice, owner codex-hcmq3f, 2026-10-25. Base is actual
`123c8aa15fffd74ec02556128f9fd0146049d4f4`. No production, inventory, Cargo or
existing fixture changes belong to this checkpoint. The preceding ticker-seed
slice is draft #2075 at `92fc8690bff`, not an actual landing dependency yet.

## Size and inventory

One occurrence in `account.rs::next_setting`, fingerprint
`433b39542c0b7e8d83e35d3e897cccc64377df2f741272c2b8a3ec51f571b6e1`, count one.
Estimate 140–220 production lines across account.rs/lib.rs, plus 250–400 lines
of dedicated controls. Actual family inventory at this base is 25. This slice
alone gives 25 to 24; after the separate candle slice's three occurrences and
ticker seed's one occurrence, it gives 21 to 20. Relative to the requested
22-site candle-adjusted baseline, the ticker and this slice together give 20.
The pending #2040 local slice's fifteen removals are separate; never count its
removals twice or claim an unlanded dependency is actual.

This sender is independent of the shared info sender. Managing metadata alone
cannot remove the shared sender while account/history still use it, so no
cosmetic split or shared-sender inventory reduction is proposed here.

## Native behavior and proposed custody

`activeAssetData` is an unsigned main-wallet/vault read: POST /info, original
JSON user/coin/body and Content-Type then x-api-version header order. Native
account routing has no PublicCors override. It is not a setting mutation and
needs no signing, credential selection or fabricated identity.

Propose one genuine keyless Background Read Context per physical setting call,
Never retry and no pacing. Its explicit 20-second admission-to-expiry bound,
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

This checkpoint records sizes and a plan only. No RED execution, runtime
adoption, review, CI, readiness or landing claim is made.
