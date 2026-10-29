# Hosted EXEC runtime

Owner: codex-zs5e83. The `safe-cex-exec` crate is a leaf consumed by the shared
per-account algo runtime. This first slice does not enable native hosted users.

The adapter supplies fresh engine-derived TS store fields, existing callMethod
translation, one completion per token, and existing exec-log/exec-result event
wrapping. It calls dispose on the starting socket's close or account disposal.
No in-flight write is canceled, retried or replayed by EXEC. Native structured
errors remain intact in errorDetails, including outcome and partial IDs.

Semicolon commands wait sequentially and continue after handled failures, as
the worker router does. Prefix-@ commands run concurrently and never replace
the last sequential result used for reqId/capture writeback. Unknown verbs
fail visibly. Initial commands are echo, prices, cancellation and leverage;
full catalog parity, replay scenarios and server registration remain required
before hosted activation. Chaser and TWAP entry points use their own Rust ports.
