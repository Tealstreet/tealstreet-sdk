# Orderly replay reference

Test-only legacy implementation frozen from commit `39cf3b70e6` (the unchanged production
source inherited from `feat/rust-rewrite`). Production registries
load the Rust engine. These files exist only for independent replay comparison
and baseline regression tests. Do not import them into production code.

The public snapshot transport uses the documented WebSocket request so public
keyless recordings can exercise both implementations; legacy signed REST
snapshots required credentials. All corrections are declared in KNOWN_TS_FIXES.

Public replay also preserves broker suffixes in market IDs and routes each
market's funding topic on separate sockets of at most 90 topics, matching the
venue's documented topics and observed 100-topic limit. These are explicit
comparator transport/mapping corrections, enabled by `replaySnapshots`.
Private comparison derives guarded corrections from the input tape alone:
execution deduplication, missing order fields, terminal parent/child state,
selected-mode settings, native root placement IDs and inherited query stubs.
Unexpected legacy values and protocol failures still fail the gate.
