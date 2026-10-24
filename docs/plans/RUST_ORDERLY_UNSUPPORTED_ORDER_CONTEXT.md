# Orderly unsupported order-control entry custody

## Checkpoint and boundaries

Plan and original REDs only on actual `e932d2b45019f9b0ade2ae192462c4f2428afe10`.
Branch `feat/rust-orderly-unsupported-order-context`, owner codex-5c57jl.
No production, inventory or existing-test edits are authorized here.
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
