# Eager socket dial audit

The assigned `BitgetSecondaryPublicWebsocket.connectAndSubscribe` row remains
**OPEN** for CONNECTING-wrapper presence during a held native handshake. This
first shared repair closes the independently observable call-versus-poll gap.

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

## Remaining CONNECTING representation

NativeRuntime currently awaits the HTTP upgrade before returning WsSocket.
A separate compiled test changing the held-handshake presence expectation to
true fails at `SECONDARY pending handshake has the source CONNECTING wrapper`.
Starting the future eagerly cannot repair that state by itself. The follow-up
must expose CONNECTING presence and source send/close/dispose semantics while
preserving #1094's Error then Close(1006), URL admission errors, generation and
ended guards. No ACCEPT ruling or completed-row claim is made here.
