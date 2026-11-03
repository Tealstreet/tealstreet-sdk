# Family CPU remeasurement on unchanged bytes

SDK CPU collection retains the strict original public measurements. For an
eligible family CPU miss, it independently builds the authenticated base using
the pinned family build, retaining both packages and their metadata. The base
is the checked PR composition's first parent, or the authenticated master push's
actual first parent. A failed build or mismatched seal never grants a pass.

When WASM and JS glue (and package metadata bytes) match, planning and final
certification recheck the original producer's digest-checked artifact, archive
source/run/family, actual file hashes, common tools/flags and candidate binding
to the first runtime capsule. The disposition is "unchanged bytes, not measured"
and no second CPU measurement is issued. The initial strict report is retained
as diagnostic evidence. No synthetic Q, timings or two-run receipt is created.

Changed family bytes retain the existing second-run arithmetic and all bounds.
Combined builds retain their existing path. Existing eligibility, complete
public roster, original-attempt source/producer, raw correctness and memory
checks remain required. No venue runtime, dependency, Cargo.lock, WASM budget
or numeric policy changes are involved. This implements ddi's extension of
cpu-cohort-marker-clock.md's Bitget byte-identity ruling to family remeasurement.

Regressions retain original medians from #2963 run 41617614920 (txflow) and
master run 41618845764 (OKX inverse). Each previously failed the frozen two-run
mean; each unchanged family now avoids that measurement only with a valid seal
proof. Collector controls pin exact candidate restoration after success and a
failed base build; failures cannot emit a completed proof.
