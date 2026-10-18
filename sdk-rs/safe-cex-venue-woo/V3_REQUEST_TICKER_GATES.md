# V3 request input and ticker scope

The inactive account/private-order/write closure and request DTO were retired.
See ACCOUNT_WRITE_REVIEW.md; historical counts below describe the pre-retirement
audit, while the runners select only the remaining owners.

The unused request preparer, authentication input and prepared-output descriptor
are retired after caller review. Only WooV3RequestInput remains because the
inert account/algo/regular/position-mode/websocket builders still consume it.
Its method, endpoint, query/body number bits and map-only refusal checks remain.

The shared source/native fixture retains every quote and ticker input and
assertion: five surviving records, ten normalizations, fifteen positive-price
checks and fourteen ticker cases. Source BBO/Trade referents, frozen ticker/info,
owned native snapshots and field-level binary64 assertions remain observable.
The removed seven query and 23 preparation cases tested only the deleted port.
They are not counted as retained coverage or as a repaired behavior.

The control script retains the two ticker/quote controls and removes the orphaned
request control. Its commands use the pinned SDK toolchain and one Cargo job.
No new mutation receipt is claimed for those mechanically preserved controls.

FIX_AUDIT_LEDGER.md rows 1–2 record the caller and constructed-exchange evidence.
No V3 request route is enabled and no TypeScript production source is changed.
