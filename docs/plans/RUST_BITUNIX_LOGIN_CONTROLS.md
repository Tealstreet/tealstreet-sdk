# Bitunix private login control visibility

Private message handling currently returns before inspecting `op` when `data`
is absent/null, silently dropping login responses. Handle login controls before
that guard, parsing only the existing typed `data.result` boolean. Explicit false
retains the current rejection diagnostic; missing/null/wrong-type results get a
sanitized malformed-stream warning rather than an invented key rejection.

Keep the current two-second same-socket retry policy, coalescing repeated bad
responses without postponing its deadline. Bind its timer to the originating
private socket; success clears it and retired epochs cannot send on a replacement.
Integrate actual socket-lifecycle #1639 before merge, preserving its retirement
cancellation and first-real-pong watchdog compatibility. No permanent WS code,
new deactivation, login timestamp units, signature resend or write retry changes.

Regress missing/null/empty/wrong-type result, explicit false, success cancellation,
coalesced deadline and close/replacement/late timers. Prove original failures.
Run focused Bitunix/Binance native + strict Clippy/fmt/bans, independent reviews,
CI and a separate exact guard. Merge only after preceding Bitunix source queue.

Independent plan and implementation reviews CLEAR. All four regression families
fail the original implementation and pass the correction: malformed-control
visibility/retry, success cancellation, coalesced deadline and retired timer
ownership. 382 combined Bitunix/Binance native tests and strict Clippy/fmt/bans
pass. Actual #1639 integration remains mandatory before merge.
