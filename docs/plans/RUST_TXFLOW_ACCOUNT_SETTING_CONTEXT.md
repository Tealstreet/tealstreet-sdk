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


The corrected checkpoint executes eleven controls on unchanged actual49f:
three original adoption-bound assertion REDs plus one new managed Primary-intent
assertion RED, seven native GREENs, zero ignored and no compile/fixture failures.
All ten prior control bodies remain an exact prefix. The new test first fails
because no managed preparation exists; its exact Primary-intent and bound-route
assertions remain gated until adoption. Scoped strict Clippy and formatting pass.

## Managed source checkpoint

The corrected plan/original checkpoint 72c7 is CLEAR from m26ps4 (API/core) and
5c57jl (native/caller). Both independently clear runtime artifact SHA256
fa32fd1684191b99348805f84147c028216de13df64259015aec14dedf4fbae4.
Only account.rs and lib.rs change production: genuine explicit Primary Background
Read admission, current generation/Context custody before generic status/DATA,
exact failure-only retirement and the original settings revision/body/reducers.
The new physical Context retires before native cadence and on disconnect.
Queue refusal returns its actual Pending into the same current-owner cleanup.

All eleven original controls remain byte-exact and now pass. Eight added unit
controls execute real engine contexts and host handshakes around the private
sender: 256-peer pressure, one-credit recovery and all-credit reclamation;
nonfinite refusal; typed precommit failure; old preparation/released DATA versus
replacement generation; revision-stale current delivery; released Network and
Timeout; and engine Context retirement without native follow-up. These helper
controls do not fabricate a successful startup or economic facts. The separate
originals/native account tests cover startup, actual setting publication and
write/socket revision plus hedge rails. Precommit local NotSent is preserved;
released Read failures retain no Mutation outcome. An initial unit expectation
incorrectly stripped precommit outcome and was corrected to the shared typed
NotSent contract, without a runtime change. Missing Venue import and required
fixture book methods were compile-only corrections, excluded from RED evidence.

Scoped native TxFlow passes 183 tests, zero ignored, with strict all-target
Clippy and package formatting passing at jobs=4. Initial existing account tests
dropped the managed setting request; five collectors now perform real managed
handshakes at the same fixture cause clocks. All 166 assertion invocations in
those files are token-exact, and the existing financial account assertion file
is untouched. The WASM fixture also receives the same two-line required
prepareRecordedHttp hook approved for #2075; all its original assertions remain
byte-exact and no readiness wait or runtime fallback is added. No local WASM
build is claimed for this setting source; CI owns the composed artifact proof.

Only the account.rs count-one sender is pruned, actual inventory 10 to 9. Foreign,
signing and selection maps remain incoming-exact. Existing CPU hints/cache,
account state/orders/fills, history, trade serialization, wire, signer, sockets,
engine, registry, Cargo, profiles and budgets are unchanged.

Composition pin from m26: #2071 adds typed Candles failure arms to the same
on_http_preparation_failed/on_http_failed pair. Whoever integrates second must
retain one pair of overrides, handling Candles through read.failed with the full
typed error before generic on_request_failed fallback for setting cleanup. A
same-engine managed candle plus setting failure control is mandatory at that
actual composition. #2075's pre-status ticker guard and generic failure fallback
must likewise remain when that actual source is integrated. No pending venue
head is described as an actual dependency or consumed for this checkpoint.
Committed full-map/fixture preservation reviews and fresh CI remain gates.

### Actual command-candle composition checkpoint

Plain-merged actual #2071 through landing 2caa80214f06. m26ps4 and 5c57jl
cleared exact source-union patch
`039ff692f25f0202dd5f2c7b784d0733f1fa46091a6016d7a38b6cd513a6a734`
before application; complete lib blob is
`7569644a6170c4d9929cbb157c4100253442e207`. The Account and Candles DATA guards
remain independent early returns before common status/codec. Incoming handlers
keep typed candle failures first, with the native sign-error/request-failure
fallback retaining Setting exact-owner retirement and its 100 ms continuation.

The new same-engine `setting_candle_failures` target drives both preparation
refusals and released failures. Both controls pass, pinning one typed candle
terminal (InvalidInput/NotSent or Network/no outcome), no immediate HTTP or
Prepare retry, exactly one native 100 ms Setting continuation, and the second
coin's real managed request without candle retry. The unchanged setting11 and
candle19 targets also pass. Final actual ticker-seed composition, scoped strict
checks and composed CI remain gates before readiness.
