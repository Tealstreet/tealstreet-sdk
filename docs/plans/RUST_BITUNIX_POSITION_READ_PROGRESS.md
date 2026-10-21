# Bitunix position reconciliation during continuous WS notifications

Reverified: position WS frames change only routing IDs, but every notification
bumps generation and postpones a one-second read. A REST response spanning such
a notification is discarded, so active accounts can keep stale financial state.

Retain existing generations for all WS/write events and strict command-owned
read.after/prepared fences. Add position_write_revision captured by Read and
advance it only on accepted leverage/margin/mode writes. Ordinary position reads
can survive WS-only generation changes when latest serial and write revision
still match. Keep REST and WS identity maps and routing precedence unchanged.

Coalesce position notification timers without postponing. Track current in-flight
position serial/time, prevent ordinary reads from superseding it before a bounded
10-second lost-read deadline, and release only the matching token on all reply/
failure paths. Notifications during a read request one follow-up after response;
lost reads can be replaced after the deadline. Do not remove serial fences or
allow ordinary polling to supersede fresh command preflight reads.

Regress sustained subsecond WS notifications during slow REST success, timer
non-postponement, changed quantity/empty snapshots, matching failure/retry,
bounded lost-read replacement/late-old-response isolation and strict owned
preflight rejection across WS activity. Existing leverage/margin/mode ACK and
symbol-settings race tests remain unchanged. Position financial mappers, row
isolation, order stream/snapshot reconciliation, auth and read keys are separate.

Fresh actual landing branch; no unlanded sibling code. Independent plan review,
red/green regressions, Bitunix/Binance native +strict checks, independent review,
CI, actual same-crate/shared source integration and separate exact merge guard.
Current source hold for1637 remains until overseer release.

Plan audit and implementation review CLEAR. Capture token before dispatch and
release on synchronous failure, HTTP/stream failure and completed response; late
old completions preserve a newer token/deadline. Two original-source regressions
fail, five regression families pass, and 373 combined native tests plus strict
Clippy/fmt/bans pass. Unchanged existing leverage/margin/mode ACK race tests also
fail when only new write-revision advancement is disabled and pass restored;
symbol-settings and owned preflight generation checks remain unchanged.
