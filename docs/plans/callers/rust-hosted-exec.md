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

`safe_cex_exec::native::ExecFactory` now implements the shared `AlgoFactory`.
The server registers it and provides the account port; task lifetime uses only
the shared scope. Its adapter preserves cancellation symbols in typed engine
commands and maps native partial IDs into the existing serialized error fields.

Bare semicolon routing deliberately continues even after a failed cancel or
leverage command, matching TS. It is not a stop-on-error transaction. ID suffix
cancellation also deliberately spans symbols and permits multiple matches,
and empty filter selections still call cancelOrders, exactly as TS does.

Approved native resource difference: reject the whole payload before dispatch
above 16 KiB UTF-8 command text, 128 nonempty steps, or 32 parallel slices plus
already in-flight parallel calls in this Exec. Rejection produces exec-log and
the optional failed exec-result. No earlier command in that payload is executed.

Construct one `ExecFactory::default()` per account registry. Its shared parallel
credits span payloads; command settlement and scope-aborted waiter disposal
return credits without replaying any engine write. Reusing a factory for a
different account port is refused. Separate requests keep separate chain state
and reply ownership, even though they consume the same account budget.
