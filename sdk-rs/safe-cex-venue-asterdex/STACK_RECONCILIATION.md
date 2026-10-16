# Sprint stack reconciliation

Branch:merge/asterdex-stack.
Original integration parent:539ac8165b8e36ef009dc467123e69fe14e002e2.
The requested heads were merged in ascending PR order:

| PR | Head | Local merge |
| --- | --- | --- |
|7859 |2fa27b3bf279b871cd146409f951b8085dcf944d |b25ede1e99 |
|7871 |69a0b85cc2087faeee60ed65bab23f3db2b11bfa |6701e20248 |
|7885 |07c3d6ee391d14606bdcd8822d49c327bea9a460 |5a7ac1ea4f6fa025303be17ad171f707a0968b2b |

The first local merge's conflict helper accidentally collapsed stream rows.
The next commit restored complete three-way Git text and reconciled individual
hunks; no history was rewritten. Final independent TOML checks over512 files
retain every source-row identity and order. The six numeric and seven stream
notes cite the newer integration audit as reconciled historical scope. Its
other24 Aster findings are byte-identical, remain open and are not suppressed.
Manifest files were reconciled, never regenerated; no target array was authored.

The final Aster/GRVT crate code, tests and prior evidence files are byte-identical
to7885. The GRVT proxy repair is already an ancestor of that PR. Integration's
seven expanded BuilderHost mappings remain present; only the stale proxy-timing
finding is replaced by its reviewed repair annotation. Every source Aster/GRVT
GATE_EVIDENCE.md history remains verbatim. Older helper-only descriptions refer
to their recorded historical revisions, not the current runtime assembly.

Measured status movements against the integration parent:9pending→8ported and
1divergent, plus4divergent→ported (generateV3Signature, AsterDEXKlineResponse,
mirrorCumulativeOrderFill, handleKlineUpdate). Exactly13status moves; no others.
Final venue census: Aster142ported/89divergent/0pending with24open findings;
GRVT113ported/51divergent/0pending with0open findings. No new runtime behavior
repair is claimed by this reconciliation, and no old gate exit is transferred.

The shared declarations remain canonical at
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

The handoff supplies the exact committed SHA and separately measured workspace
build and host workspace clippy exits in a local receipt. No push, CI rerun,
new scoped test or mutation execution is claimed here. The latest sprint request
specifies these two gates; wasm and the extractor are not rerun for this stack.
The unfinished diagnostics slice stays on its own local branch and is not included.
