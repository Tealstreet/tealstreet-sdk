# Rust command identity receipts — implementation plan

**Status: PLAN ONLY.** The API and venue hooks below are proposed, not
implemented or proven. This document records the phase 3b source audit for
[`RUST_BRIDGE_FIXES.md`](RUST_BRIDGE_FIXES.md). Phase 3a command settlement and
deadline work is a separate checkpoint. No native receipt implementation,
generated-ID coverage, runtime verification or performance result follows from
this plan.

## Production problem and current coverage

An interrupted write can have an unknown outcome. A caller needs the actual
native identity to reconcile that write before deciding whether to place again.
Current host interruption metadata captures only supplied `clientOrderId`
fields on the command or its input orders. It cannot observe identities minted
inside native formatters and jobs. Supplying no ID, or using a venue which
rejects supplied IDs, does not mean native code never generates an identity.

The source audit found generation paths in 21 of the 26 venue crates. That
count includes Lighter's client order index and Breakout's order code; it does
not mean every product, order shape or child has a client ID. Five crates do
not mint a native client ID on the inspected placement paths. Some of those
honor supplied IDs, while others have no client-ID field at all.

An identity receipt is attribution data, not a venue acknowledgement. It must
not be described as proof that an order exists, was accepted or was rejected.
Known native order IDs from acknowledgements and existing partial-result
metadata remain separate from pre-write client identities.

## Proposed native API

Add one typed notice and a corresponding `Cx` helper, with final names and
field types reviewed during implementation:

```rust
Notice::CommandProgress {
    command: CommandId,
    receipts: Vec<OrderIdentityReceipt>,
}

// Proposed convenience API; ownership details remain an implementation choice.
cx.command_progress(command, receipts);
```

The required receipt row is deliberately small:

```ts
{
  symbol: string;
  kind: 'clientOrderId' | 'clientOrderIndex' | 'orderCode';
  identity: string;
}
```

`kind` identifies the native identity namespace, not the order's Limit/Market
type. Numeric identities cross JS boundaries as decimal strings without
floating-point conversion. Accumulate a command-scoped set, deduplicated by the
tuple `(symbol, kind, identity)`, never by identity alone. Retain every actual
split, TP/SL child and replacement identity in that set.

Role or input-index metadata is optional, only where already typed and useful.
Do not fabricate an index or reconstruct original input/child indexes by
refactoring flattened planners. No existing receipt consumer requires those
indexes. Native client-ID reconciliation uses symbol and client identity;
command/session/account/generation correlation supplies ownership. An original
order ID may be useful replacement context where already available, but it is
not the replacement's client identity.

Supplied/requested identity and actual native identity must remain distinct.
The host already knows the requested fields. Native receipts report values
actually selected by the formatter, including generated children. Do not
overwrite the requested value with an assumption about native formatting.

Represent absence explicitly where the native write has no client identity.
Do not label a salt, request nonce, signature, optimistic store key or unknown
future venue order ID as a client order ID. Providing identity data does not
promise that the SDK already implements lookup by that identity namespace.

## Exact ordering and storage boundary

1. Keep the existing validation, entropy generation, formatting and signing
   behavior. Do not mint an extra ID just for reporting or move generation
   earlier across validation, cancellation or reconciliation boundaries.
2. Capture selected identities while their typed formatter values are still
   available. Carry a small receipt vector alongside the existing per-write
   `Step`, `Planned`, `Operation` or equivalent when serialization would
   otherwise discard them.
3. Emit progress before the corresponding `Http`, `WsSend` or delegated
   `SignPayload` action can be executed by a host. A notice appended after an
   already-queued HTTP action does not satisfy this order.
4. A host records the receipt before dispatching the subsequent write. It must
   record internal state before invoking an optional consumer progress callback;
   callback errors must not silently erase identity or strand unrelated action
   completions.
5. Existing definitive completion closes the command's progress stream. A
   receipt does not itself complete a promise, retry a write or dirty the Store.

Emission before delegated signing is safe as attribution: a later signing
failure may prove that no write occurred, while the receipt still records the
identity that was prepared. Error outcome classification must continue to use
the actual completion/failure path, not the mere presence of a receipt.

A trap before a Sans-I/O call returns its action batch prevents that batch's
queued I/O from escaping. Previously returned batches which did dispatch writes
must already have published their identities. Never query or drain a poisoned
engine to recover IDs after the fact.

No retained native command ledger, new Store section or public-feed frame is
needed. Use existing job/step ownership until the write is issued. Receipts must
not enter public provider frames or shared market-data channels.

## WASM host, SDK and worker contract

`sdk-rs/hosts/wasm/src/lib.rs` needs an additive serialized progress action.
`host.mjs` and `host.d.mts` must recognize its typed shape. The host accumulates
known receipts for pending commands and copies them into interruption errors,
alongside existing supplied IDs, partial order IDs and command metadata.

