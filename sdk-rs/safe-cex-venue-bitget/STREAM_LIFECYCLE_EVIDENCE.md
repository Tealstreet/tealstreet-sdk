# Synchronous private-stream lifecycle bodies

Moved **4 rows pending→ported, 0→divergent**. Python 3.11 `tomllib` measured
all eleven Bitget manifests against integration parent
**`3e4e053b47fbfa44475712b68fdf53c23f5071cd`**:
**42 ported / 80 divergent / 105 pending → 46 / 80 / 101**,
with independently expected 227 unchanged identities. The four rows are dispose,
markPrivateStreamMessageReceived, startPrivateStreamDegradedReconcile and
stopPrivateStreamDegradedReconcile. This is a census, not full venue proof.

Tested code SHA: **`01563578d2cf33ee592e39cf453647037c909a42`**.
Its actual immediate parent is the integration SHA above; no unreviewed lane
branch was used as an ancestor. Evidence-only commits follow the tested SHA.

## Measured gates at the tested code SHA

| Gate | Exit and observed result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | **0**, finished dev profile, zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test stream_lifecycle --jobs 2 -- --include-ignored` | Restored **0**, two passed, zero failed/ignored, including maintained-source capture |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | **0**; complete output says 7341 symbols, would update zero manifests, zero added/orphaned |
| Scoped rustfmt check of new production/test source | **0** |
| `git diff --check`; restored tracked crate/parity diff to tested SHA | **0**; **0** |
| Independent Python 3.11 tomllib census | **0**, four pending→ported changes, eleven files/227 identities |

The extractor writer after manifest editing also exited **0**, reporting zero
updated manifests and zero added/orphaned symbols. Metadata arrays/provenance
were written through parseManifest/serializeManifest. Cargo ran sequentially;
Rust PATH included `/opt/homebrew/opt/rustup/bin`. Source execution used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

## Native proof and discrimination

The independent floor is **69 cases: eleven successful guard combinations plus
58 injected receiver/call failures**. It stops at stop-reconciliation, truthy
timer, failure index 3 (the final undefined assignment). Each actual source
initializer executes in a real subclass preserving lexical super dispatch,
with receiver properties and called operations supplied. Native assertions cover
decoded booleans, socket identities, timeout read counts and original second-read
timer identities, own undefined health-property presence, unprojected timestamp
identity/UTF-16 payload, original error identity/payload, and writes before/after
failure. Every native assertion finishes before output projection or comparison.
Source arguments and the independent 69-case count are checked before outputs.

Each deliberate production break compiled, exited **101**, and failed its named
native assertion; all edits were restored byte-for-byte. The restored target
then exited **0**, two tests passed. No control survived.

| Break | Named assertion |
| --- | --- |
| Swap second/third socket disposal | `STREAM native socket identities` |
| Replace own undefined health property with absence | `STREAM native own undefined health property` |
| Change running/timeout OR guard to AND | `STREAM native timeout read count` |
| Omit timer assignment to undefined after clear | `STREAM native timer slot value` |

## Review and scope

Reviewer **`/root/account_read_review`** inspected committed blobs at
**01563578d2cf33ee592e39cf453647037c909a42** and found no blockers or new real
divergence preventing the four supplied-operation bodies from being ported.
It ran source capture and independent Python 3.11 checking, both **0**; checked
69 cases, four mapping targets and 22 individually named plumbing exemptions.
It did not run or certify Cargo, extractor, mutations, Clippy, wasm or CI.

The host boundary preserves fallible property/call operations and opaque values;
it does not implement or exempt the called SDK operations. In particular,
launch_rest_reconcile represents synchronous invocation with host-owned detached
work, not awaiting or implementing REST reconciliation. Health update preserves
the generated own undefined argument; no omitted/null substitution is made.
Full BitgetExchange construction and its concrete bindings, full Base disposal,
child socket implementation, Store behavior and REST reconciliation are separate
claims. No full venue/workspace suite, semantic TypeScript check, standalone Rust
invention checker, host/wasm Clippy, wasm build, live calls or CI was run.
The 58 selected failure seams are a floor, not enumeration of arbitrary global
accessors, receiver objects, reentry or every dependency behavior.

## Merge-down receipt

Fetched and merged latest integration
**`3788c309f1edfa70981c48886460149cb503f945`**. Merge code SHA
**`4c00e30a2166b6b729d8d19061750d57b3fcb854`** has first parent
`f3896e3bcf386c3bc3f9cb523b368110ea93a37c` and that integration second parent.
The only conflict was appended exemption records; both sets were retained,
parsed, and checked for duplicate targets. There was no CLAUDE conflict.
Required own-crate src/tests diff against **01563578d2** exited **0**.
The four discrimination controls remain measured at 01563578d2, with their
implementation and native assertions unchanged; no mutation rerun at this merge.

Shared dependencies/lockfile changed, so these gates were rerun at **4c00e30a21**:

| Gate | Measured exit |
| --- | --- |
| Exact scoped Bitget all-targets check above | **0**, zero errors |
| Exact lifecycle test command above | **0**, two passed, zero failed/ignored |
| Exact required extractor check above | **0**, 7341 symbols, would update zero manifests, zero added/orphaned |
| Python 3.11 tomllib census versus latest integration | **0**, same eleven files/227 identities; 42/80/105 → 46/80/101, exactly four pending→ported and zero divergent moves |

No shared-crate standalone check, full suites, Clippy, wasm, live calls, semantic
TypeScript check, standalone Rust invention checker or CI was run at this merge.
The review citation remains the tested implementation SHA 01563578d2; this
conflict resolution does not claim review of unrelated integration changes.
