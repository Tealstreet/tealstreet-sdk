# Nuke close sizing and ordering

The shared nuke methods send placement amounts in native contracts. Positive
`Position.contracts` values are base units, so the close helper reads each
position's current loaded market and calls the existing
`safe_cex_num::native_position_size`. Linear sizing uses base / contract size;
inverse sizing uses notional / contract size; the existing maximum with
`extra.contractsNative` preserves the maintained source policy. No new carrier
or rounding policy is introduced. The concrete venue keeps its own precision
adjustment and strict placement preflight.

`nuke` awaits cancellation before taking the close-position snapshot. A rejected
cancel is logged visibly and does not prevent closes. `nukePositions` never
cancels. Both log and skip a missing market, preserve later good positions and
continue after an individual placement failure. The existing four-attempt Store
retry rule, disposal behavior and NukeGuard lifetime remain in the shared owner.
The private close helper creates no independent latch or task; its caller owns
the guard across every close await.

`safe-cex-registry/tests/native_nuke.rs` constructs actual OKX through the
production registry/native host, seeds the production DefaultStore, calls the
public Exchange methods and observes signed native HTTP on loopback. It covers
three size policies, cancellation-before-close, cancellation failure, a missing
market followed by a good sibling, and dropping a pending close before a later
cancellation. Account-mode reads and batch-order formatting run through the real
binding; the fixture only supplies HTTP responses. Successful closes do not
invent Store position updates.

The companion `tests/nuke-source.cjs` executes the maintained TS nuke fields and
nativePositionSize utility, using the installed p-iteration implementation.
Its six source cases check placement amounts/flags, order, visible errors and
unchanged Store/guard state. The ordinary native target also requires that
source oracle. It does not substitute a whole-venue implementation.

Before the Rust change, the four behavioral tests compile and fail on the
master Rust bodies: small base-unit close is refused before HTTP, close precedes
cancel, cancellation failure escapes, and the unloaded-market error differs.
The source oracle and pending-close guard control already pass. Setup/compile
failures from constructing the fixture are not regression evidence.
