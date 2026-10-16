# Facade composition gates

DisplayedPosition producer/model seam (2026-10-12): the live TypeScript
DisplayedPositionsPostprocessor emits one real row with key and one fake row
without it. The facade decodes both through Vec<DisplayedPosition> and compares
all key sets, array order and numeric bits after serialization. It does not
assert object-key iteration order. key is Some on the real row and None on the
fake row, and None serializes as absence, not null.

Negative: remove the optional key field. safe-cex-types still builds, but
facade displayed_position_wiring fails specifically with E0609/no field key.
Restore the field and the actual producer round-trip passes. Logs:
`/tmp/p4-key-negative-model-build.log`, `/tmp/p4-key-negative-facade.log`,
`/tmp/p4-key-restored-facade.log`, `/tmp/p4-displayed-wiring-values.log`.

Touched types/numeric/store/facade packages passed fmt, strict all-target clippy,
all-target tests including ignored live oracles, and wasm builds; extraction,
parity-check and parity-report passed. Logs: `/tmp/p4-seams-{fmt,clippy,tests,wasm,
extract,parity,report}.log`. Facade formatting, unused-code and wasm-only
compile_error controls each failed as intended, with restored runs passing
(`/tmp/p4-facade-{fmt,clippy,wasm}-{negative,restored}.log`). Manifest gates retain
independent missing-row/missing-target controls recorded in types/store evidence.

Production Store seam: DefaultStore now owns interior mutability and directly
implements Store. production_store_wiring constructs Rc<DefaultStore>, passes
it to the real BaseExchange::new, and proves a core mutation reaches that same
instance. No store adapter or test-only projection is used.

Negative: remove the Store impl and its dependent PublicDataStore impl. The
production store crate still builds; the facade fails E0277 specifically at its
constructor because DefaultStore does not implement Store. Restoring the impl
passes. Logs: `/tmp/p4-seam-negative-store-build.log`,
`/tmp/p4-seam-negative-facade.log`, `/tmp/p4-seam-restored-facade.log`.

Native workspace CI runs the non-ignored constructor test. Types CI explicitly
runs facade and shared-model tests with include-ignored, after root dependencies
are installed. The central CI audit rejects removing this step or its ignored
fixture flag; all49audit tests pass (`/tmp/p4-seam-ci-audit.log`). The existing
frozen terminal corpus now executes production DefaultStore, and its complete
store source differential still pins49frames.

shared_venue_utilities uses the public facade aliases for all3 named venue
utilities. An inserted sort in calc_order_book_total fails the unsorted-array
assertion. Store/facade all-target tests, fmt, strict clippy, wasm and parity
controls pass; details in safe-cex-store/GATE_EVIDENCE.md.