Give each command a host-session-scoped receipt key, for example a session
identifier plus the native command ID. This is a correlation key, not a venue
client ID. It must not contain credentials, signed payloads or other secrets.
Command IDs alone can be reused by a replacement engine, so they are insufficient
outside one host instance.

A deadline can settle a promise before a queued job selects its ID. Therefore:

- Removing a pending promise must not make subsequent identity progress vanish.
  Forward progress independently of whether that promise still exists.
- Include the same receipt key in the interruption error and later progress.
  An immutable error snapshot can truthfully contain only currently known IDs;
  do not mutate a previously delivered error as the late-delivery mechanism.
- State that identities may expand while native work remains active. A partial
  list must not be advertised as complete. A host-only error containing an
  empty list does not prove that native code will never mint an ID.
- The mainstream `RustEngineExchange` and worker wire need a typed progress
  event, not only a low-level host callback. Attach account and engine generation
  so the owner can correlate late information after the original RPC settled.
  Do not require an active `_pendingRequests` entry to forward that event.
- Keep state and recovery generation fences. Do not attribute old receipts to a
  replacement account generation, another account, or a reused RPC request ID.
  A dead/disposed host performs no further native I/O; it preserves identities
  already published before cleanup.
- Ordinary worker disposal must allow structured host interruption metadata to
  reach the matching RPC, or build an equivalent structured failure before
  removing its tracking. A generic early worker failure would discard IDs even
  if host disposal itself reports them correctly.

The implementation must provide an explicit consumer owner for late progress
so generated-ID attribution is not silently lost after a timeout. A correlated
SDK/worker event is the minimal seam; a new persistent journal or unbounded
receipt history is not required by this plan. No automatic resubmission,
compensation, JS HTTP-body parsing or substitute JS venue-ID minting is allowed.

## Native host and account server

`sdk-rs/hosts/native/src/lib.rs` already forwards `Action::Notice` to its
`on_notice` callback in action order. Extend that typed notice path without
parsing requests or translating venue protocols. Verify receipt observation
precedes the corresponding transport invocation.

`sdk-rs/hosts/native/src/server.rs` currently maps an internal command ID to
`Pending { client, original }`. `CommandDone` removes that entry and replies
using the client's original command ID. A receipt must use that same scoped
mapping and target only its owning client. **Never broadcast identities like
ordinary log notices.** Account routing is already scoped by the account task;
retain it and distinguish client/session ownership as well.

Mapping lifetime matters independently of promise lifetime. If a client-side
deadline expires while native work continues, the server's correlation mapping
must remain usable for subsequent receipts and eventual completion. A future
server-side waiting deadline must not delete the only route and silently drop
late identities. Keep routing until native completion or engine termination, or
define an explicit bounded policy that reports loss of receipt availability;
do not introduce an unbounded native ledger as a shortcut.

Currently `leave` clears the departing client's ownership from pending entries
instead of transferring it to another peer. Preserve that isolation: a newly
connected client or reused original command number must not inherit old
receipts. This plan does not promise delivery across disconnect, process death
or lost network delivery, and does not add durable reconnection replay.

The server currently collects host notices and handles them after
`Host::execute`. Audit that buffering when implementing the before-write
contract: host observation, serialization and client delivery are distinct
boundaries. Do not claim that a remote client durably received a receipt before
I/O merely because the native notice preceded the action.

## Venue inventory and hook scopes

Paths below are relative to `sdk-rs/venues/`. This is a source inventory, not
verified coverage. Ordinary variant aliases share their crate's paths; separate
spot modules and Hyro require the additional hooks explicitly listed here.

