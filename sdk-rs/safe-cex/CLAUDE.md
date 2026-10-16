# SDK facade

Exchange contracts live in [safe-cex-exchange](../safe-cex-exchange/CLAUDE.md).
This host-facing facade enables its `health` feature by default so terminal
credential classification cannot silently disappear from normal host builds.
Venue crates depend directly on exchange-core with default features off and do
not acquire the host-only regex classifier through this facade.

Production Balance currencies/coins use `types::object_map::JsObjectMap`: integer
indices enumerate numerically before other keys in insertion order, recursively.
Replacements keep positions; delete/reinsert moves non-index keys to the end.
`balance_key_order` wires the actual DefaultStore to the Store trait and checks
live Node order before and after serialization plus suffix-first selection.

`risk_recovery` executes actual TypeScript risk functions AND its catch/clear
postprocessor, comparing catchable Rust errors and real DefaultStore clearing
notifications. This proves the shared API supports recovery; the production
worker processor remains owned by P6 and is not implemented by this test.

Facade Store fixtures check every notifying write's Result. Counter-only
subscribers return success after recording their observation. Tests inspecting
patch fields first require StoreMutationError::Patch; notification failures must
not be flattened into field-validation errors to satisfy those assertions.
