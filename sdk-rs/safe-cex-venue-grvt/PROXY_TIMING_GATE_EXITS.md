# Proxy header timing exits

Gate SHA: `90090cd4bb032f5b5cb3483029afcb0638f8e09d`.
Fresh parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
No unmerged feature branch was used as an ancestor.

Python tomllib: GRVT stays **113 ported / 51 divergent / 0 pending**.
**One FINDING closed**, by repairing eager invocation and classifying the
remaining fixed shared ABI as FORCED. No pending rows moved in this repair.

The resolver now enters before return and retains its selected proxy inside the
same pinned frame while pending. A counting-waker witness checks one invocation,
registry replacement, caller-waker registration and native header completion.
The source/native cancellation witness deliberately records their difference:
an abandoned source observer continues; a dropped borrowed native future stops.
Header order is also measured: source Zulu/Alpha, native Alpha/Zulu. Neither
remaining ABI difference is described as repaired or serialized convergence.
See the row's exact shared trait signature and
[canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

| Named gate | Exit | Measured result |
|---|---:|---|
| Scoped cargo check, safe-cex-venue-grvt, all-targets, jobs 2 | 0 | Finished, no compilation errors |
| Scoped builder_proxy test target, Node 24.20.0 | 0 | 5 passed |
| Same target, Node 26.7.0 | 0 | 5 passed |
| Compiled lazy-resolver substitution | 101 | eager resolver invocation; eager pending resolver; borrowed resolver dropped before continuation |
| Restored production, builder_proxy target | 0 | 5 passed |
| Existing source empty-URL mutation child | 101 | BUILDER-PROXY native url; enclosing permanent control passes |
| parity:extract writer | 0 | Updated 0 manifests; 0 added/orphaned symbols |
| parity:extract:check | 0 | Would update 0 manifests; 0 added/orphaned symbols |
| Direct rustfmt check, touched Rust files | 0 | No changes |

The lazy control substituted returning the unentered pinned frame. Production
was restored byte-identically to the gate SHA. The source floor is ten proxy/
network cases through index 9, with fixed counts and native fields before wire.
The pending-waker and discarded-observer witnesses are additional bounded cases.
Pinned package-manager JS entrypoint output was read for both extractor gates.
Compiler selection used SAFE_CEX_TYPESCRIPT_PATH; no laptop path is committed.

A read-only sub-agent review returned MERGE for code, tests and classification
at the gate SHA. It ran no cargo or tests; the listed exits were measured by the
root agent. No shared type or trait was changed.

Not run: workspace build, full GRVT/monorepo suites, host/wasm clippy, wasm
compilation/tests, real proxy credentials, live exchange calls or CI reruns.
