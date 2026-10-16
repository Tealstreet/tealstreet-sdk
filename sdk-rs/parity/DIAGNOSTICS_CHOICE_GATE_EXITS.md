# Diagnostics choice measured evidence

Original preparation parent:539ac8165b8e36ef009dc467123e69fe14e002e2.
Resumed integration parent:dcb88b8e64efc8da8ed831388c9adaec2609b77f.
The prior preparation excluded the then-unmerged venue stack; the resumed
integration parent includes it. These are measured exits at the named commits;
no earlier measurement is claimed numerically for a later commit. The sprint
handoff reports its exact delivery SHA and three gate exits with a separate
local receipt. This authored document records completed scoped evidence only.
All Cargo commands use CARGO_INCREMENTAL=0, jobs2 and one Cargo at a time.

| Commit | Command/scope | Exit | Observation |
| --- | --- | ---: | --- |
|148639c9d5ec525cfe0f4744ff77e15bc8cefac1 |cargo test -p safe-cex-store --test diagnostics_choices |101 |All six tests fail at native assertions; compilation succeeds |
|781393c60fc542e1a49001e628f920e64c1e4eaf |Same scoped target |0 |Six native tests pass before first source comparison |
|edc61fcf38b29409e31a7ca392fa6bf1f1de4279 |cargo test -p safe-cex-store --test diagnostics_choices --test remaining_utils, single test thread |0 |Six new plus four existing native tests; zero filtered |
|edc61fcf38b29409e31a7ca392fa6bf1f1de4279 |cargo test -p safe-cex-store --test diagnostics, single test thread |0 |Three existing tests, including legacy wire tail after native values |
|edc61fcf38b29409e31a7ca392fa6bf1f1de4279 |node parity/scripts/diagnostics-choice-source-gate.cjs |0 |Twenty actual-source cases; strict mode, actual lodash, fixed clock |
|edc61fcf38b29409e31a7ca392fa6bf1f1de4279 |Existing diagnostics_oracle.cjs with literal parent assertions |0 |Ordinary aggregate plus two signed-zero orders |
|2f5dcaeed567095cb805166227b13b72484accc2 |cargo check -p safe-cex-store --all-targets --locked |0 |Finished31.66s; zero warnings/errors |
|2f5dcaeed567095cb805166227b13b72484accc2 |cargo build --workspace --locked |0 |Finished1m42s; zero warnings/errors |
|2f5dcaeed567095cb805166227b13b72484accc2 |Same guarded twenty-case source command |0 |Completed twenty-case marker |
|2f5dcaeed567095cb805166227b13b72484accc2 |parity:extract:check through explicit YarnJS |0 |Output would update0 manifests,0 added,0 orphaned |

The writer ran after the nine-note edit and reported Updated0 manifests with
0 added/0 orphaned; no symbol arrays were hand-spelled. Independently parsed
old/new TOML over512 manifests finds exactly nine note edits, zero status,
target, layer, file or row movements. Selected three-file census is
11 ported /25 divergent /0 pending. The selected nine identities stop at
percentile; this does not resolve all findings in these files.

## Discrimination

The pre-repair six-test control stops at iteration6. Named native failures:
REST SEALED native existing request stamp; REST SEALED native existing proxy
stamp; REST SEALED native direct first write; REST SEALED native public wrapper
keeps first write; MERGE native selected identity after second lastUpdate read;
MERGE native selected second existing before dedupe effect. All fail101 before
repair and pass0 after repair. Added zero/-0/NaN characterization cases were
not in that initial failing version and are not credited to it.

Two deliberate source controls ran atedc61fcf38b29409e31a7ca392fa6bf1f1de4279:

| Iteration | Break | Named assertion | Mutated/restored exit |
| ---: | --- | --- | --- |
|1 |Empty successful source producer |DIAGNOSTICS SOURCE completed twenty-case marker |1/0 |
|2 |Replace maintained-source second timestamp read with cached1500 |SOURCE MERGE second read determines selected identity |1/0 |

Each altered file was restored bytewise before the restored run; no survivors
occurred in these two controls. They are source/producer controls, not additional
native101 mutations. The source floor ends at20 cases; native effect trace has
an independent literal fourteen steps. No serialized roundtrip is used.

Production and source instrumentation passed independent read-only sub-agent
review. Review required pricing supplied time and collapsed proxy presence,
which the declaration now includes. It did not establish native venue/HTTP
assembly or engine-wide NaN sorting conformance.

Not run for this finite unit: full test suites, all-feature builds, separate
wasm build/runtime/browser/FFI job steps, real-network/native host assembly,
Node26, canonical parity-check diagnostics or CI reruns. Workspace host lint
and the canonical wasm helper are separate claims, supplied with the exact
handoff SHA after they complete; neither is inferred from native tests.

## Resumed measurements

Branch:fix/rust-shared-diagnostics. The final build, host lint and canonical
wasm helper each run at the exact committed delivery SHA. Their separate local
receipt is supplied with the handoff; no post-gate documentation commit changes
that SHA. These completed scoped measurements belong to the earlier merge below:

| Commit | Scope | Exit | Completed observation |
| --- | --- | ---: | --- |
|e92069cd4e8861a0a720f66e3544ce0177b20ed6 |cargo test -p safe-cex-store --test diagnostics_choices --test remaining_utils |0 |Six regression plus four existing native tests; zero filtered |
|e92069cd4e8861a0a720f66e3544ce0177b20ed6 |cargo test -p safe-cex-store --test diagnostics rest_native_stamps_aggregates_and_bounds -- --exact |0 |One selected ordinary REST regression, two filtered |
|e92069cd4e8861a0a720f66e3544ce0177b20ed6 |node parity/scripts/diagnostics-choice-source-gate.cjs |0 |Twenty maintained-source cases, after native assertions |
|e92069cd4e8861a0a720f66e3544ce0177b20ed6 |Empty producer control |1/0 |DIAGNOSTICS SOURCE completed twenty-case marker; bytewise restoration |
|e92069cd4e8861a0a720f66e3544ce0177b20ed6 |Cached second timestamp read control |1/0 |SOURCE MERGE second read determines selected identity; bytewise restoration |

The repeated source-control enumeration stops at2 with no survivors. These are
source/producer controls, not additional native101 mutations. The six native
pre-repair failures at148639c9 and restored successes at781393c6 remain their
original evidence; current test bodies also include later presence cases.

For completeness, the interrupted preparation run at5c96c51c76bdb5a7ef6be0b72f65e7a14f231fc4
measured workspace build0 and host workspace lint0. Its wasm helper returned1
after SIGINT to the owned Cargo process to honor the higher-priority sprint
merge blocker. That was neither a lint failure nor a pass. It is not reused as
wasm evidence for this delivery. The later stack's two0exits belong to5121fb236a
and likewise do not cover this resumed shared-core code.

Read-only incremental sub-agent review passes production/test/source byte
identity with the reviewed preparation and the nine note-only changes. The
source and native controls are bounded utility proofs; production venue/HTTP
binding, Node26, full test suites, other wasm job steps, all-feature builds,
extractor reruns and CI reruns remain unmeasured for this resumed delivery.
The three requested delivery gates are separate from that NOT RUN list.
