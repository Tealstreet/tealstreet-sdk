# Eager socket dial audit

The assigned `BitgetSecondaryPublicWebsocket.connectAndSubscribe` row is **FIX**
through two shared repairs: eager preparation/dial startup and the native
CONNECTING handle described below. The initial eager-only repair left wrapper
presence during a held HTTP upgrade open.

The maintained whole-class oracle `scripts/secondary-public-capture.mjs` checks
18 unchanged family/network/index scenes and synchronous wrapper presence.
`tests/secondary_public.rs::secondary_connect_installs_ready_socket_before_polling`
calls the real secondary class and BaseWebSocket, checks all three family URLs,
initial topic keys, the immediate market-change path and exact outgoing fields.
The original production code compiles and fails its named pre-poll assertion;
the eager repair passes. Backpack's constructed production public socket has an
independent equivalent RED/GREEN in `tests/disposal_binding.rs`; its maintained
source member and URL declaration execute in `connect-prefix-source.cjs`.

`BaseWebSocket::start_prepared_websocket` now starts the already registered body
through shared `eager::start_eager`. `OwnedTasks::start_eager` reuses the same slot
and Driver as the deferred path. Registration precedes the prefix. The registry
owns the actual preparation/dial future; cancelling it drops pending I/O
synchronously and wakes the shared driver to reap the completed registration.
Dropping a returned waiter alone never cancels a Promise producer. Existing
non-eager tasks, generation guards, guard Drop, and error dispatch remain.

Three core registry regressions cover eager prefix/registration, immediate I/O
release and driver reaping, ready completion, reentrant close, inline hosts and
host discard. The existing REST-token preparation, superseded generation,
failed preparation, no-dial completion and disposal witnesses stay enabled.
The former all-host-task cancellation assertion now checks the actual dial Drop
before the scheduler runs: the canonical shared microtask driver may serve
other tasks and must not be cancelled wholesale. Its weak-owner/no-stale-handle
assertions remain.

`scripts/eager-dial-controls.py` independently makes the production call lazy,
leaks the cancelled I/O body, and suppresses the cancellation wake. Every mutant
must compile and fail its named production witness, then exact byte restoration
must pass the same test. The lazy mutant is checked through both venues.

## CONNECTING representation follow-up

The original NativeRuntime awaited the HTTP upgrade before returning WsSocket.
A separate compiled test changing the held-handshake presence expectation to
true fails at `SECONDARY pending handshake has the source CONNECTING wrapper`.
Starting the future eagerly cannot repair that state by itself. The shared follow-up now exposes one stable CONNECTING handle, drives the one
transport handshake through canonical eager OwnedTasks, and keeps #1094's
network Error then Close(1006), admission origins, generation and ended guards.

`safe-cex-registry/tests/native_connecting_socket.rs` constructs the actual Bitget
parent and secondary owner, uses a real held local HTTP upgrade, and invokes
onMarketsChanged before upgrade completion. The original code compiled RED at
`CONNECTING Bitget market callback retains the source-selected topic before
upgrade`: native topics were empty instead of `["NEW"]`. The repair observes the
same topic, CONNECTING warning, reconnect, Error and Close(1006) effects as the
maintained whole classes over an actual Node WebSocket in
`connecting_socket_source.cjs`. Direct send preserves InvalidStateError and the
source message. Close preserves Error-before-Close, no Open and the measured Node
CLOSING-after-close quirk. Other real handshakes prove same-handle upgrade/frame
delivery, pending I/O release and suppression after termination/final drop.

Core's native connecting tests independently prove immediate socket-owner
cancellation and event-waiter waker release without cancelling a held socket.
`scripts/native-connecting-controls.py` in exchange-core disconnects production
state, send error kind, close I/O cancellation, close wake, final drop and waiter
registration cleanup, requiring compiled named RED and exact restore GREEN.
