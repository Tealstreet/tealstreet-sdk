# Family CPU remeasurement on unchanged bytes

SDK CPU collection retains the strict original public measurements. For an
family CPU-only miss, it independently builds the authenticated base using
the pinned family build, retaining both packages and their metadata. The base
is the checked PR composition's first parent, or the authenticated master push's
actual first parent. A failed build or mismatched seal never grants a pass.

When WASM and JS glue (and package metadata bytes) match, planning and final
certification recheck the original producer's digest-checked artifact, archive
source/run/family, actual file hashes, common tools/flags and candidate binding
to the first runtime capsule. The disposition is "unchanged bytes, not measured"
and no second CPU measurement is issued. The initial strict report is retained
as diagnostic evidence. No synthetic Q, timings or two-run receipt is created.

Changed family bytes retain the existing eligibility, second-run arithmetic
and all bounds. Combined builds retain their existing path. Complete public
roster, original-attempt source/producer, raw correctness and memory checks
remain required. Proven unchanged bytes have no CPU verdict at any ratio,
including the first measurement; strict measurements stay diagnostic. No venue runtime, dependency, Cargo.lock, WASM budget
or numeric policy changes are involved. This implements ddi's extension of
cpu-cohort-marker-clock.md's Bitget byte-identity ruling to family remeasurement.

Regressions retain original medians from #2963 run 41617614920 (txflow) and
master run 41618845764 (OKX inverse). Each previously failed the frozen two-run
mean; each unchanged family now avoids that measurement only with a valid seal
proof. Collector controls pin exact candidate restoration after success and a
failed base build; failures cannot emit a completed proof.


Replay measurement jobs now use a full-history checkout, as the shared source
selector requires. At a depth-one shallow boundary, Git's %P output is empty
although the commit object and API both contain the two correct PR parents.
This caused the family selector's parent assertion to fail in #2967; the
separate paired Bitget job already had full history and passed. The parent and
tree checks remain unchanged, with explicit mismatch fields in the error.
A real Git regression retains the branch-updated-from-master merge shape.

The first-measurement control retains txflow's 17.924/20.579 ms CPU miss, beyond
the changed-byte eligibility bound. Only a valid byte proof accepts it; changed
WASM or glue still refuses it. Collection controls preserve private coverage,
ledger consistency, complete raw evidence and memory failures before any
unchanged-byte acceptance. Ddi approved this broader application in #2967.
