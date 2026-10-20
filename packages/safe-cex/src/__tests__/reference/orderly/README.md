# Orderly replay reference

Test-only legacy implementation frozen from commit `39cf3b70e6` (the unchanged production
source inherited from `feat/rust-rewrite`). Production registries
load the Rust engine. These files exist only for independent replay comparison
and baseline regression tests. Do not import them into production code.

The public snapshot transport uses the documented WebSocket request so public
keyless recordings can exercise both implementations; legacy signed REST
snapshots required credentials. All corrections are declared in KNOWN_TS_FIXES.
