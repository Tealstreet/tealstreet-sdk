# Apex oracle dependency gate recovery

The actual parity job [38681105139 / 116245641861](https://github.com/Tealstreet/tealstreet-next/actions/runs/38681105139/job/116245641861)
passed Coinbase and the repaired registry equivalence step, then failed at
`Check apexomni live TypeScript conformance`. Its Apex exchange binary reported
53 passed and six failed: three API oracle startup failures importing
`@noble/hashes/hmac.js`, and three trading startup failures importing
`bignumber.js`. These are dependency-environment failures before native assertions.

The shared `ci-deps.cjs` generator omits three direct maintained Apex dependencies:
`@noble/hashes`, `bignumber.js`, and `@ethersproject/bignumber`. The hoisted Noble
1.x package supplied transitively by ethers cannot satisfy the maintained source's
2.x subpath imports. The repair must retain the selectors from
`packages/safe-cex/package.json` and the existing frozen lockfile.

The [diagnosis record](evidence/ci-oracle-gate-recovery/diagnosis.json) binds the
measurements to their revisions and source hashes. Both unchanged real source
scripts fail with exit **1** at archived integration merge base `ac0f5fccf2` and
head `7b00aabd3e`, against a fresh bundle produced by the current generator. The
scoped diff of generator, Apex source/oracles, venue dependency manifest and
lockfile between those revisions exits **0**. This establishes an upstream
bundle fault; the Coinbase recovery lane owns the repair.

A candidate generator containing all three maintained selectors produces a fresh
frozen installation with exit **0**. The unchanged source API and trading scripts
then exit **0**, yielding 27 and 13 cases respectively. These are source-oracle
measurements, not the eight-target native Apex gate. The candidate regression
suite against the current generator exits **1**: the three named direct-selector
assertions fail, and the byte-exact lockfile/package-manager assertion passes.
The original installation's output reported completion, but its exit was not
captured; it is not reported as a measured pass.

Sub-agent static review found no repair blocker and verified that the candidate
covers all ten literal Apex dependency roots. That enumeration stops at literal
imports; it does not prove every dynamic transitive load. No Cargo or tests were
run by the reviewer. At that diagnosis point, the candidate code still needed application,
committed-head positive tests, actual omission controls, and the full native
Apex gate.

The integration parity job at merge `f2229103d0` was cancelled. The later
integration job at `f853d56fd2`, run 38692425805 / job 116281188601, passed
Coinbase and registry but failed the same six Apex dependency loaders. Its
conclusion remains failure. No CI rerun was requested.

## Committed dependency repair

At `0a6b621394`, the generator includes the three selectors from the maintained
venue manifest. The four focused tests run through the existing CI-wiring test's
new import; direct focused execution exits **0**, 4/4, on both Node 26.7.0 and
Node 22.23.2. The latter is a local Node 22 measurement, not the CI runner's exact
22.23.1 runtime. The later complete CI-wiring run at
`881dcb68c0dd0e25cfb5d801b4dc431f166b111e` exits **0**, 290 passed, zero
failed or skipped. The eight-target native Apex run at that revision exits
**0**, 144 tests passed, zero failed, ignored or filtered out. The unchanged
exchange target now passes all 59 tests, including the six previously blocked
by loader failures. These are local measurements; integration parity is not green.

Each of the three actual dependency entries was removed individually from the
committed generator. Each run exits **1**, with the named maintained-selector
assertion failing. Each restoration exits **0**, 4/4, and restores byte-identical
source. [The control record](evidence/ci-oracle-gate-recovery/dependency-controls.json)
records the code SHA, exact assertions, exits and hashes. This enumeration stops
after these three dependency-omission controls.

A fresh frozen installation generated from the committed repair exits **0**;
its lockfile bytes match the repository. Every one of the explicitly enumerated
15 Apex source scripts exits **0** and emits valid JSON on both local Node
runtimes. The [Node 26](evidence/ci-oracle-gate-recovery/source-floor-node26.json)
and [Node 22](evidence/ci-oracle-gate-recovery/source-floor-node22.json) records pin
the scripts and their hashes. This enumeration stops at those 15 scripts, before
native Rust assertions. The [committed gate record](evidence/ci-oracle-gate-recovery/committed-gates.json)
links the later native and full-wiring measurements to their tested revision.

A scoped diff against `43cabb625f` for all Rust sources, Cargo manifests and
lockfile, and the wasm runner/policy/tests exits **0** after applying this repair.
The serial workspace measurements remain evidence for that Rust code revision;
no Rust inputs were changed while those gates ran.

Sub-agent review of `7b00aabd3e..d3158f73bb` found no material blocker. It
independently verified all 30 source receipt hashes, the four archived base/head
script hashes, candidate generator/test hashes, the three original/restored
control hashes, and the lockfile hash. It also confirmed the actual CI import
and absence of laptop paths in the nine reviewed files. Review ran read-only
Git/Python inspections, not Cargo, metadata or tests. Its minor historical-tense
clarification was applied here; no human review gate was introduced.

The [workspace gate record](../safe-cex-registry/evidence/gate-recovery/workspace-gates.json)
contains all six completed broad gates and the three subsequent local phases,
each with its own measured code SHA. Broad Rust measurements cover the frozen
`43cabb625f` inputs; later integration Rust changes are explicitly outside that
measurement. Both host and policy-selected wasm clippy have measured exit **0**.
