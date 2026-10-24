# Orderly unsupported order-control entry custody

## Checkpoint and boundaries

Original plan/RED checkpoint on actual `e932d2b45019f9b0ade2ae192462c4f2428afe10`.
Branch `feat/rust-orderly-unsupported-order-context`, owner codex-5c57jl.
Following uufub4's exact d9658b5502c plan/API CLEAR, the user authorized
production and source-stage controls, then a draft PR and CI in parallel with
source review. Original test assertions and existing native tests stay exact.
Hcmq3f released Orderly source/fixture ownership; notify that lane of this pick.

This slice targets ONE numeric failure in trading.rs::trade_command: any
PlaceOrders batch containing bybit_bbo or bybit_slippage_tolerance_ticks is
rejected as Unsupported/NotSent before account readiness, queueing, planning,
signing, receipts, reads/writes or optimistic financial mutation.
Fingerprint a8f3774dc23d4fd21635850184d55eaea951ae853ee80d2c14aafe0810264aa7,
count1. Native message: Bybit BBO and TickSize controls are not supported by
Orderly. Both field-presence checks remain native, including nonfinite or zero
TickSize values; this is not a new validation rule or a Bybit adapter.

On this actual base Orderly inventory is28F/29O, trading.rs9F/9O. A later
implementation prunes1F/1O to27F/28O and trading8/8. The separate #2051 history
slice prunes3/3, so AFTER its actual landing this slice instead goes25F/26O to
24F/25O. Do not claim #2051 is landed or prune its entries on this branch.

## Genuine initial-only proof

trade_command is also reentered by trade_queued, so most other numeric errors
are NOT eligible for an initial-entry helper. This selected first branch is:
all incoming PlaceOrders with either unsupported field return before the only
PlaceOrders queue push_back; retained Commands are unchanged, and push_front
is exclusively CancelSymbolOrders. A rejected field-bearing batch cannot enter
the deferred queue. No synthetic initial entry or special deferred fallback is
needed or allowed. Preserve this invariant in source review before adoption.

Account-not-ready, queue-full, planning errors, queued supersession, job
terminals and leverage results remain held. In particular, bulk cancellation
can push_front beyond the ordinary queue cap: do not assume every later
queue-full rejection is fresh merely because trade_queued pops an element.
There is no whole-job custody or physical request adoption in this slice.

## Existing shared API only

For later approved production, replace this ONE failure with a private
trading.rs helper consuming genuine Mutation command entry. None and Expired
receipts use reject_unstamped_command/reject_expired_command respectively.
A live entry admits one keyless Mutation Context with existing local20s/Never,
no pacing, then fail_context with the unchanged native Unsupported/NotSent
error. Shared admission rejection already emitted its terminal; never use a
numeric fallback or reject a different queued command. No physical leg,
signer, entropy, journal write fact or retry authority is issued.

At a finite valid entry retain the exact native message, kind Unsupported,
outcome NotSent, whole-batch no-IO and no Store mutation. At a nonfinite genuine
receipt the intended custody correction is shared InvalidInput/NotSent instead
of the legacy Unsupported result. Mutation entry and physical-write outcome
are distinct; this rejection never creates an Unknown write.

## No combo-5b source or key changes

Actual train/combo-5b Orderly own diff includes lib.rs, sign.rs, Cargo.toml and
its dispatch_recipe test. None is in this slice. Preserve all selection,
credential/nonce/signing/setup paths and accepted wire exactly. Source target
is trading.rs only; the new dedicated test uses existing native support.
Implementation must preserve queued/native planning and reducers byte for byte
outside the one call and helper/imports. This checkpoint changes plan/index
and one new test only. No core seam or architecture exception is required.

## Original evidence and later gates

Commit plan before a dedicated tests/unsupported_order_context.rs. Separate
BBO and TickSize nonfinite-entry assertions expose missing shared entry custody.
Working controls pin full returned actions, exact native metadata and populated
financial Store for idle/busy batches; keyless setup proves the unsupported
branch precedes account readiness. Preserve ordinary keyless failure and an
accepted native write as working reachability controls. Test invalid tick
values with a FINITE entry clock so payload validation cannot masquerade as
entry-clock rejection.

Before a future freeze add genuine peer-capacity denial/recovery, retained
current-command duplicate and repeated-terminal credit controls. Preserve all
original assertions; check the initial-only source proof and exact one-site
inventory pruning against the actual landing, including #2051 if landed.
Production needs fresh native/custody review, scoped strict/fmt/bans and CI;
none of those later source gates is claimed by original RED evidence.

Only scoped local Cargo with CARGO_BUILD_JOBS=4, one own build at a time.
Logs stay outside Git. No push/PR/production/merge or broad build is part of
this plan/RED-only checkpoint.

## Original executed receipt

Plan14f9cbcf83f precedes original test7a0bc0786cc. First compiled execution
reached the two intended shared-entry assertions plus three fixture-only
failures: the public-only helper wrongly expected unloaded orders. Native
startup marks that empty table loaded. Fixture correction8ba8750e93d pins the
actual loaded flag plus empty orders/positions, without production changes.
Those three earlier fixture failures are excluded from custody RED evidence.

Corrected original command:
`cargo test -p safe-cex-venue-orderly --test unsupported_order_context`
Result:11 executed, **2 assertion RED / 9 GREEN / 0 ignored**. BBO and TickSize
each fail at the intended typed-kind assertion (Unsupported vs InvalidInput);
every preceding full-batch/financial-frame/NotSent assertion passed. All native
finite-clock controls pass, including private ready idle/busy, public-keyless
precedence, nonfinite tick payloads and a genuine active write finishing with
its exact large order ID, no delayed rejected batch, then one native successor
POST with original symbol and caller identity fields.
Log `/tmp/codex-orderly-unsupported-order-original-corrected.log` stays outside
Git. Scoped strict Clippy and fmt PASS. Canonical scanner/source manifest agree;
only README, this plan and the new test differ from actuale932. All runtime,
prior fixtures, inventories, checker algorithms and 5b files remain byte-exact.
No new source application, PR, CI, production or READY claim.

## Production adoption and source-stage controls

The first unsupported-field branch now calls a private trading.rs helper.
Full None/Expired receipt rejection precedes keyless Mutation admission; the
native Unsupported/NotSent error finishes through fail_context. Shared
admission refusal owns its failure, with no numeric fallback. The native
field-presence predicate, preceding trade_id extraction, all subsequent guards,
queue changes, planning, physical signing/writes and reducers remain unchanged.
Only a8f3774dc23d is removed from the request inventory; no new entries or
checker changes. #2051 remains a separate unlanded history slice at this point.

New unsupported_order_admission controls allocate256 actual Background Read
peers from a preceding no-ID callback, then independently test BBO and TickSize
capacity failure exactly once as Mutation/NotSent. Freeing those peers restores
the original native error. A retained genuine Mutation root blocks both
same-ID queries; its own fail_context emits one pre-write NotSent terminal.
Then540 local rejections at one clock and one reused command ID all preserve
single-terminal behavior, after which all256 peer credits allocate again.
No fabricated accepted-write fact is needed. The initial fixture tried to
complete that unconfirmed Mutation root successfully; the engine correctly
projected failure, so the control now asserts the owner's real failure path.
Production was unchanged by that fixture correction.
