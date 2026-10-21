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
its changes without re-integrating and rerunning. The held clock/login/routing
stack must finish before this dependent scheduling PR can merge. The later keyed-admission phase must preserve current
WS raw families, malformed identity coverage and staged initial-half authority.

Verification: all seven regressions fail against the original scheduling source
and pass with the correction. Bitunix/Binance native tests pass (427), together
with strict Clippy, formatting and bans. Independent implementation and fixture
reviews are clear. CI and the final landing guard remain required.

CI run39653858321 exposes 44 private checkpoint differences after reconciliation
can run sooner. Independent full raw audit confirms every missing row was already
terminal on the tape, and every open/common-ID row is completely equal at all 16
checkpoints. Exact declarations use tape SHA
`e8d90edafa93fabcbd9cefb0e84be32a077399e0e3a2f4a906999d9d48a3850c`
and these native terminal records:

| Native ID | Terminal time (ms) | Status | Affected checkpoint indices |
| --- | ---: | --- | --- |
| 1001 | 2400 | closed | 1, 2, 3, 4 |
| second | 3600 | canceled | 1, 2, 3, 4 |
| 1003 | 6000 | canceled | 4 |
| 1004 | 9200 | canceled | 7, 8 |
| 1005 | 9600 | canceled | 7, 8 |

The 44 literal keys include positional shifts at checkpoint4; no still-open row
is missing. Thirteen command results, seven writes and signature/protocol checks
agree. HTTP read counts increase with reconciliation, so total transport receipts
are not asserted identical. The overseer approved exact terminal-retention paths;
no tape, comparison, budget, integrity or threshold change is included.
