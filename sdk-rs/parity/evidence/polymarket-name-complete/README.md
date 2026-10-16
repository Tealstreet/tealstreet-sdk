# One complete callable-body calibration

Selected row: `PolymarketExchange.get name`, mapped to
`safe-cex-venue-polymarket/src/spot_contracts.rs::name`.
This row was already `ported`; it is **outside** the frozen 242 divergent rows.
Proving this row does not change their 0/242 complete-function evidence count.
No manifest status or note is changed.

## What a complete-function claim requires

A row-level proof needs a precise source/native invocation and observation
relation, pinned implementations, configuration and dependencies, coverage of
every admitted input/state class, and matching completion and externally visible
effects. Relevant observations include values and errors, missing/null fields,
numeric bits, order and alias identities, state/I/O effects, and asynchronous
prefix/suspension/cancellation behavior. A dependency protocol or mock is not a
proof of that dependency. A known mismatch on a valid input invalidates the
unqualified claim; accepting the mismatch in a test or dropping the case does
not repair it. Finite examples require a completeness argument, not a large case
count. Native values must be independently observed, and controls must show the
proof instrumentation rejects falsification. Review and current runtime receipts
must cover the exact input revision; later changes require renewed evidence.

Those are proof obligations, not a new representation declaration. Canonical
contracts remain in MANIFEST-SPEC
[14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../../MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../../MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../../MANIFEST-SPEC.md#divergence-taxonomy) and
[19](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

## Exact claim for this row

After successful ordinary module/crate initialization and successful call entry,
invoke the original evaluated source getter with any already-evaluated receiver
and argument list the runtime can pass. Relate that entered invocation to the
zero-argument native `name()` call.
Both complete synchronously, without suspension, body-generated external reads,
writes, calls, scheduled work or exceptions. The source result is the primitive
string `polymarket`; the native result is the real unit variant
`ExchangeName::Polymarket`. The supplementary shared enum wire tag is checked
only after native/source assertions. No known-difference case is accepted,
filtered, refused or normalized into a pass.

This is complete **callable-body parity at the existing mapped row boundary**.
It is not SDK instance construction, arbitrary `obj.name` property dispatch,
function-object reflection or cross-language ABI identity. Caller-side argument
evaluation, call setup/argument-stack limits and module initialization are
separate operations, not silently omitted body inputs. Synchronous/no-yield ordering is covered; equal wall-clock
duration is not asserted. The argument assumes normal JavaScript/Rust language
semantics and working compilers/runtimes, not process survival under resource
exhaustion, unsafe memory corruption or runtime instrumentation. No theorem
prover has verified those trusted foundations.

## Why the input coverage is complete here

The maintained TS source AST must contain exactly a zero-parameter getter whose
only statement returns one string literal. The **full original module and real
imports** are loaded with `tsx`; the actual prototype descriptor's getter is
called. Its runtime `Function.prototype.toString` is parsed independently and
must have the same closed literal body. There is no extracted replacement class,
copied reference function, dependency stub or fallback execution path.

The native source AST must contain a zero-parameter, nongeneric, nonasync safe
Rust function, with no semantic attributes, whose sole expression constructs a
unit variant of the real shared `ExchangeName` enum. The actually linked native
function must return `ExchangeName::Polymarket`, checked by a native pattern
before launching Node or doing serialization.

Given those exact syntactic forms, neither body can examine inputs or mutable
state, branch, invoke a dependency, throw or suspend. Every source receiver and
argument list therefore falls into **one behavior equivalence class**, related
to native unit input. Evaluating that one class fixes the entire body result and
empty effect trace. This is the completeness argument; it does not follow from
the runtime witness count. Any drift outside these forms invalidates this
particular proof and requires a new argument even if some fixtures still pass.

The runtime floor adds 21 receiver witnesses, including every primitive kind,
lone-surrogate strings, a throwing proxy and a revoked proxy, with zero and extra
arguments: **42 calls**, stopping at `revoked_proxy/extra_args`. Source values
are asserted directly before JSON capture; native enum identity is asserted
before any wire comparison. The independent Rust ID/count inventory prevents a
shrinking source corpus from satisfying the check. An outside queued microtask
must run after all synchronous calls. These witnesses test execution provenance
and instrumentation; they do not replace the one-class argument.

## Falsifiers, measurements and cost

The control floor stops at four: native wrong variant, source wrong string,
native added environment read, source added global property read. The first two
must fail **runtime value** assertions. The latter two must invalidate the
**structural proof guards**, and are not reported as measured runtime effect
observations. Production bytes are restored in `finally`, and each same target
must pass after restoration. Surviving/invalid controls are failures, not omitted
from the report.

The tracked gate sheet records the exact current input SHA, runtime versions,
commands/exits, restored hashes, scope and measured lane time. No old test result
is relabelled as current. The lane clock began at **2026-10-15 10:56:30 UTC**,
before candidate selection and module-load probing. Cost includes selection,
proof definition, implementation, compilation, controls, reviews and delivery
preparation; it is wall time, not CPU or billed model time. This is a minimal
closed constant-body case, not an estimate for REST, mutable state, crypto or
asynchronous row proofs.

Authoring integration parent: `694e07f7e049a5a7047c41e994a0fc854dc78a59`.
The earlier unmerged census/audit branches are not authoring ancestors.

## Initial measured result

**One complete mapped callable body established**, under the contract and trusted
foundations above, at `cf64733d9bc9a5f58763e18cf5a885081935c4e9`.
The integration fetch before measurement was still `694e07f7e0`; merging it
reported already up to date. The new native test uses real Rust 1.99.0 on
`aarch64-apple-darwin`; the fully loaded TS module runs in Node 24.20.0 / V8
13.6.233.17-node.53, with TypeScript 6.0.3 used for structural analysis.

| Gate at the measured input | Exit / outcome |
| --- | --- |
| Scoped Polymarket all-targets `cargo check --locked --jobs 2` | 0 |
| `name_complete_current_runtime` baseline | 0; one native unit-input class, 42 source calls |
| Native wrong variant | 101 at `name_complete_native_value`; restored 0 |
| TS wrong string | 101 with actual getter `name_complete_source_value`; restored 0 |
| Native added state read | 101 at `name_complete_rust_body_schema`; restored 0 |
| TS added state read | 101 at `name_complete_source_body_schema`; restored 0 |
| Existing `spot_contracts` target | 0; two tests passed |
| New Rust proof-file formatting | 0 |
| Maintained extractor check | 0; output says 0 manifest updates, 0 added/orphaned |
| Diff check | 0 |

No selected control survived; enumeration stops at control four. The two
closure-guard failures do not claim runtime observation of those added reads.
Both production files were restored byte-for-byte. Sub-agent review inspected
the committed input, retained failure/restoration output and hashes; both
reviewers returned MERGE within this proof's scope. Reviewers reran no gates.

Measured cost: **22.07 lane wall minutes to readiness**, including selection,
proof design, implementation, preflight compile corrections, static reviews,
measured gates and cache cleanup. This excludes the subsequent evidence
commit/push/PR mechanics; it is not CPU, billed time or summed reviewer effort.
The measured controls used 205.116 seconds across their nine Cargo invocations.
The scoped check took 84.679 seconds. Two uncommitted native preparation attempts
failed on the Syn 3 `Safety` API before correction; those are not discrimination
controls or exits at the measured input SHA. The original 242-row cohort remains
0/242 complete, and the venue triple remains **107 ported / 18 divergent /
16 pending**. Status rows moved: **0**. About 500 MB of this worktree's disposable
incremental cache was removed after compilation completed.

See [result.json](result.json), the [gate sheet](gates-cf64733d9b/gates.json),
[control receipts](gates-cf64733d9b/controls/controls.json) and
[review record](reviews.md). Logs are retained and sanitized for publication;
input bytes and log hashes are recorded. Production behavior is unchanged.

Not run in the original sequence: full venue suite, workspace build, host
clippy, wasm/browser runtime/clippy, live venue/network checks, SDK instance
construction/registry/FFI or CI. See the later requested Clippy supplement. This is an entered-body calibration, not end-to-end SDK integration.

## Current-head merge and final cost

Integration advanced during delivery. It was merged cleanly from
`8fcf323edfc6747fcdf4427ce19267a7147d018b`, producing measured current input
**`e9a093ba7979c49d0ce2184089f563ecb8f205b6`**. All 12 direct proof-input hashes
match. A separate diff verifies preservation of the entire Polymarket crate,
TS source, actual enum declaration, Cargo manifests/lock and package/yarn inputs. Changed
shared core implementations do not occur in the literal getter or unit-value
construction; sub-agent review inspected that preservation argument.

Because shared dependencies changed, the scoped all-targets check and real
paired runtime were rerun at the merge SHA: **check 0; name proof test 0 (one
passed); existing spot tests 0 (two passed); extractor 0 (zero manifest updates);
preservation diff 0; diff check 0**. The four controls were **not rerun**: their
0/101/0 results remain explicitly attached to `cf64733d9b`, with unchanged
kernel/control sites. No control exit is relabelled as a merge measurement.

The final readiness checkpoint cost was **35.81 lane wall minutes**, including
the integration merge, fresh gates, preservation review and second cache cleanup.
The initial readiness checkpoint was 22.07 minutes; those are separate measured
checkpoints. Final evidence commit/push/PR mechanics occur after this clock stop.
About another 544 MB of incremental cache files was removed after compilation.

The current-head proof still establishes **one mapped callable body outside the
242-row cohort**, under the exact contract above. This slice changes zero of
that cohort's complete counts and zero manifest status rows. See the
[merge gate sheet](merge-e9a093ba79/gates.json) and [delivery/cost record](delivery.json).


## Requested Clippy supplement

Measured input **3acd4d3fe1604238a95f328cb7fa9c2e55681766**, after merging
integration parent **49a942e76143d6d1d44567edff59c12669d92627**. No Rust,
TypeScript, Cargo manifest/lockfile or wasm helper/policy body changed from
previous published head d545ab71f0d6024666cd1d40a7ad937181c3a61c. All twelve
direct proof-input hashes match the earlier receipt.

| Gate | Exit | Wall seconds |
| --- | --- | --- |
| cargo clippy --manifest-path sdk-rs/Cargo.toml --workspace --all-targets --locked --jobs 2 | 0 | 1503.847 |
| node .github/scripts/rust-wasm.mjs --clippy | 0 | 2497.397 |
| node $YARN_JS parity:extract:check | 0 | 43.627 |
| compiled-source preservation diff | 0 | not timed |

The wasm helper validates the policy and checks all 38 declared wasm packages,
all targets, wasm32-unknown-unknown and safe-cex-ffi/wasm. The native-only parity
CLI is excluded by that policy and included by host workspace Clippy. These are
lint/compilation checks, not wasm execution. Cargo invocations were serial and
used jobs2. Both Clippy commands together took 66.69 wall minutes in this cache
and fleet-load state; this later supplement is **additional to**, not hidden
inside, the original 42.41-minute handoff milestone. It is shared workspace
validation cost, not a measured per-function proof lower bound.

[Gate exits and commands](clippy-3acd4d3fe1/gates.json),
[extractor text attestation](clippy-3acd4d3fe1/extractor.json), and
[input preservation](clippy-3acd4d3fe1/preservation.json) are tracked. New raw
stdout stays locally ignored under the merged artifact policy; the receipts pin
its sanitized hashes, and sub-agent review inspects the local logs.

The original four controls remain measured at cf64733d9bc9a5f58763e18cf5a885081935c4e9;
they were not rerun for this supplement. Earlier runtime proof remains measured
at e9a093ba7979c49d0ce2184089f563ecb8f205b6 with unchanged proof inputs.
No new full venue/workspace tests, workspace build, host all-features Clippy,
standalone wasm build, browser/wasm runtime, live venue or CI run is claimed.
Status rows moved: **0**. Polymarket tomllib census: **107/18/16** across seven
manifest files, stopping at polymarket.ws-public.ts.toml.
