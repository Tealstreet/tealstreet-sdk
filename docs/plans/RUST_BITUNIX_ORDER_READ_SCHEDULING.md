# Bitunix order read scheduling

Order/TP-SL notifications reset a shared one-second timer on every event. Slow
REST reads can also be superseded on every scheduled refresh. Fix scheduling as
a separate prerequisite to the remaining keyed WS-vs-REST admission work.

Keep independent in-flight serial/time/stall-timer tokens for regular and TP-SL
reads. Ordinary refreshes coalesce against each current ordinary or command-owned
read for ten seconds; each endpoint independently replaces a lost read. Capture
its token before signing so synchronous failures release it. Completion/failure
releases only the matching serial, and retired timers/replies cannot retire a
newer token. Shared notifications arm one non-postponing one-second follow-up;
completion coalesces dirty work into that cadence rather than immediate loops.

Preserve full existing generation/write/preflight checks, raw-family financial
admission, two-half initial readiness, native metadata and execution accounting.
This scheduling phase does not close the remaining global WS-generation finding:
a later phase must safely admit healthy REST peers while retaining newer valid
WS families and strict trading prerequisites. No retry of an owned write.

Regress continuous subsecond notifications, slow reads at both endpoints,
independent lost-read replacement, late old responses/failures, synchronous or
transport failure release and command-owned token protection. Prove original
failures. Run Bitunix/Binance native and strict Clippy/fmt/bans, independent plan
and implementation reviews, green CI and separate own-only merge guard.

Fresh branch from actual lifecycle landing77a5e5faaec. Integrate exact reviewed
#1650 head27430679ef9 as a genuine account/lifecycle/read-admission dependency;
this PR cannot merge before the preceding stack lands with that head, or after
its changes without re-integrating and rerunning. Do not alter the eight heads
already running CI. Full keyed-admission design: reviewed read-only analysis in
/tmp/bitunix-order-read-progress-design.md (continuity evidence, not a dependency).
