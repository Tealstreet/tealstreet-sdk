# Frozen TypeScript references and Rust regression baselines

Status: read-only audit and implementation proposal. No harness, workflow,
package or venue implementation changes are authorized by this document.
Source inventory begins at actual landing
`d05c5ede4b86409e1a8d9798e64b79b1c1c888f4` (2026-10-23).

## Work plan

1. Inventory each live TypeScript execution dependency in CI replay/parity,
   native protocol oracles, audited fixers, recorder/input generation, performance
   and memory measurements, and supporting package tests. Distinguish executable
   TS venue code from ordinary TS infrastructure that remains after venue deletion.
2. Define an immutable, content-addressed frozen-reference bundle: exact tapes,
   case roster/options, complete raw outputs and per-case outcome/gate evidence,
   original TS source/tool/build provenance, accepted corrections/variants and
   integrity/retention rules. Preserve failed cases and gaps; a freeze is no pass.
3. Define independent Rust-vs-frozen-TS compatibility and Rust-vs-Rust regression
   checks, including checkpoint/command/request evidence, declared behavior changes,
   new-case bootstrap, performance policy and fail-closed source/schema binding.
4. Sequence baseline capture, dual-run validation, corruption/negative controls,
   immutable publication, TS-free rehearsal and eventual venue/reference deletion.
   Keep the long-lived frozen TS rollback package independent from active support.
5. Obtain independent correctness/adversarial plan review and record bounded
   verification plus open decisions. Any later harness implementation is a separate
   authorized PR with original-RED controls; this phase changes docs only.

Maintain all five history CI/proof watches and the current owner merge queue while
preparing this proposal. The initial #1769 hold has been superseded by the #1741
exclusive window. Do not run local release WASM, full replay, size or timing sweeps.
Corepack Yarn only; never bare/Volta Yarn, npx or the account-policy check.

## Recommendation and boundary

Keep three independently reported checks: **compatibility with frozen TS
behavior**, **regression against an approved Rust baseline**, and **native
protocol/financial/ownership invariants**. None replaces another. A reproducible
known TS defect stays a recorded strict failure or an explicitly audited
correction; freezing it does not establish parity. New Rust behavior needs an
explicit reviewed contract, not automatic expected-output regeneration.

Executable TS venue support ends after cutover; the long-lived frozen rollback
package is a separate deliverable, not dual maintenance or the active CI oracle.
Keep executable references through capture and rehearsal, then remove them only
when their replacement coverage is proved. This proposal does not authorize TS
deletion, change performance thresholds, or claim cutover/runtime readiness.
[Current dispositions](RUST_MASTER_MERGE_BLOCKERS.md),
[rollback boundaries](RUST_ROLLBACK.md).

## Current dependency inventory

