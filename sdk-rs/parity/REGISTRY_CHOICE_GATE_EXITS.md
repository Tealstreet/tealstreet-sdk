# Registry choice gate exits

Original integration parent:4891e46a0770116f292342ece37e7c6809983859.
Merge-down integration:8fcf323edfc6747fcdf4427ce19267a7147d018b.
Measured delivery merge:d40068e664d3186456ad976ec787486740b2ff17.
No unmerged feature branch is an ancestor. PR7954 was separate at branch creation
and is included through its subsequent integration merge, not a feature stack.
This authored evidence is not generated test output. Temporary logs/captures stay
outside the repository; later evidence edits do not change measured code SHAs.

## Completed delivery gates

All scoped Cargo commands use `--manifest-path sdk-rs/Cargo.toml --locked
--jobs 2 -p safe-cex-registry` with `CARGO_INCREMENTAL=0`. Source tests use
SAFE_CEX_TYPESCRIPT_PATH. Every result below is measured atd40068e664.

| Command / scope | Actual exit | Completed observation |
| --- | ---: | --- |
| cargo test --test loaders --test native_load |0 |3 loader tests and2 native-wrapper tests; independent94-entry/47-descriptor floors, two new native schedules and17 source choice cases |
| cargo check --all-targets |0 |Selected crate/targets and dependencies, not workspace check |
| cargo clippy --all-targets -- -D warnings |0 |Scoped host lint |
| cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings |0 |Separate scoped wasm lint; empty feature list from .github/rust-wasm.json |
| cargo build --workspace --locked, from sdk-rs |0 |Whole workspace build, Finished dev profile in1m01s; CARGO_BUILD_JOBS=2 and incremental disabled |
| parity:extract writer, explicit YarnJS entry |0 |Output: Updated0 manifests,0 added,0 orphaned |
| parity:extract:check, same entry |0 |Output: Would update0 manifests,0 added,0 orphaned |
| rustfmt --edition2024 --check, changed Rust test file |0 |No format drift |
| git diff --check against integration8fcf323edf |0 |No whitespace errors |
| production TypeScript, .github and registry/src diff against integration |0 |No changes |

Writer/check output text was inspected; a shim exit alone does not establish
freshness. The Rust test command names two integration-test files, not a crate
or workspace test suite. Host and wasm remain separate claims. The build is the
exact requested workspace command, with its job limit supplied by environment.

## Deliberate controls

Controls1–7 use committed base364110ae000808a64822101bcb8504131f22e927.
No commit changed during those controls. Each compiled test failed its named
assertion with101, then passed the same filtered target with0 after bytewise
restoration. Source controls exercise the actual child-source pipeline; they are
not mislabeled as native runtime value assertions.

All use cargo test with the common flags above, `--test loaders`, and either
`every_loader_matches` or `chosen_caller_owned` as the exact finite filter.

| Iteration | Deliberate break | Named failure | Mutated / restored |
| ---: | --- | --- | --- |
|1 |Wrong web module literal |REGISTRY web import before poll |101 /0 |
|2 |Wrong native constructor export literal |REGISTRY native require and export before future return |101 /0 |
|3 |Wrong descriptor module literal |REGISTRY native descriptor module |101 /0 |
|4 |Poll settled import and look up its export during future Drop |REGISTRY CHOICE native dropped continuation omits export |101 /0 |
|5 |Source web loader discards non-function exports |REGISTRY CHOICE source admits null, reported by actual-source parent assertion |101 /0 |
|6 |Freeze actual source native registry initializer |REGISTRY CHOICE source registry permits replacement |101 /0 |
|7 |Replace source-choice oracle with a no-op |REGISTRY CHOICE completed source marker |101 /0 |

Control8 uses base d40068e664d3186456ad976ec787486740b2ff17. It swaps all47
native descriptor module/constructor values and swaps their serializer field
names together. The original serialized comparison survives: cargo test
`--test data registry_descriptors_and_registered_name_order_match_reviewed_snapshot`
exits0. This survivor is a finding at
safe-cex-registry/tests/data.rs::registry_descriptors_and_registered_name_order_match_reviewed_snapshot;
it is not quietly counted as a discriminating old gate. The new direct native
assertion in tests/loaders.rs::every_loader_matches_source_and_retains_native_identity_and_timing
fails101 at REGISTRY native descriptor module before any Rust serialization.
Restored old wire and new native targets both exit0. Existing fixture/helper
bytes are preserved; the supplement supplies the missing field-identity proof.
This is a test-coverage finding, not a new venue/runtime defect or a97th resolved
annotation. Enumeration ends at control8: five native controls and three
source/producer controls. No final combined gate accepted these mutations.

## Isolation, census and scope

