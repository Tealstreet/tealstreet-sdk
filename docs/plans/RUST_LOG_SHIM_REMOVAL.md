# Remove the engine log shim (PR6)

Branch from current feat/rust-rewrite; merge after coded SDK health PR5.

Migrate engine/startup diagnostics to stable codes at existing levels. Route
engine-only diagnostics through capped/filtering events and count rejected
empty catalogues/discovery rows. Replace remaining venue severity parameters
with Level, then remove Cx::log, LogLevel, Notice::Log and host wire log actions.
Keep host onLog callbacks for transport/command diagnostics. Native and Durable
Object output uses coded events. Migrate existing collectors without weakening
financial/state assertions or counting both old/new notices.

Verify focused engine/host and touched severity-helper venue tests, formatting,
Clippy, SDK host/worker regressions and typecheck/lint; CI owns release WASM and
full replay matrices. Independent review and the revert guard precede merge.
No deployments; safe-cex-server deployment remains Sam's call, before native
clients rely on Info+ venue events. Downstream cutover docs record the ordering.

The overseer's merge window holds this PR until lifted. Keep CI running.