| Venue crate | Existing identity selection | Proposed typed hook scope |
| --- | --- | --- |
| `apexomni` | `trading.rs::prepare_order` selects the parent ID and separately mints embedded TP/SL IDs (`generated_id`, approximately lines 352 and 449 at audit). Preparation can occur later in `Step::Build` or amendment processing. | Retain every parent, split and embedded leg identity before fields become `sign::Request`; extend `Step::Write`, currently paired only with `Accepted`. Include identities prepared later during amendment processing. |
| `asterdex` | No generated client-ID field on inspected placement paths; caller IDs are rejected. | Represent unavailable identity truthfully. Do not introduce venue ID generation or replace acknowledgement IDs with placeholders. |
| `backpack` | Caller IDs are optional uint32 values. `prepare_place` generates IDs only for eligible paired ordinary orders using `generated_client`; ordinary singles/other shapes may omit them. | `Step::Pair(Vec<NativeOrder>)` and `Step::Place` retain typed `client_id`. Report both generated pair identities and actual supplied values; preserve absence on writes without one. |
| `binance` | `format_spot`, `format_order` and related inverse/algo paths assign broker-format IDs to regular, split and bracket orders; caller IDs are preserved where allowed. Amendments can plan replacements after cancellation. | Capture IDs beside typed `Params` before conversion to signed `Query`; carry receipt vectors on `Planned`. Retain every regular/algo, split and TP/SL identity across batch regrouping and later replacement planning. |
| `bingx` | `format` assigns IDs for plain native orders; stop/protection paths may have none. Cancel-replace also generates a replacement ID. | Carry receipts alongside `Step.params` from the exact selection sites, including replacement planning. Do not assume every `Call::Place` has a client ID. |
| `bitget` | Typed `Payload.client_oid` and `new_client_oid` hold regular, strategy and replacement IDs. Several payload builders mint IDs. | `Planned.payload` retains values at dispatch. Report identities from the actual selected `plan_job`, fallback/rollback or replacement write path; do not publish an unused alternative as a dispatched identity. |
| `bitrue` | `Placement.client_order_id` holds parent/split IDs; `trigger_order_create_params` contains separately minted protection IDs. | Capture all identities before `Plan::placements` serializes payloads into `Step.body`; retain receipts on the matching step, not only `Plan.creating` parents. Cover split and replacement preparation without rebuilding input indexes. |
| `blofin` | `Placement.client_order_id` is selected in placement formatting; additional splits get new IDs. Normal, TPSL and algo families differ. | Carry every actual identity before payload serialization into each `Step`, including chunked writes. Existing `Job.creating` is not sufficient justification to infer every step's identities from the whole job. |
| `breakoutprop` | Placement/replacement `Step::Place` holds a generated `tealstreet-*` code; `PreparedOrder::body` writes `orderCode`. Caller client IDs are rejected. | Report every actual code from `prepare_orders` and `plan_amend` with its explicit namespace before submission; do not claim caller-ID support. |
| `bybit` | Regular/inverse/spot `format_lots` uses supplied `orderLinkId` when present, without a general generated fallback. `format_option` generates UUIDs. Hyro has separate generated links in `hyro.rs`. Position trading-stop operations can have no client ID. | Retain identity alongside `Lot`, then `Planned`, through single/batch dispatch. Include options and the special Hyro rules below. Do not add ID generation to ordinary Bybit products as part of receipt reporting. |
| `coinbase` | `prepare` selects caller ID or generated `tealstreet-*`. `Step::Place` retains typed `Create` and `Order`, including replacement placements. | Use those retained values at write dispatch, including replacement identities. Do not use JWT nonces as order IDs. |
| `cryptocom` | Uses supplied `client_oid` only; no native-generated client ID found. | Receipt can report the actual supplied value from placement planning. Omission remains explicit; request IDs/nonces are not substitutes. |
| `grvt` | `prepare` selects a caller ID or generated numeric ID. `Effect::Place(Order)` retains identity. | Hook `trade_step` before the write, using the retained order and plan command. Keep generated decimal strings opaque. |
| `hyperliquid` | Parent and TP/SL cloIDs are selected while preparing writes; amendment code separately mints a replacement cloID. Covers the shared perpetual/spot implementation. | `Effect::Place` retains placement records with actual IDs. Publish every parent/child/replacement identity before the request. Keep replacement IDs distinct from the canceled order ID and from any bracket children. |
| `kraken` | Futures `trading.rs::prepare_order` generates `ts_*` `cliOrdId`, retained in `Prepared.order`. Spot `spot/trading.rs` independently generates random hex `cl_ord_id` and keeps only request params. Caller IDs are rejected. | Hook both futures and spot dispatchers. Futures can use retained typed order data; spot should carry the selected ID beside params. Include attached legs/replacements and avoid treating venue acknowledgement IDs as pre-write client IDs. |
| `kucoin` | Futures `trading.rs::format_placement` selects caller ID or a native timestamp/counter ID. Spot `spot/trading.rs::placement` independently selects caller ID or UUID. | Retain receipts before each `Placement` becomes a serialized `Step`/spot `Write`. Cover futures delayed replacement formatting after cancellation as well as spot. |
| `lighter` | `mint_id` creates a u64 client order index for each lot. `Prepared.native.order_id` becomes signing `client_order_index`, while SDK `Order.client_order_id` is `None`. | Publish every retained native client index, including split and TP/SL placements, before submission/signing. Use decimal strings and the client-order-index namespace; do not promise generic client-ID lookup support. |
| `mexc` | Regular payload construction generates `externalOid`; not every plan/stop path has one. Payload becomes `Step.body`. | Capture the generated value at regular/batch formatting and retain on the corresponding step. Include replacements which format later after reads/cancellation. Never infer an ID from a UUID-looking value elsewhere in the request. |
| `okx` | Typed `OrderPayload` selects `cl_ord_id` or generated `algo_cl_ord_id`, including split/bracket families. | Capture all actual identities before `Step.body` serialization across normal/algo batches, splits, brackets and replacement plans. Report selected field values, with no broker-prefix reconstruction in the host. |
| `orderly` | Uses supplied `client_order_id` only; no native generation found, including split planning. | Report actual supplied values where present; do not turn local optimistic keys such as `pending:{index}` into client IDs. |
| `paradex` | Parent `Placement.client_id` selects caller ID or generated `ts_*`; `Kind::Place.expected` retains these optional values. Child placements intentionally have no client ID. Preparation may be deferred for metadata/quotes. | Use retained expected identities at `Operation` dispatch, including deferred/replacement preparation. Preserve the deliberate absence on TP/SL children. |
| `phemex` | Per-lot IDs select caller value or generated `Tealstreet_*`; `Call::Place` retains the typed order. | Publish every actual identity before `trade_next` dispatch from the retained order, including splits and replacements. |
| `polymarket` | No client-order-ID field; caller IDs are rejected. A random signing salt is selected later while constructing the signed order. | Report client identity unavailable. Do not relabel salt/signature as an ID or add a speculative order-hash/lookup implementation. Existing acknowledged order IDs and partial-result metadata remain valid. |
| `txflow` | Selected/generated `ts_*` cloIDs exist on typed `OrderWire`; `Effect::Place` already retains the IDs in each chunk. | Publish every chunk's actual identities before its signed exchange request. Preserve existing reconciliation/acknowledgement behavior; execution acknowledgement itself does not supply order IDs. |
| `weex` | Placement planning selects caller value or generated `ts*`; `Call::Place.orders` retains typed IDs. | Publish every actual identity before `trade_next` dispatch, including batch/protective/replacement plans, without parsing `Step.body`. |
| `woo` | Uses supplied `client_order_id` only; no native generation found. | Report actual supplied values where present. Preserve paths without client IDs rather than inventing a generated fallback. |

