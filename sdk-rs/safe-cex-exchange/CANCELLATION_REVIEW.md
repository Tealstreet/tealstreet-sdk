# Owned-task cutover cancellation review

Reviewed integration parent `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`,
including `204da7be8d` and `9d087cd8ff`, before updating the cancellation inventory.
The sibling `order_operations.rs` pins were preserved.

`BaseExchange::dispose` sets disposed, closes `OwnedTasks`, unsubscribes Store
notifications, settles signer waiters, disposes the provider, stops the watchdog,
disposes sockets and removes listeners before the fallible Store reset. A reset
subscriber error reaches the caller after those cancellation steps. The new
`exchange_dispose_error_still_cancels_every_owned_task_and_rejects_new_work`
witness covers unpolled and pending owned work, interval and timeout handles,
capture release, rejection of new work, repeated disposal and absence of late ticks.

`OwnedTasks::finish` removes a registration before host cancellation.
`Slot::finish` marks terminal and takes its future, host handle and cleanup before
calling out; repeated cancellation is inert. `Driver` handles task discard,
completion during inline spawn and cancellation during polling without restoring a
terminal body. Existing registry witnesses cover sibling independence, replacement
preservation, owner Drop, self-cancellation and reentrant cancellation during error
reporting. These are bounded witnesses, not a proof over arbitrary host callbacks
or Rust panics.

`OwnedTasks::timer` rearms a repeating timer before reporting a callback error;
a one-shot completes and unregisters after reporting. The new
`exchange_listener_error_rearms_interval_and_releases_timeout` invokes the actual
`BaseExchange::emit` boundary, observes the native listener and reported error,
checks the next interval tick, and checks timeout and disposal capture release.
The independent Node timer corpus has five cases, ending at index 4 (`self-cancel`).

Sub-agent review found a real observer leak in `SafeCexHOW::new`: dropping a
polled token observer retained its waker in the independent task. The sentinel
witness failed before repair. The implementation now uses the existing
`waiters::Registration` to release only the abandoned observer. Token execution
continues; host task discard still wakes a surviving observer with its native
cancellation error. Separate tests assert both properties.

The first discovery census contained 143 creators versus 115 reviewed entries:
28 new owners and seven changed producers. The inventory was reconciled after
ownership review, retaining existing rows and annotations. Access-token wrappers,
journal/paging and cross-trade composition own local child futures without a
shared pending reservation. Synchronous callback setters and the inspected
`thread_local!` storage initializer are immediate. Registry, HOW observer and
socket guard definitions remain fingerprinted; socket ownership now also pins
`owned_tasks.rs`.

Recovery limitation retained from TypeScript: `BaseWebSocket.reconnectNow` calls
`connectAndSubscribe` before clearing `reconnecting`. A synchronous hook failure
leaves the latch set in both runtimes. The existing successor-listener test resets
that latch explicitly and does not prove automatic recovery. Likewise, source
`startWatchdog` schedules its successor after `auditSocketHealth`; a thrown audit
does not schedule another timeout. Neither behavior is included in a general
callback-error recovery claim.

Three independent sub-agent reviews covered the registry/disposal path, the new
local-future families, and HTTP/socket/HOW composition. The HOW repair received a
follow-up review with no blocking findings. Reviewers did not run Cargo; measured
gate exits and deliberate controls are recorded separately.

Manifest declaration references:
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
This slice does not change parity rows or make venue-completeness claims.

Post-merge follow-up at `f2229103d079a43a5b99c41b206b1deb6e51c102` includes
the merged cancellation review `850913aa64`. The dispose-close mutation was
repeated with disk headroom restored: the named native assertion failed (101),
then the restored exact test passed (0). Workspace and policy wasm commands,
the separate all-features exchange wasm commands, host clippy and their actual
exits are recorded in [the follow-up evidence](POST_MERGE_GATE_EVIDENCE.json).
The original disk-full attempt remains historical context, not a valid control.

After merging integration `fada0ab29c`, the eight commands were repeated at
`0b320701f8c211d7aba8c9e9ea43e3f954610363`, all exiting zero. The extractor
check also exited zero with no manifest drift. Those records, their explicit
cache/debug settings, and the independent merge-parent census are in the same
follow-up evidence. The mutation-control exits retain their original source SHA.

Integration merge-down `4bf3b26d8f` correctly failed the inventory on the changed
proxy source pin. Two independent sub-agent reviews compared it with `0b320701f8`.
The proxy lifecycle suffix, from `ProxyTokenHost` through EOF, is byte-identical;
`owned_tasks.rs` and `waiters.rs` are unchanged. Socket production changes are two
comments; remaining socket changes are tests. Six dependency pins were updated
only after that review and native witnesses. Added message decoding owns local
values synchronously. Its wire acceptance and error timing are outside this
cancellation-equivalence claim. The new numeric-output macro owner was inspected
and classified separately, without changing discovery or the validator.

