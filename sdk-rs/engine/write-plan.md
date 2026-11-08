# Write outcomes journal

`safe_cex_engine::write_plan` is a reporting layer. The venue's legacy TypeScript
parity path owns validation, allocation, request bytes, ordering, continue/stop,
retry and compensating writes. The journal never decides which writes are sent.

1. Construct `WritePlan::new(Vec<Element>)` from the logical elements the venue
   wants to report. Construction is infallible and retains no wire payloads or
   validation callback. Unrecorded elements initially report `notSent`.
2. The venue dispatches through its existing transport owner. Record an in-flight
   element as `Unknown` with the venue's existing error classification, then call
   `record(index, result)` when its native outcome is known. Recording updates the
   matching logical index, including out-of-order responses and later corrections.
   An unknown index returns only `PlanError::UnknownElement`, a reporting mistake.
3. After a refusal or unknown result, the venue decides whether to continue,
   stop, restore protection or perform another legacy action. The journal remains
   writable. It has no next-write, settle, fail-stop or validation-gate API.
4. Use `outcomes()` to snapshot the facts or `attach(error)` to add them to a
   venue error. Attaching changes only `error.elements`; outcome, partial IDs,
   counts, text and every other native fact stay exactly as supplied. The engine's
   existing physical lifecycle remains authoritative for aggregate classification.

`ElementResult` retains its existing serialized alternatives: `placed` with
`orderIds`; `applied` with an `effect` and optional existing `targetOrderId` or
`positionKey`; `notSent` with optional `error`; and `unknown` with required `error`.
Applied effects are `cancel`, `amend`, `tradingStop`, `settings`, `deferredChild`
and `positionTrigger`. They do not manufacture newly placed order IDs. The venue
records the actual per-element facts; unattributable execution evidence remains
unknown, while definite accepted/refused rows retain their own outcomes.

`VenueError.elements` is optional and boxed behind one pointer. Its ordered JSON
array, handwritten `VenueErrorDetail`, host `ExchangeError.elements`, SDK
`SerializedCommandError.elements` and shared validator are unchanged from the
Applied contract. The Rust fixture is checked byte-for-byte, including existing
partial fields. Plain errors omit `elements`. `venue_message` remains boxed str
with unchanged JSON and the prior 112-byte native layout.

For engine-owned unproven HTTP seals, the venue may copy this journal through
its existing read-only `UnprovenEvidence::write_plan(&journal)` hook. It retains
the recorded prefix and untouched suffix and projects the engine's failure into
Unknown rows without changing the parent failure's classification or partial IDs.
Neither this hook nor `attach` grants send, completion or retry authority.