## Hyro shared-write edge cases

`bybit/src/hyro.rs` stores `Group { creates, index, ids, waiters, ... }` and
`Create { link, ... }`. Several caller commands can share one native write.

- Publish the active write's identity for every waiting command before dispatch.
- A new waiter joining an already-running or reconciled group must receive the
  already-known identities immediately; waiting only for a new dispatch can
  leave it without IDs for the write it now shares.
- Publish all actual create identities without reconstructing source order
  indexes from the flattened lots/creates. The group index is an internal
  execution cursor, not a promised caller input index.
- A definitive rejection can cause an existing native group to select a new
  link for future work. Report the newly selected link for its owning command;
  do not replace or erase previously reported identities.
- Keep existing grouping, reconciliation, lookup and retry policy unchanged.
  Receipt delivery must never initiate another native write.
- Hyro position-stop writes can omit the create identity field. Do not claim a
  locally allocated link was transmitted on a trading-stop request that did not
  contain it.

## Proposed verification gates

Implementation must add meaningful tests before claiming coverage:

- Exact action ordering: receipt precedes each matching HTTP/socket/delegated
  signing action; no receipt itself initiates I/O.
- Independent caller and generated values; no extra entropy calls, broker
  prefix reconstruction or changed signed/request bytes.
- Batch regrouping, split lots, TP/SL children, nested protection payloads,
  replacement planning and deferred ID selection retain every actual identity.
  Deduplication uses `(symbol, kind, identity)` and cannot discard distinct IDs.
  Optional role/input-index metadata is tested only where already available;
  no original-index reconstruction is required.
- Identity absence is explicit for products/children without a native field;
  numeric indices survive JS serialization without precision loss.
- Deadline before ID generation, progress after promise removal, immutable
  interruption snapshots and eventual definitive completion use the same
  account/generation/session-scoped receipt key.
- Fatal and ordinary disposal retain prior receipts, preserve structured worker
  failures, ignore late state callbacks and never re-enter a dead engine.
- Hyro multiple waiters, joining after dispatch, and changed future links retain
  per-command attribution without altering protocol retry behavior.
- Native host callbacks observe receipts before transport dispatch. FrameServer
  rewrites to the correct original command/client, retains routing across a
  caller waiting deadline, and never broadcasts identities or gives a new peer
  an old peer's receipts.
- Existing venue protocol/signing oracles, native strict checks, SDK/worker
  checks and relevant size/performance gates remain required. This plan adds
  no permission to weaken thresholds, fixtures or assertions.

Coverage claims must distinguish source-hook inventory, focused tests, actual
native execution and end-to-end SDK/worker receipt delivery. Until those gates
pass, generated-ID attribution remains planned work.
