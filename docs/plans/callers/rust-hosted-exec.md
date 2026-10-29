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

Close, stop, break-even, trailing stop and move use the same prepared order
policy as buy/sell. Full and partial closes convert base positions to native
contract amounts and cap partial requests to that position. Explicit hedge
side is required for an ambiguous partial close. Stop/trail Full amounts remain
zero only on TS-metadata-supported venues; metadata inheritance and flags are
pinned against the TS source. Reduce-only clamp now follows the complete TS
metadata venue list instead of only Hyperliquid and Woo.

Global close all/longs/shorts deliberately attempts each independent position
after another position's failure, including Unknown, matching TS; no physical
write is retried. Scoped close and protective operations stop on failure.
Break-even cancels only same-side stop losses, never take-profits, and waits for
that cancel before replacement. After a confirmed cancel, any replacement
refusal/Unknown/preparation failure explicitly reports that the position is
unprotected; this is an intentional native safety message addition, without
retry. An underwater break-even that becomes a TP warns there is no stop loss.
Move requires exactly one open limit matching the real ID prefix and sends
only a rounded price amend, retaining size/side/reduce-only in the engine.

Protective/close scope tokens fail closed: an unrecognized explicit symbol
never becomes the focused position. Move and trigger prices require the entire
numeric token; native deliberately refuses TS parseFloat-prefix inputs such as
61k, 61,000 or 6100O rather than amending at a truncated price. Unprotected
break-even replacement errors name the actual symbol and say no stop is in
place in both the failed exec-result and the existing error notice channel,
not only the diagnostic exec-log. Duplicate completion cannot repeat the notice.

Order tools add cancel top/bottom/first/last, limit price ranges and side stops;
selector keywords cannot be captured by a listed market with the same base.
Bump issues price-only amendments, catches each independent order failure and
continues without replay, matching TS; its summary counts successful/failed
orders. Counts and price amounts are strict whole tokens, unlike TS parseInt /
parseFloat prefixes. Scale prepares every rung before one batch release,
preserves structured partial IDs/count/index, and never resends a failed batch
or tail. Native bound: 2–128 scale rungs (8x approved); 0/1/noninteger/oversized
ladders refuse whole before dispatch. Zero-size rungs and unknown explicit
symbols/modifiers also refuse before any batch is sent.

Swarm uses concurrent independent native market calls with TS equal/irregular
weight distribution and last-slice lot remainder. It waits for every slice,
collects fulfilled native IDs in declaration order, and reports partial versus
all-failed results without retrying a failed/Unknown slice. Every slice shares
the account credit pool. Native bounds (8x approved): 1–32 slices and enough
free account credits for the entire swarm before any slice is dispatched;
otherwise the whole swarm refuses. Irregular weights use the host random
source; native uses OS randomness and refuses before dispatch if unavailable.
Browser preprocessing budget/fat-finger state remains absent on this hosted
worker path, as in TS; no synthetic account state is introduced.


CLI chase and TWAP use the same injected account ChaserBook/TwapFactory as direct
ops. The constructor takes the server's trusted account name; no payload account
or client order id is forwarded. Chaser percent/USD sizing, reduce-only, captured
termination anchor, list and stop delegate to those shared instances and their
existing account bounds. TWAP duration is converted to minutes and uses the
direct factory's validation, state messages and lifecycle. Intentionally fixes
the TS hosted CLI no-op: its router dropped taskAction; native CLI twap now
starts exactly one real schedule. Shared algo stop/dispose policy remains owned
by the account runtime; EXEC never adds write retry or resume replay.

Sleep/wait use the shared account scope, including disposal/abort; no polling
request is sent to a venue. Conditions read fresh engine frames every 100 ms,
with TS bid/ask crossover semantics, position/hedge PnL, mark/index/funding/
premium, captured order-ID suffixes and nested both/either. Sleep/timeout/for
are capped at ten minutes; wait time HHMM UTC targets the next occurrence.
Native bounds nested conditions at sixteen levels. @variables still require
the absent hosted PreprocessStore and refuse; browser script preprocessing
remains client-owned. Wait/sleep failures deliberately stop their remaining sequential chain,
including parse/admission refusal and timeout. This tightens TS worker parity
to prevent a later trade on a false condition; other explicit semicolon steps
retain continue-after-error. Disposal prevents a sleeping
chain's successor from dispatching; no write is replayed on reconnect.

Read-only order/position/margin/spot/market/status/check/max displays use current
engine frames and the existing exec-log string envelope. Order/position filters
retain TS focused/all/side and open/creating semantics; web tables use aligned
nonbreaking-space columns. The focused position card retains native position,
entry/mark/PnL/liquidation/leverage data. Coin and loaded sections remain unknown
until delivered, distinct from a delivered empty or false section. No read-only
command fetches venue data. Classification reports unknown for missing or new
Bybit labels, and never advertises eligibility. Native spot uses the same aligned
web-table style as other displays rather than TS's terminal box/ANSI formatter.
Help metadata and case-insensitive aliases are generated from the registered TS
catalog; regenerate with algos/exec/scripts/generate-cli-catalog.py, and verify
with --check. The worker's symbol/session updates remain client-owned: each exec
payload supplies its focus. Hosted variable/alias/budget/simulation commands keep
the TS Worker's absent-PreprocessStore refusal; no second script state is created.
Manual socket reconnect remains outside hosted EXEC (explicit fixed refusal).

Tasks/kill inspect the same injected account chaser and TWAP books as direct
ops. TS ordering is retained (chasers, then TWAPs; bare kill takes the last active
row), with ID-prefix/all and existing aliases. No independent task book is added.
Nuke is one account-guarded pass: cancellation first attempts every fresh-store
symbol independently, even after another symbol is Unknown. It then re-reads
positions and issues reduce-only market closes for each current position,
including fills during cancellation, continuing after an independent Unknown.
This deliberately tightens TS close-first ordering and first-cancel-failure
short-circuit, per the post-merge money review.
No write is retried. The guard and account credit return on completion or abort;
disposal cannot release a later operation. CLI TS passes tries=3, which disables
its base retry loop; native preserves that single-pass behavior. Intentional,
ddi-approved difference: native waits for and reports final counts, per-symbol
failures and structured Unknown/partial IDs in exec-result, instead of TS immediate
optimistic success. Acceptance of a close/cancel command is not a filled/flat
readback claim. A concurrent nuke on the same account refuses before any send.
The wait barrier now resolves through the evaluator's same generated canonical
lookup, so adding a future wait/sleep alias cannot bypass the failure barrier.
