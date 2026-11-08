# Full-plan write gate

`safe_cex_engine::write_plan` owns no transport, signing, nonce, retry, cancel or rollback authority. Venue drivers keep those responsibilities and submit their fully materialized, immutable wire payloads to this helper.

1. Construct `Vec<PlannedWrite<T>>`. Each physical write carries its payload and the logical `Element`s it can affect. A batch carries multiple elements. Element indices are consecutive in dispatch order, starting at zero; client order IDs are optional correlation evidence.
2. Call `WritePlan::validate(writes, validate_fn)`. The pure callback checks every row at exact rounded wire values, including quantity/price limits, grids, notional and batch counts. The callback visits every payload before construction succeeds. An invalid plan returns `VenueError` with outcome `notSent` and a complete `elements` array of unsent entries, with the failed write's validation error attached.
3. At physical dispatch, call `next_write()`. It returns `(write_index, &PlannedWrite<T>)` and makes that write pending/unknown. Payloads cannot be released twice. No further write is available until settlement.
4. Call `settle(write_index, Vec<ElementResult>)` for proven per-row outcomes, in that physical write's row order. Preserve all mixed batch successes and failures. `Placed { order_ids }` requires native identities. A definite venue rejection is `NotSent { error: Some(rejected_error) }`; its nested error retains `outcome: rejected`. Missing/unclassified evidence is `Unknown`, never a resend signal. If an entire request failed without row evidence, use `fail(write_index, error)` instead.
5. Any refused or unknown row permanently closes dispatch of the untouched suffix. The helper does not repair anything. Call `failure(error)` to seal dispatch and attach complete indexed outcomes plus accepted IDs to the terminal error. A placed prefix or unresolved dispatched write forces the aggregate outcome to `unknown`, so it cannot imply that the whole intent is safe to resend.

`outcomes()` returns a snapshot. Before dispatch, entries are `notSent`; while a write is pending, its entries are `unknown`; after settlement they retain only the evidence supplied by the native decoder. Invalid settlement (wrong write, wrong row count or missing placed identity) changes no journal entries and keeps dispatch blocked, so the caller can fail conservatively.

The helper validates the complete plan supplied by the venue; it cannot detect a child the planner omitted. Adopters must include every lot, batch, TP/SL child and predictable cancel-replace write before the first transport write. Dynamic account changes still require the venue's existing final release checks. Engine lifecycle ownership remains authoritative for actual transport release.

## Public error shape and NX

`VenueError.elements` is optional. When present it is an ordered JSON array:

```json
[
  {"index":0,"clientOrderId":"first","status":"placed","orderIds":["native-0"]},
  {"index":1,"clientOrderId":"second","status":"notSent","error":{"message":"refused","outcome":"rejected"}},
  {"index":2,"clientOrderId":"third","status":"notSent"}
]
```

Unknown rows use `status: "unknown"` and retain their nested error with `outcome: "unknown"`. `clientOrderId` and an unsent row's `error` may be absent. Existing `partial` and legacy error text remain compatible. The transparent `ElementOutcomes` Vec wrapper is boxed through one pointer, keeping `VenueError` below its existing 128-byte stack budget. It dereferences to the outcome slice; JSON remains an array.

This JSON is exposed by the wasm host's `ActionOut::Done.detail` and native host detail serialization. It is not a ts-rs generated type. The binding follow-up on this PR adds the matching handwritten `VenueErrorDetail.elements` declaration and checks it against the Rust serialization fixture. NX approval remains required for downstream consumers; the current host `ExchangeError` constructor does not yet expose these rows on the JavaScript error instance.

The engine's terminal failure projection preserves the complete plan's `elements` while retaining its existing authoritative physical aggregate classification and native failure text. This matters because its native-error selection otherwise replaces the fallback carrying the plan journal.
