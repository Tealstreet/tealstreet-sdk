# Full-plan write gate

`safe_cex_engine::write_plan` owns no transport, signing, nonce, retry, cancel or rollback authority. Venue drivers keep those responsibilities and submit their fully materialized, immutable wire payloads to this helper.

1. Construct `Vec<PlannedWrite<T>>`. Each physical write carries its payload and the logical `Element`s it can affect. A batch carries multiple elements. Element indices are consecutive in dispatch order, starting at zero; client order IDs are optional correlation evidence.
2. Call `WritePlan::validate(writes, validate_fn)`. The pure callback checks every row at exact rounded wire values, including quantity/price limits, grids, notional and batch counts. The callback visits every payload before construction succeeds. An invalid plan returns `VenueError` with outcome `notSent` and a complete `elements` array of unsent entries, with the failed write's validation error attached.
3. At physical dispatch, call `next_write()`. It returns `(write_index, &PlannedWrite<T>)` and makes that write pending/unknown. Payloads cannot be released twice. No further write is available until settlement.
4. Call `settle(write_index, Vec<ElementResult>)` for proven per-row outcomes, in that physical write's row order. Preserve all mixed batch successes and failures. `Placed { order_ids }` requires a nonempty array of nonempty native identities. `Applied { effect, target_order_id, position_key }` proves an accepted effect without a newly created order ID and permits continuation. Its closed effect enum is `Cancel`, `Amend`, `TradingStop`, `Settings`, `DeferredChild` or `PositionTrigger`; optional targets refer only to existing state and must be nonempty when supplied. Keep client-only correlation in `Element.client_order_id`. A definite venue rejection is `NotSent { error: Some(rejected_error) }`; its nested error retains `outcome: rejected`. Missing/unclassified evidence is `Unknown`, never a resend signal. If an entire request failed without row evidence, use `fail(write_index, error)` instead. When that error contains accepted partial IDs without attribution, every dispatched row remains `Unknown`; the untouched suffix remains `NotSent`. With attribution, settle accepted rows as `Placed`, definite refusals as `NotSent`, and uncertain rows as `Unknown`.
5. Any refused or unknown row permanently closes dispatch of the untouched suffix. The helper does not repair anything. Call `failure(error)` to seal dispatch and attach complete indexed outcomes plus accepted IDs to the terminal error. IDs from nested failed-write evidence are retained at the legacy top level, and existing partial counts are preserved. A placed/applied prefix, accepted partial IDs or unresolved dispatched write forces the aggregate outcome to `unknown`, so it cannot imply that the whole intent is safe to resend.

`outcomes()` returns a snapshot. Before dispatch, entries are `notSent`; while a write is pending, its entries are `unknown`; after settlement they retain only the evidence supplied by the native decoder. Invalid settlement (wrong write, wrong row count or missing placed identity/empty applied target) changes no journal entries and keeps dispatch blocked, so the caller can fail conservatively.

The helper validates the complete plan supplied by the venue; it cannot detect a child the planner omitted. Adopters must include every lot, batch, TP/SL child and predictable cancel-replace write before the first transport write. Dynamic account changes still require the venue's existing final release checks. Engine lifecycle ownership remains authoritative for actual transport release.

## Public error shape and NX

`VenueError.elements` is optional. When present it is an ordered JSON array:

```json
[
  {"index":0,"clientOrderId":"first","status":"placed","orderIds":["native-0"]},
  {"index":1,"status":"applied","effect":"cancel","targetOrderId":"old-order"},
  {"index":2,"clientOrderId":"second","status":"notSent","error":{"message":"refused","outcome":"rejected"}},
  {"index":3,"clientOrderId":"third","status":"notSent"}
]
```

Unknown rows use `status: "unknown"` and retain their nested error with `outcome: "unknown"`. `clientOrderId` and an unsent row's `error` may be absent. Existing `partial` and legacy error text remain compatible. The transparent `ElementOutcomes` Vec wrapper is boxed through one pointer, keeping the elements handle to one pointer. Stored `venue_message` is `Option<Box<str>>` (identical JSON) to keep `VenueError` within its prior 112-byte native layout, including dependent tuple and allocation budgets. It dereferences to the outcome slice; JSON remains an array.

This JSON is exposed by the wasm host's `ActionOut::Done.detail` and native host detail serialization. It is not a ts-rs generated type. The handwritten `VenueErrorDetail.elements` declaration is checked against the Rust serialization fixture. The host `ExchangeError` and SDK commandOutcome allowlist both use `writePlanElementsOf` to validate and clone the array. Placed IDs reject empty arrays, empty strings and sparse arrays. Applied rows reject created `orderId`/`orderIds` fields, unknown effects and empty target references. Unknown nested outcomes or unattributed accepted IDs normalize conservatively to Unknown. Malformed rows discard the complete field; nested errors copy only declared venue fields, and cyclic/deep error graphs are refused. NX approval remains required for downstream consumers.

The engine's terminal failure projection preserves the complete plan's `elements` while retaining its existing authoritative physical aggregate classification and native failure text. This matters because its native-error selection otherwise replaces the fallback carrying the plan journal.

For engine-owned unproven HTTP seals (including 5xx), call `UnprovenEvidence::write_plan(&plan)` from the venue's existing read-only `unproven_http_evidence` hook. It copies earlier placed and unissued rows and gives pending Unknown rows the engine's classified failure. It grants no send or completion authority; normal engine retirement still discards the venue job.

## Reporting without dispatch policy

Legacy dispatchers that continue after a refusal can record the shared
`ElementOutcome`/`ElementResult` values directly. `ElementResult::failed` applies
shared conservative failure classification without releasing or stopping a
write. `UnprovenEvidence::write_plan_outcomes` accepts such a snapshot and applies
the same engine-owned HTTP classification to pending Unknown rows. Neither API
validates payloads or changes continuation. AsterDEX uses these reporting APIs;
its existing venue/engine dispatch rules remain authoritative.