Anchors below describe the pinned source, not a deployed system. CI has **32
suites / 110 cases / 50 venue names**, with 42 public and 68 private cases
([roster](../../sdk-rs/harness/ci-gate-suites.mjs#L4)). Public/private classification
comes from tape metadata and replay policy, not `books !== ''`: Binance's
scripted private lifecycle case requests a book. At nine public performance
samples and one private correctness run this produces 976 raw outputs, 488 per
implementation. That output count is a capture profile, not a future universal
constant; effective runs and every expected slot belong in the manifest.

| Surface | Live TS dependency today | Required replacement / retained coverage |
| --- | --- | --- |
| Every strict replay matrix suite | [gate.mjs:60](../../sdk-rs/harness/gate.mjs#L60) starts TS with `tsx`; [the run loops](../../sdk-rs/harness/gate.mjs#L120) execute both implementations for correctness and public performance. [replay.mjs:657](../../sdk-rs/harness/replay.mjs#L657) imports the selected venue class. | A central frozen-reference reader plus a Rust candidate replay; keep the exact case roster, positive/negative command results, checkpoints and request/subscription/write evidence. |
| Venue and reference catalog | [venues.mjs:53](../../sdk-rs/harness/venues.mjs#L53) expands 50 aliases through 46 module/class pairs; [Orderly](../../sdk-rs/harness/venues.mjs#L279), [Coinbase](../../sdk-rs/harness/venues.mjs#L773) and [TxFlow](../../sdk-rs/harness/venues.mjs#L839) load test-reference classes too. | Freeze these transitive sources as provenance; split runtime Rust metadata/protocol hooks from deleted TS constructors. Removing only production venue folders is insufficient. |
| Audited TS corrections | [gate.mjs:220](../../sdk-rs/harness/gate.mjs#L220) builds corrections; [oracle-audit.mjs:6](../../sdk-rs/harness/oracle-audit.mjs#L6) audits writes. Raw duplicate/coverage and checkpoint checks precede correction ([gate.mjs:224](../../sdk-rs/harness/gate.mjs#L224)); Rust may not be mutated. | Preserve original raw TS and a separately labeled corrected expectation, transform/contracts/source digests, overwrite counts and errors. Retain auditable pure correction logic without deleted venue imports; never apply a fixer twice or manufacture expected output from Rust. |
| CI certification and known debt | [record-gate.mjs:10](../../sdk-rs/harness/record-gate.mjs#L10) collects, [verify-gates.mjs:7](../../sdk-rs/harness/verify-gates.mjs#L7) validates ledger/coverage, and [known-failure-policy.mjs:73](../../sdk-rs/harness/known-failure-policy.mjs#L73) requires live TS/Rust sample contracts. Known FAIL is explicitly not parity. | New mode/schema-aware shared certification; do not merely omit the TS subprocess or synthesize missing samples. Preserve exact known-diagnostic declarations and protected integrity checks. |
| Public CPU and memory gates | [performance-policy.mjs](../../sdk-rs/harness/performance-policy.mjs) compares fresh interleaved pairs: at least seven, nine for small windows; current 10% and small-window rules use TS observations. [gate.mjs:309](../../sdk-rs/harness/gate.mjs#L309) also binds performance to the audited workload. | Historical TS samples remain descriptive. Execute approved prior Rust and candidate afresh on the same runner under a separately approved regression policy; retain workload and measurement integrity. Private metrics remain informational. |
| Protocol/crypto validators | [ApeX](../../sdk-rs/harness/apexomni-protocol.mjs#L24) has an independent verifier and a TS-only setup import; [Lighter](../../sdk-rs/harness/lighter-crypto.mjs#L11) distinguishes TS crypto setup from checked-in Go-WASM verification. Coinbase uses Node crypto; other protocols use independent crypto packages. | Keep signature, native-ID, request/ACK, socket, accounting and transport validators. Remove TS-only preparation imports from Rust execution paths; retain shared crypto infrastructure where still needed. Saved outputs cannot replace native truth. |
| Recording and new tapes | [record-ts.mjs:13](../../sdk-rs/harness/record-ts.mjs#L13) invokes the TS public recorder; Rust recording already exists in [record.mjs](../../sdk-rs/harness/record.mjs). | Finish required TS-origin rerecording before the freeze. New post-deletion captures use a supported Rust recorder and independently reviewed native contracts; they cannot acquire fictitious frozen TS coverage. Live capture remains separately owner-authorized. |
| Harness and SDK tests | Harness tests execute TS classes/sockets/signer/entropy in `comparison-checkpoints`, `replay-startup`, `record-ts`, `lighter-replay-random`, `coinbase-replay-clock`, `asterdex-clock`, `bybit-products` and `okx-balance-reference`. Root CI also runs broad SDK tests/typecheck/lint. | Migrate each active behavioral assertion or retain its independent equivalent before removing its import. A static scan finds 343 of 621 SDK test files with quoted legacy-venue paths, plus 35 with reference paths; these include mocks/source checks, not 378 files safe to delete. |
| TS-language SDK infrastructure | [replay.mjs:658](../../sdk-rs/harness/replay.mjs#L658) uses DefaultStore; [line705](../../sdk-rs/harness/replay.mjs#L705) uses ExchangeWorker. Rust SDK construction still imports helpers from legacy venue directories. | “Delete TS venues” does not mean delete TypeScript or the Rust SDK wrapper. Relocate required product/types/helper modules first; prove no dangling imports. Review native/store/worker integration tests individually. |
| Native/WASM, capabilities, size, browser | Rust native tests, actual-WASM host/capability tests, size budgets and browser module readiness chiefly consume Rust artifacts. Some bundled product checks also import TS reference classes. | Keep these independent gates. Audit mixed script bundles rather than deleting a whole job. Browser warning cohorts and fixed size budgets retain their own policies; a replay freeze does not supersede them. |

The inventory deliberately distinguishes test dependency candidates from executed
TS constructors. Capture a machine-readable transitive import/dependency closure
in the implementation phase; a grep or a successful snapshot comparison is not
proof that deletion is safe.

## Proposed immutable artifact contract

Use separate versioned **TS-reference** and **Rust-regression** manifests. Commit
small reviewed root digests/metadata in the repository; store bulk evidence in
durable content-addressed storage with an independently readable replica. Do not
rely on Actions expiry: current strict artifacts last 14 days and CI engines
three days ([workflow](../../.github/workflows/rust.yml#L547)). Never select
`latest`, silently regenerate a missing baseline, overwrite an ID, or require
expired live API metadata to verify an already attested archive.

Proposed layout (names/schema are a design, not an implemented API):

```text
references/<kind>/<manifest-sha256>/manifest.json
references/<kind>/<manifest-sha256>/attestation.json
objects/sha256/<digest>              # immutable original bytes
```

| Manifest component | Required contents |
| --- | --- |
| Identity / trust | Closed schema and canonicalization versions, kind, parent manifest, repository identity, exact producer head/tree/parents, trusted workflow identity/blob, run/attempt/physical job, checkout/log digests and reviewed approval/attestation. Detached attestation signs the canonical manifest digest; no self-hash. SHA proves integrity, not origin. |
| Source / execution closure | TS venue/reference/helper and lockfile closure; harness/roster/tape/protocol/fixer/host/comparison hashes; Node/Corepack/Yarn/TSX and Rust/wasm tools, OS/architecture, target/family/profile/features/flags, clock/entropy and effective options. Retain reproducible source and inputs, not just their hashes. Existing [ci-wasm build.json](../../sdk-rs/harness/ci-wasm.mjs#L70) is one component; its build-input list is not a TS capture fingerprint. |
| Cases / slots | Exact ordered case IDs derived from suite, venue/alias/network/profile, tape path and byte SHA, literal books/options/private classification, epoch/entropy, command plan, mode/effective runs and sample ordinal. Store exact compressed tape bytes plus decoding metadata. Require the whole roster, every slot exactly once, no substituted case or unknown extra. |
| Original observations | Byte-exact raw JSON for every TS and approved Rust capture, including original metrics, final state, duplicate rows, checkpoints, commands/errors/outcomes, request/write/subscription order, signatures/protocol failures and transport receipts. Keep lengths and SHA of every object and an independently computed whole-nonmetric digest excluding **only top-level metrics** under a versioned rule. Do not discard nested evidence. |
| Derived compatibility view | Separately labeled corrected TS expectation, audited transform source/contracts/parameters, exact operation/count/error report and input/output hashes. Preserve corrections derived from tape declarations as such; they are not direct observations of executable TS. Native independent invariants remain separately versioned. |
| Gate / debt / coverage | Original full gate JSON, ledger exit, ordered complete failure strings, fixers and exact known-diagnostic manifest; explicit complete/incomplete and timeout evidence. Archive combined historic PASS/FAIL unchanged. New frozen correctness certification recomputes its own status; it cannot relabel an old failed combined gate parity PASS or reuse old CPU failures as current measurements. |
| Variants | Only independently authenticated **whole-case** alternatives with their corresponding full gates and producer evidence. The four currently documented classes are ApeX/TxFlow request multiplicity and Backpack/WOO checkpoint-zero books. Choose a complete recorded variant, never mix leaves or accept a third variant automatically; no Rust variant exemption. |
| Object inventory | Unique normalized member path/type/length/digest, byte format and compression. Strictly reject duplicate JSON keys, nonfinite numbers, duplicate/path-traversing/symlink archive members, missing or extra objects and conflicting case/slot declarations. Retain provenance objects and logs for offline verification. |

At promotion, authenticate live Actions repository/run/head/event/attempt/job,
physical checkout, artifact identity/digest/bytes and source bindings, then seal
that evidence under the reviewed trust root. Consumers subsequently verify the
pinned attestation and archived objects offline. Existing historical control
links or summaries are discovery pointers, never a promotion certificate.

For pre-#1762 controls, retain original failures and any separately attested order
projection: only unchanged whole raw, identical status/failure multiset/fixers and
exact current UTF-16 sort qualify under the standing rule. Preserve both original
and derived bytes/rationale. This does not normalize current candidate output or
permit a fieldwise waiver.

## Rust regression and performance contracts

The behavioral Rust baseline stores approved whole outputs for the same exact
cases/schema. Default comparison is strict whole nonmetric output, preserving
array order and every nested field. A case missing from an anchor is **uncovered**,
not a pass. Freeze case-specific nondeterminism out of the Rust runner where
possible; any genuine remaining Rust instability is a blocker requiring a
separate explicit policy, not inheritance of the four TS classes.

An intended behavior change is a separate reviewed baseline revision with the
full old/new diff, source rationale and independent protocol/native regression
proof. The candidate never writes its own expected outputs in the certification
path. Routine green CI cannot auto-approve changed behavior or remove old cases.
Keep the previous immutable anchor and report compatibility, regression and
native-invariant results separately. A known compatibility FAIL cannot excuse a
new Rust regression or any protected workload/signature/coverage failure.

For performance, execute an approved prior Rust package and candidate on the
same current runner, alternating order and retaining all samples. Use matched
package/glue/host/ABI and a common versioned measurement/workload contract;
include cold loading, main-thread/steady/process CPU, idle-loaded retained memory,
WASM memory and egress mode. Each side must reproduce its own audited correctness
and transport workload. Inability to execute an old ABI is a failed/unavailable
comparison needing an explicit new anchor, never permission to skip the gate.

Do not bypass candidate [current-input fingerprint validation](../../sdk-rs/harness/ci-wasm.mjs#L95)
to load the old module. Introduce a separate trusted-baseline loader bound to the
old approved manifest/package hashes and its compatible host. Historical TS
metrics may be displayed as historic evidence only; feeding them into today's
`performanceFailures` would falsely claim a fresh paired TS advantage. Preserve
the current live TS performance gate until an owner-approved Rust regression
threshold/floor/absolute-budget policy and its negative controls are ready.
This proposal intentionally does not invent accepted replacement percentages or
silently require 10% improvement over the previous Rust build on every PR.

## Cutover order and acceptance gates

1. **Inventory and close coverage prerequisites.** Enumerate all CI cases and
   transitive executors/imports, finish required TS-origin tapes and independent
   goldens/native regressions. Preserve live TS gates and frozen rollback custody.
   Missing command/financial/native evidence stays visible; no cutoff date alone
   turns a known failure into complete coverage.
2. **Capture and attest final anchors while TS still runs.** Use trusted exact
   source/package inputs and the complete roster, retaining every strict FAIL,
   timeout, raw slot, fixer and independently authenticated whole variant. Copy
   artifacts/provenance before expiry; verify a second durable copy. A timeout
   can be frozen as diagnostic evidence but cannot be a complete output golden.
3. **Implement shared readers/comparators in a separate source PR.** One central
   closed manifest validator, Rust baseline loader and certification policy;
   no per-venue fallback. Negative controls must fail for wrong trust/hash/input/
   tape/options/ABI, missing/duplicate cases or slots, nested command/checkpoint/
   transport edits, changed outcomes, widened fixers/diagnostics, variant mixing,
   Rust drift, candidate-authored baselines and incomplete/performance work.
4. **Dual-run without weakening current gates.** New frozen correctness results
   must reproduce the same correctness component of live TS comparison, coverage,
   corrections and known-debt classification for every complete case/variant.
   Keep historical combined/performance status separate. Run approved prior Rust
   versus current Rust too; approve the new performance policy independently.
5. **Prove TS-free operation before deletion.** In a clean CI rehearsal make TS
   venue/reference modules unavailable and run all replay/certification, protocol,
   native/WASM/capability, SDK/worker, test/lint/typecheck, size and browser jobs.
   Hidden loader/crypto/helper imports must fail the rehearsal until migrated.
   Do not mask them by removing test globs or dropping a suite. No deployment or
   trading is part of this rehearsal.
6. **Switch CI, then delete only covered executable references.** After reviewed
   artifacts, negative controls and rehearsal, replace active live-TS gates with
   frozen compatibility plus Rust regression/invariants/performance gates. Keep
   the separately archived frozen TS rollback package long-term. Remove active
   venue/reference code and obsolete-only tests/imports in reviewable batches,
   retaining replacement assertions. Rollback of the gate uses the prior manifest
   and checker version, not mutation of an existing bundle.
7. **Maintain new coverage honestly.** New tapes, options, product/account shapes,
   commands or output schemas need independently documented/captured/native
   expectations and a reviewed Rust baseline. Report no TS-reference coverage
   when none exists. Rebaseline tooling, features, ABI or runner cohorts explicitly;
   never let a missing golden trigger capture of the current candidate as truth.

Open implementation decisions: durable storage/trust-root custody and retention;
final capture revision/coverage closure; owner-approved Rust performance limits
and runner cohorts; baseline-compatible ABI/host packaging; promotion ownership
and exact replacement inventory for executable TS tests. These are planning
outputs, not permission requests or implemented selectors.

Architecture gate: freezing/comparison/promotion is a cross-cutting harness
concern. Implement one shared contract and reader, with native invariant modules
retained; do not introduce venue-local baseline loaders or exception policies.
This docs-only phase changes no executable gate, source, workflow or package.

Audit checkpoint: independent correctness, feasibility and adversarial reviews
found no blocking gap. Static verification checked the roster/dependency counts,
relative links and source anchors, document fences and the two-document scope.
No local replay, release WASM, performance sweep or harness implementation ran.