The timer-drop control retained the timer slot deliberately. The exact native
assertion `discarded refresh TASK must release timer slot` failed (101), then
passed after restoration (0). An earlier incorrect test selector executed zero
tests and is retained as an invalid attempt. An exploratory combined native run
also failed three HTTP oracle setup checks for an absent isolated Axios dependency;
its successful cancellation witnesses do not make the combined command green.

The reviewed follow-up measurements at `caeba6b66f` are now attached in the
follow-up evidence, including the initial red pin, both invalid attempts, native
witnesses, valid timer mutation/restoration, and all eight actual broad gate exits.
The discovery floor is 144 creators, ending at the recorded `ws_parent` owner;
classification remains separate from semantic cancellation proof. The three HTTP
oracle setup failures remain historical results alongside the dependency-backed
retry; they are not erased or reclassified as successful runs.

The external numeric macro definition hash is retained as review evidence. The
immediate-owner gate pins invocation tokens and does not enforce that external
definition or its field-trait implementations against future edits. This is a
review dependency, not an evergreen macro-safety guarantee.

Integration `d4e20845a8` was subsequently merged down. All eight commands were
repeated at `89d01630011aee1222e836367f572848c9bdbdba`, each exiting zero.
The latest gate section records the merge parent, source comparison and tomllib
census independently. Native mutations, HTTP retry and extractor retain their
earlier measured SHAs; no workspace runtime suite or Linux CI result is claimed.

Review after integration `49a942e76143d6d1d44567edff59c12669d92627`

The source diff from the previous reviewed integration parent contains a synchronous Exchange recovery dispatcher, a protocol recovery opt-out before socket timer/state changes, diagnostic key removal, cross-trade inherited-key handling, and four numeric-output methods. Native sub-agent review found no new cancellation ownership defect in those five changed production files. This is a bounded cancellation review; it does not certify cross-trade economic behavior or numeric wire parity.

`BaseExchange::dispose`, the owned registry, socket disposal/scheduling/cancellation, task guards and resync guard bodies are unchanged. Disposal still closes the registry before later fallible callbacks. A false recovery policy leaves the existing timer registered to its socket; the true-policy cancellation path is unchanged. The new native witness covers an unpolled and a pending reconnect timer through this opt-out and subsequent socket disposal, including immediate registry release and socket release after dropping the caller handle and draining cancelled host work. Its exact measured result and mutation control belong in the gate evidence, rather than being inferred from this source review.

The fresh maintained AST export at merge `47f00ff760186e22e6d6722f5bda9b344d904086` exited0 and discovered148 creators: the prior144 plus exactly four `JsNumberOutput::check_number_output` methods for CrossTradeConfig, CrossTradeCopier, CrossTradeExecution and CrossTradWeightMode. Enumeration stops at the existing final `WsExchangeParent` creator. The first all-features classification run exited101 at the changed `audit_copier_positions` creator assertion. Both cross-trade audit bodies now handle inherited/missing symbols by appending an immediate result to the same local request Vec; placement children still belong to that Vec and its returned join_all future. Their measured producer tokens were reconciled after this ownership review.

The four numeric methods use the inspected exhaustive struct/enum macro arms. Primitive/String checks and Option/Vec/JsObjectMap recursion create no task or pending reservation; the two unit enums have no payload operation. They receive immediate classifications with actual producer fingerprints. As with the earlier BroadcastMessage row, invocation fingerprint coverage does not pin the external macro provider against future drift; that existing limitation remains explicit.

The reviewed whole-file guard dependencies now identify exchange.rs as `28a654aee4eb05040aea7c150583ab04bc96833480623c7bf633acf006314bd8` and websocket.rs as `b39f58844b69841b4d577be149fda9ffac135423d235fcd9e0f72ade2498b2be`. Only their two and three dependent guarded rows changed. The sibling order-operations and prior proxy pins remain unchanged. Source review supports these fingerprints; fresh all-features witness execution is a separate gate recorded at its own SHA.

The ModelField cutover changes both cross-trade audit predicates to compare their
property values, so absent and own undefined read alike. Request collection,
join ownership, reservations and guards are unchanged. BaseExchange changes only
prepared input signatures and model enum reads; disposal bodies and task guards
are unchanged. The maintained AST exporter supplies the two audit producer pins,
and the BaseExchange guard dependencies retain their whole-file pin after review.

The standard Error prototype initializer in value.rs is an explicitly reviewed,
source-pinned opaque thread_local macro inside its sole private accessor, matching
the parity indexer's supported storage pattern. Its initializer tokens are unchanged.
It builds data descriptors and a pure
stringifier, with an acyclic parent graph and no runtime/scope capture. It creates
no future, task, timer, waiter or in-flight slot. This is intrinsic data storage;
no discovery exemption was added. The globals constructor dispatch stays synchronous
and preserves its four existing Promise boundaries. The native Error witness
compares156 Node instance observations and actual Axios diagnostic descriptors.

The accessor relocation is covered by the full production rust_index regression,
which compiled and failed on the module-level macro. The opaque cancellation
classification moves to error_prototype and retains the same boundary obligation;
no indexer support exception or discovery exclusion was added.
