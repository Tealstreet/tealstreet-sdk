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
fail visibly. Commands include echo, prices, cancellation, leverage and buy/sell;
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
already in-flight account calls. Rejection produces exec-log and
the optional failed exec-result. No earlier command in that payload is executed.

Construct one `ExecFactory::default()` per account registry. Its shared call
credits count both sequential and parallel calls across payloads; command settlement and scope-aborted waiter disposal
return credits without replaying any engine write. Reusing a factory for a
different account port is refused. Separate requests keep separate chain state
and reply ownership, even though they consume the same account budget.

Buy/sell placement follows the worker CLI's USD/percent sizing and shared order
preparation, including inverse/fractional contracts, lot/tick rounding, attached
stops, reduce-only clamping and Bybit classification. The adapter requires the
classification section on Bybit before admitting placement; None means not yet
loaded and refuses, while an empty delivered section preserves TS unknown-label
behaviour. The server frame adapter owns preserving this distinction.

A comma-symbol command issues one native PlaceOrders (one order) at a time,
reads the current store before each successor, and stops its fan-out on a
failure. It never retries an Unknown or invents a client/native ID. Successful
capture data aggregates only returned native IDs, including genuine empty IDs.
Explicit semicolon chains still continue after a failed cancel/leverage, as TS
does; this is pinned with real buy successors. Every dispatched account call
holds one of 32 credits, including plain single-step messages; a full budget
refuses before that call is sent and returns the credit on settlement or abort.

Native symbol resolution deliberately tightens the TS resolver's bare-prefix
fallback: a base must be followed by nothing or a recognized quote/perpetual
suffix. ETHFI never routes to ETH solely because ETH is listed. Explicit
unresolved leverage or filtered-cancel scopes refuse instead of falling back to
the focused symbol. Null placement completions are Unknown and stop fan-out;
only an actual string-array result, including [], is a successful native shape.