At364110ae00, a tracked-source mirror without node_modules in any ancestor runs
both source oracles successfully using an existing isolated TypeScript6.0.3
compiler: original94-entry loader capture and new17-case choice supplement,
each exit0. A nonexistent compiler path exits1 with MODULE_NOT_FOUND. No new
installation is claimed. Captures use Node24.20.0 arm64; Node26 was not run.

Read-only tomllib reconciliation at364110ae00 preserves all512 manifest file
structures except exactly96 notes. Its expected cohort is independently built
from94 actual source entry observations plus the two source object identities.
Repeated atd40068e664 against integration8fcf323edf:512 manifests, exactly96
notes and zero other parsed differences. Both registry manifests contribute48.
The unchanged rust.yml retains93 named steps; no workflow is edited.
Statuses, targets, layers, row order and per-source Rust file declarations are
unchanged. The96 written owner rationales do not retrospectively make the old
unsupported annotations valid, and do not prove complete function conformance.

The native schedules are both unpolled: one host is unsettled and one settled.
A measured Poll::Pending observation belongs to the existing ordinary-entry
witness, not a retroactive claim about these new drop cases. The source discard
case drains four microtask turns. Export examples cover seven values at only
the first ApexOmni entry per platform; ordinary route evidence stops after all94
actual entries and47 descriptor field triplets. Two source object mutation
sequences and one discarded-Promise case complete the17-case source floor.

A bounded textual SDK Rust search for the module-host/constructor/table names
ends at six matching files: registry module_host, lib, loaders and loader tests,
plus facade factory and root-service tests. The facade defines a different
constructor type. No production concrete registry-host assembly is established;
external embedders and arbitrary alias/generic consumers are not enumerated.

Not run: separate wasm build/runtime job steps, all-feature compilation,
full crate/monorepo suites, real venue/network/browser or wasm execution, Node26,
the canonical parity-check binary or CI reruns. No diagnostic reduction is
claimed; no checker rule, exemption or severity changed.

## Last merge-down delivery

Integration advanced through49a942e76143d6d1d44567edff59c12669d92627.
It merged cleanly atb4abce756923be1a0d4765b97f5f892d37dc9b16; incoming changes
do not touch registry production/tests or dependency manifests. At that exact
merge, the exact workspace build above was repeated: actual exit0, zero warnings.
The extractor check was repeated: actual exit0, text reports no drift. Scoped
tests/check/host/wasm exits remain attached tod40068e664; they are not presented
as reruns on this last merge. Both integration parents are merged work.

Read-only adversarial sub-agent review independently reconciled the96 changed
identities and source94-entry floor, verified each of the eight control logs
and their measured SHA boundaries, and returned PASS after correcting the
reported filter to its actual `every_loader_matches` spelling. The reviewer
ran no Cargo gates. Earlier semantic review separately checked every selected
module/export mapping and the three priced choices; its unpolled-schedule
wording correction is included.

## Requested whole-workspace host lint

The reviewer-requested `cargo clippy --workspace --all-targets --locked` ran
from sdk-rs at exact merge commitb4abce756923be1a0d4765b97f5f892d37dc9b16.
HEAD remained detached at that commit throughout execution, with no tracked
changes. Actual exit:0. Cargo finished the dev profile in10m02s; inspected
output contains zero warnings and zero errors. CARGO_BUILD_JOBS=2 and
CARGO_INCREMENTAL=0 bounded the run; no second Cargo ran concurrently here.
This closes the workspace-host-lint gap for that delivered code tree, including
the new registry Rust tests. It does not rerun or replace the separate scoped
wasm result, and is not a full wasm policy-matrix claim. Subsequent evidence
commits change only this authored gate record, not Rust/TS/tests/workflows.

## Requested final-tree wasm workflow lint

The actual workflow invocation `node ../.github/scripts/rust-wasm.mjs --clippy`
ran from sdk-rs at exactb4abce756923be1a0d4765b97f5f892d37dc9b16, again with
detached HEAD and no tracked changes throughout. The helper's measured process
exit is0; its asserted Cargo child status is0. Cargo finished the dev profile
in20m01s, with zero warnings and zero errors in the inspected output.

This invokes cargo clippy with --all-targets, --locked, wasm32-unknown-unknown,
--jobs2, every one of the38 policy-selected packages and combined feature
safe-cex-ffi/wasm. CARGO_INCREMENTAL=0. The only policy native-only exclusion is
safe-cex-parity. The helper validates coverage and workflow before running Cargo;
it does not add a -- -D warnings argument. A read-only sub-agent independently
checked this exact command scope while it was running, without claiming an exit.

This is the full policy-selected wasm-clippy invocation, distinct from both
whole-workspace host clippy at this same commit and earlier strict registry-only
wasm lint atd40068e664. Separate wasm --build, extra all-feature exchange builds,
policy-test execution, transport warning runtime execution and browser/FFI smoke
were not run by this helper. It does not establish that the entire wasm CI job
passed. The subsequent commit edits only this authored gate record.
